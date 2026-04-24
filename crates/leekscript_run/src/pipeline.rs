//! Full parse pipeline for tooling and `lek run`.

use leekscript_directives::{parse_file_preamble, FmtPreamble};
use leekscript_hir::{
    lower_file, HirClassMember, HirExpr, HirFile, HirLoweringDiagnostic, HirStmt, HirSwitchClause,
};
use leekscript_lexer::{Lexer, LexerConfig};
use leekscript_parser::parse_file_green;
use leekscript_resolve::{resolve_hir_with_extra_globals, ResolveDiagnostic};
use leekscript_span::Span;
use leekscript_syntax::LeekLanguage;
use leekscript_types::{check_hir_types, TypeDiagnostic};
use rowan::SyntaxNode;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Max leading lines scanned for `// leek-*` (matches `lek check`).
pub const PREAMBLE_MAX_LINES: usize = 64;

/// CLI + manifest overrides for language settings (same precedence as `lek check`).
#[derive(Clone, Debug)]
pub struct CompileOptions {
    pub manifest: Option<PathBuf>,
    pub cli_language_version: Option<u8>,
    pub cli_strict: Option<bool>,
    /// Directory resolution for `include("…")` (typically the real path of the compiled `.leek` file).
    pub source_path: Option<PathBuf>,
    /// When compiling an included file, diagnostics use this path for snippets (spans are into that UTF-8).
    pub snippet_origin: Option<PathBuf>,
    /// Names from signature TOML ([`leekscript_signatures`]) merged into the resolve global scope.
    pub signature_globals: Vec<String>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            manifest: None,
            cli_language_version: None,
            cli_strict: None,
            source_path: None,
            snippet_origin: None,
            signature_globals: Vec::new(),
        }
    }
}

/// Phase that produced a diagnostic (for display and JSON consumers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompilePhase {
    Directives,
    Lexer,
    Parser,
    /// Rowan → HIR lowering (should be rare if the parse tree is well-formed).
    Hir,
    /// Lexical name resolution (duplicate bindings, undefined idents).
    Resolve,
    /// Type analysis (casts, typed parity checks).
    Types,
}

impl CompilePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            CompilePhase::Directives => "directives",
            CompilePhase::Lexer => "lexer",
            CompilePhase::Parser => "parser",
            CompilePhase::Hir => "hir",
            CompilePhase::Resolve => "resolve",
            CompilePhase::Types => "types",
        }
    }
}

/// One issue that prevented compilation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileDiagnostic {
    pub phase: CompilePhase,
    /// Registry / Java-style id (`INVALID_CHAR`, `unknown_leek_directive`, …).
    pub reference: String,
    pub span: Span,
    pub message: String,
    /// When set, `span` is a byte range in this file’s UTF-8 (not the root unit `src`).
    pub snippet_origin: Option<PathBuf>,
}

/// Successful parse: metadata plus the grammar-backed rowan root and lowered HIR.
#[derive(Debug)]
pub struct CompiledUnit {
    pub path_display: String,
    pub language_version: u8,
    pub strict: Option<bool>,
    pub token_count: usize,
    pub fmt: Option<FmtPreamble>,
    pub experimental: Option<Vec<String>>,
    pub syntax_root: SyntaxNode<LeekLanguage>,
    /// Statement-level IR for backends and analysis.
    pub hir: HirFile,
}

/// Outcome of [`compile_source`].
pub type CompileOutcome = Result<CompiledUnit, Vec<CompileDiagnostic>>;

/// Fully expanded top-level statements for one physical source file (nested `include`s already inlined).
///
/// [`ModuleExpansionCache`] stores these keyed by canonical path so each file is lexed / parsed / lowered
/// at most once per [`compile_source`] run. A future `import` mechanism can reuse the same cache type and
/// [`resolve_include_file`] for path resolution.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExpandedSourceUnit {
    pub stmts: Vec<HirStmt>,
    pub stmt_sources: Vec<PathBuf>,
}

/// Per-compilation cache: canonical source path → expanded IR.
#[derive(Clone, Debug, Default)]
pub struct ModuleExpansionCache {
    expanded: HashMap<PathBuf, ExpandedSourceUnit>,
}

impl ModuleExpansionCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, path: &Path) -> Option<&ExpandedSourceUnit> {
        self.expanded.get(path)
    }

    /// For future `import` or custom loaders: register expanded IR so later `include` / `import` skips work.
    pub fn insert_expanded(&mut self, path: PathBuf, unit: ExpandedSourceUnit) {
        self.expanded.insert(path, unit);
    }
}

fn manifest_language_settings(manifest: Option<&PathBuf>) -> (Option<u8>, Option<bool>) {
    let path = manifest.cloned().or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(leekscript_config::find_manifest)
    });
    let Some(p) = path else {
        return (None, None);
    };
    let Ok(m) = leekscript_config::LeekManifest::load_path(&p) else {
        return (None, None);
    };
    let Some(l) = m.language else {
        return (None, None);
    };
    let v = l.version.map(|x| x.clamp(1, 99) as u8);
    let s = l.strict;
    (v, s)
}

fn push_directive_diags(
    out: &mut Vec<CompileDiagnostic>,
    diags: &[leekscript_directives::DirectiveDiagnostic],
    snippet_origin: Option<PathBuf>,
) {
    for d in diags {
        out.push(CompileDiagnostic {
            phase: CompilePhase::Directives,
            reference: d.registry_id.to_string(),
            span: d.span,
            message: d.message.clone(),
            snippet_origin: snippet_origin.clone(),
        });
    }
}

fn push_lexer_diags(
    out: &mut Vec<CompileDiagnostic>,
    errs: &[leekscript_lexer::LexError],
    snippet_origin: Option<PathBuf>,
) {
    for e in errs {
        let msg = crate::lexer_reference_display_message(e.reference);
        out.push(CompileDiagnostic {
            phase: CompilePhase::Lexer,
            reference: e.reference.to_string(),
            span: e.span,
            message: msg.into(),
            snippet_origin: snippet_origin.clone(),
        });
    }
}

fn push_parse_diags(
    out: &mut Vec<CompileDiagnostic>,
    errs: &[leekscript_parser::ParseDiagnostic],
    snippet_origin: Option<PathBuf>,
) {
    for e in errs {
        out.push(CompileDiagnostic {
            phase: CompilePhase::Parser,
            reference: e.reference.to_string(),
            span: e.span,
            message: e.message.to_string(),
            snippet_origin: snippet_origin.clone(),
        });
    }
}

fn push_hir_diags(
    out: &mut Vec<CompileDiagnostic>,
    errs: &[HirLoweringDiagnostic],
    snippet_origin: Option<PathBuf>,
) {
    for e in errs {
        out.push(CompileDiagnostic {
            phase: CompilePhase::Hir,
            reference: e.reference.to_string(),
            span: e.span,
            message: e.message.clone(),
            snippet_origin: snippet_origin.clone(),
        });
    }
}

fn push_resolve_diags(out: &mut Vec<CompileDiagnostic>, errs: &[ResolveDiagnostic]) {
    for e in errs {
        out.push(CompileDiagnostic {
            phase: CompilePhase::Resolve,
            reference: e.reference.to_string(),
            span: e.span,
            message: e.message.clone(),
            snippet_origin: Some(e.source_file.clone()),
        });
    }
}

fn push_type_diags(out: &mut Vec<CompileDiagnostic>, errs: &[TypeDiagnostic]) {
    for e in errs {
        out.push(CompileDiagnostic {
            phase: CompilePhase::Types,
            reference: e.reference.to_string(),
            span: e.span,
            message: e.message.clone(),
            snippet_origin: Some(e.source_file.clone()),
        });
    }
}

struct PendingCompile {
    language_version: u8,
    strict_eff: Option<bool>,
    token_count: usize,
    fmt: Option<FmtPreamble>,
    experimental: Option<Vec<String>>,
    syntax_root: SyntaxNode<LeekLanguage>,
    hir: HirFile,
}

fn compile_pending_inner(
    src: &str,
    opts: &CompileOptions,
) -> Result<PendingCompile, Vec<CompileDiagnostic>> {
    let (manifest_version, manifest_strict) = manifest_language_settings(opts.manifest.as_ref());
    let (preamble, directive_diags) = parse_file_preamble(src, PREAMBLE_MAX_LINES);

    let mut failures: Vec<CompileDiagnostic> = Vec::new();
    let snippet_origin = opts.snippet_origin.clone();
    push_directive_diags(&mut failures, &directive_diags, snippet_origin.clone());
    if !failures.is_empty() {
        return Err(failures);
    }

    let file_lang = opts
        .cli_language_version
        .or(preamble.language_version)
        .or(manifest_version)
        .unwrap_or(4);
    let strict_eff = opts.cli_strict.or(preamble.strict).or(manifest_strict);

    let file_cfg = LexerConfig { version: file_lang };
    let (tokens, lex_errors) = Lexer::new(src, file_cfg).tokenize();
    push_lexer_diags(&mut failures, &lex_errors, snippet_origin.clone());
    if !failures.is_empty() {
        return Err(failures);
    }

    let root = match parse_file_green(src, &tokens) {
        Ok(r) => r,
        Err(errs) => {
            push_parse_diags(&mut failures, &errs, snippet_origin.clone());
            return Err(failures);
        }
    };

    let hir = match lower_file(src, &root, file_lang) {
        Ok(h) => h,
        Err(errs) => {
            push_hir_diags(&mut failures, &errs, snippet_origin.clone());
            return Err(failures);
        }
    };

    Ok(PendingCompile {
        language_version: file_lang,
        strict_eff,
        token_count: tokens.len(),
        fmt: preamble.fmt.clone(),
        experimental: preamble.experimental.clone(),
        syntax_root: root,
        hir,
    })
}

fn type_decl_leads_with_integer(decl_ty: Option<&String>) -> bool {
    let Some(t) = decl_ty else {
        return false;
    };
    t.split('|')
        .next()
        .map(str::trim)
        .is_some_and(|head| head == "integer")
}

/// `global integer NAME = <literal>;` entries (and comma-separated lists) for interpreter / constant seeding.
fn collect_global_integer_constants(hir: &HirFile) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for s in &hir.stmts {
        let HirStmt::Global { decl_ty, entries } = s else {
            continue;
        };
        if !type_decl_leads_with_integer(decl_ty.as_ref()) {
            continue;
        }
        for (name, init) in entries {
            if let Some(HirExpr::Integer(v)) = init {
                out.push((name.name.clone(), *v));
            }
        }
    }
    out
}

/// Result of parsing a single `.sig.leek` / `.sig.ls` unit with the normal pipeline (lexer → HIR).
#[derive(Debug, Clone)]
pub struct SigLeekUnit {
    /// Top-level names for the resolve pass (`function`, `global`, `var`, `class`).
    pub names: Vec<String>,
    /// Integer globals with compile-time literal initializers (`global integer X = 0;`).
    pub integer_globals: Vec<(String, i64)>,
}

/// Parse a signature file and collect both resolve names and integer constants (via HIR, not line regexes).
pub fn parse_sig_leek(
    src: &str,
    opts: &CompileOptions,
) -> Result<SigLeekUnit, Vec<CompileDiagnostic>> {
    let pending = compile_pending_inner(src, opts)?;
    Ok(SigLeekUnit {
        names: collect_top_level_signature_names(&pending.hir),
        integer_globals: collect_global_integer_constants(&pending.hir),
    })
}

/// Top-level names declared in a `.sig.leek` / `.sig.ls` unit (`function`, `global`, `var`, `class`).
fn collect_top_level_signature_names(hir: &HirFile) -> Vec<String> {
    use std::collections::HashSet;
    let mut seen = HashSet::<String>::new();
    let mut out = Vec::new();
    let mut push = |n: &str| {
        if seen.insert(n.to_string()) {
            out.push(n.to_string());
        }
    };
    for s in &hir.stmts {
        match s {
            HirStmt::FnDecl { name, .. } => push(&name.name),
            HirStmt::Global { entries, .. } => {
                for (n, _) in entries {
                    push(&n.name);
                }
            }
            HirStmt::Var { name, .. } => push(&name.name),
            HirStmt::ClassDecl { name, .. } => push(&name.name),
            HirStmt::Include { .. } => {
                // `include` is not expanded here; list declarations directly in the signature file.
            }
            _ => {}
        }
    }
    out
}

/// Parse and lower a LeekScript **signature** file (`.sig.leek` / `.sig.ls`): same lexer → parse → HIR as a
/// normal `.leek` file, but only declaration **names** are returned (no resolve, types, or `include` expansion).
///
/// Supported declaration shapes include:
/// - `global integer NAME = expr;` (optional type after `global`),
/// - `function name(T x = def, ...) => ReturnType;` (signature-only stub, no `{ }`),
/// - `function include(...)` (`include` is allowed as a function name here),
/// - overloads / duplicates of empty-bodied top-level functions merge at resolve (API stubs).
pub fn compile_signature_leek(
    src: &str,
    opts: &CompileOptions,
) -> Result<Vec<String>, Vec<CompileDiagnostic>> {
    Ok(parse_sig_leek(src, opts)?.names)
}

fn stmt_tree_contains_include(s: &HirStmt) -> bool {
    match s {
        HirStmt::Include { .. } => true,
        HirStmt::Block(stmts) => stmts.iter().any(stmt_tree_contains_include),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_tree_contains_include)
                || else_body
                    .as_ref()
                    .is_some_and(|b| b.iter().any(stmt_tree_contains_include))
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            body.iter().any(stmt_tree_contains_include)
        }
        HirStmt::Switch { clauses, .. } => clauses.iter().any(|c| match c {
            HirSwitchClause::Case { body, .. } => body.iter().any(stmt_tree_contains_include),
            HirSwitchClause::Default { body } => body.iter().any(stmt_tree_contains_include),
        }),
        HirStmt::For { init, body, .. } => {
            init.as_ref()
                .is_some_and(|b| stmt_tree_contains_include(b.as_ref()))
                || body.iter().any(stmt_tree_contains_include)
        }
        HirStmt::ForIn { body, .. } | HirStmt::ForInKeyValue { body, .. } => {
            body.iter().any(stmt_tree_contains_include)
        }
        HirStmt::FnDecl { body, .. } => body.iter().any(stmt_tree_contains_include),
        HirStmt::ClassDecl { members, .. } => members.iter().any(|m| match m {
            HirClassMember::Field { .. } => false,
            HirClassMember::Method { body, .. } | HirClassMember::Constructor { body, .. } => {
                body.iter().any(stmt_tree_contains_include)
            }
        }),
        HirStmt::Try {
            try_body,
            catch,
            finally_body,
        } => {
            try_body.iter().any(stmt_tree_contains_include)
                || catch
                    .as_ref()
                    .is_some_and(|(_, b)| b.iter().any(stmt_tree_contains_include))
                || finally_body
                    .as_ref()
                    .is_some_and(|b| b.iter().any(stmt_tree_contains_include))
        }
        _ => false,
    }
}

fn hir_contains_include(hir: &HirFile) -> bool {
    hir.stmts.iter().any(stmt_tree_contains_include)
}

fn first_include_span_stmt(s: &HirStmt) -> Option<Span> {
    match s {
        HirStmt::Include { span, .. } => Some(*span),
        HirStmt::Block(stmts) => stmts.iter().find_map(first_include_span_stmt),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => then_body
            .iter()
            .find_map(first_include_span_stmt)
            .or_else(|| {
                else_body
                    .as_ref()
                    .and_then(|b| b.iter().find_map(first_include_span_stmt))
            }),
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            body.iter().find_map(first_include_span_stmt)
        }
        HirStmt::Switch { clauses, .. } => clauses.iter().find_map(|c| match c {
            HirSwitchClause::Case { body, .. } => body.iter().find_map(first_include_span_stmt),
            HirSwitchClause::Default { body } => body.iter().find_map(first_include_span_stmt),
        }),
        HirStmt::For { init, body, .. } => init
            .as_ref()
            .and_then(|b| first_include_span_stmt(b.as_ref()))
            .or_else(|| body.iter().find_map(first_include_span_stmt)),
        HirStmt::ForIn { body, .. } | HirStmt::ForInKeyValue { body, .. } => {
            body.iter().find_map(first_include_span_stmt)
        }
        HirStmt::FnDecl { body, .. } => body.iter().find_map(first_include_span_stmt),
        HirStmt::ClassDecl { members, .. } => members.iter().find_map(|m| match m {
            HirClassMember::Field { .. } => None,
            HirClassMember::Method { body, .. } | HirClassMember::Constructor { body, .. } => {
                body.iter().find_map(first_include_span_stmt)
            }
        }),
        HirStmt::Try {
            try_body,
            catch,
            finally_body,
        } => try_body
            .iter()
            .find_map(first_include_span_stmt)
            .or_else(|| {
                catch
                    .as_ref()
                    .and_then(|(_, b)| b.iter().find_map(first_include_span_stmt))
            })
            .or_else(|| {
                finally_body
                    .as_ref()
                    .and_then(|b| b.iter().find_map(first_include_span_stmt))
            }),
        _ => None,
    }
}

fn first_include_span_hir(hir: &HirFile) -> Option<Span> {
    hir.stmts.iter().find_map(first_include_span_stmt)
}

fn expand_optional_boxed_stmt(
    init: Option<Box<HirStmt>>,
    current_file: &Path,
    opts: &CompileOptions,
    stack: &mut Vec<PathBuf>,
    cache: &mut ModuleExpansionCache,
    loaded: &mut HashSet<PathBuf>,
) -> Result<Option<Box<HirStmt>>, Vec<CompileDiagnostic>> {
    let Some(b) = init else {
        return Ok(None);
    };
    Ok(Some(Box::new(recursively_expand_stmt(
        *b,
        current_file,
        opts,
        stack,
        cache,
        loaded,
    )?)))
}

fn recursively_expand_stmt(
    s: HirStmt,
    current_file: &Path,
    opts: &CompileOptions,
    stack: &mut Vec<PathBuf>,
    cache: &mut ModuleExpansionCache,
    loaded: &mut HashSet<PathBuf>,
) -> Result<HirStmt, Vec<CompileDiagnostic>> {
    Ok(match s {
        HirStmt::Block(b) => {
            HirStmt::Block(expand_stmt_list(b, current_file, opts, stack, cache, loaded, false)?.0)
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => HirStmt::If {
            cond,
            then_body: expand_stmt_list(
                then_body,
                current_file,
                opts,
                stack,
                cache,
                loaded,
                false,
            )?
            .0,
            else_body: else_body
                .map(|v| {
                    expand_stmt_list(v, current_file, opts, stack, cache, loaded, false)
                        .map(|x| x.0)
                })
                .transpose()?,
        },
        HirStmt::While { cond, body } => HirStmt::While {
            cond,
            body: expand_stmt_list(body, current_file, opts, stack, cache, loaded, false)?.0,
        },
        HirStmt::DoWhile { body, cond } => HirStmt::DoWhile {
            body: expand_stmt_list(body, current_file, opts, stack, cache, loaded, false)?.0,
            cond,
        },
        HirStmt::Switch { discr, clauses } => {
            let mut new_clauses = Vec::with_capacity(clauses.len());
            for c in clauses {
                match c {
                    HirSwitchClause::Case { labels, body } => {
                        new_clauses.push(HirSwitchClause::Case {
                            labels,
                            body: expand_stmt_list(
                                body,
                                current_file,
                                opts,
                                stack,
                                cache,
                                loaded,
                                false,
                            )?
                            .0,
                        });
                    }
                    HirSwitchClause::Default { body } => {
                        new_clauses.push(HirSwitchClause::Default {
                            body: expand_stmt_list(
                                body,
                                current_file,
                                opts,
                                stack,
                                cache,
                                loaded,
                                false,
                            )?
                            .0,
                        });
                    }
                }
            }
            HirStmt::Switch {
                discr,
                clauses: new_clauses,
            }
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => HirStmt::For {
            init: expand_optional_boxed_stmt(init, current_file, opts, stack, cache, loaded)?,
            cond,
            update,
            body: expand_stmt_list(body, current_file, opts, stack, cache, loaded, false)?.0,
        },
        HirStmt::ForIn {
            name,
            is_declaration,
            name_by_ref,
            container,
            body,
        } => HirStmt::ForIn {
            name,
            is_declaration,
            name_by_ref,
            container,
            body: expand_stmt_list(body, current_file, opts, stack, cache, loaded, false)?.0,
        },
        HirStmt::ForInKeyValue {
            key,
            key_is_declaration,
            key_by_ref,
            value,
            value_is_declaration,
            value_by_ref,
            container,
            body,
        } => HirStmt::ForInKeyValue {
            key,
            key_is_declaration,
            key_by_ref,
            value,
            value_is_declaration,
            value_by_ref,
            container,
            body: expand_stmt_list(body, current_file, opts, stack, cache, loaded, false)?.0,
        },
        HirStmt::FnDecl {
            name,
            params,
            return_ty,
            body,
        } => HirStmt::FnDecl {
            name,
            params,
            return_ty,
            body: expand_stmt_list(body, current_file, opts, stack, cache, loaded, false)?.0,
        },
        HirStmt::ClassDecl {
            name,
            extends,
            members,
        } => HirStmt::ClassDecl {
            name,
            extends,
            members: members
                .into_iter()
                .map(|m| match m {
                    HirClassMember::Field {
                        name,
                        decl_ty,
                        init,
                        is_static,
                        is_final,
                        visibility,
                    } => Ok(HirClassMember::Field {
                        name,
                        decl_ty,
                        init,
                        is_static,
                        is_final,
                        visibility,
                    }),
                    HirClassMember::Method {
                        name,
                        is_static,
                        visibility,
                        params,
                        body,
                    } => expand_stmt_list(body, current_file, opts, stack, cache, loaded, false)
                        .map(|(nb, _)| HirClassMember::Method {
                            name,
                            is_static,
                            visibility,
                            params,
                            body: nb,
                        }),
                    HirClassMember::Constructor {
                        params,
                        body,
                        visibility,
                    } => expand_stmt_list(body, current_file, opts, stack, cache, loaded, false)
                        .map(|(nb, _)| HirClassMember::Constructor {
                            params,
                            body: nb,
                            visibility,
                        }),
                })
                .collect::<Result<Vec<_>, _>>()?,
        },
        HirStmt::Try {
            try_body,
            catch,
            finally_body,
        } => HirStmt::Try {
            try_body: expand_stmt_list(try_body, current_file, opts, stack, cache, loaded, false)?
                .0,
            catch: catch
                .map(|(p, b)| {
                    expand_stmt_list(b, current_file, opts, stack, cache, loaded, false)
                        .map(|(nb, _)| (p, nb))
                })
                .transpose()?,
            finally_body: finally_body
                .map(|b| {
                    expand_stmt_list(b, current_file, opts, stack, cache, loaded, false)
                        .map(|x| x.0)
                })
                .transpose()?,
        },
        HirStmt::Include { span, .. } => {
            return Err(vec![CompileDiagnostic {
                phase: CompilePhase::Hir,
                reference: "INCLUDE_ONLY_IN_MAIN_BLOCK".into(),
                span,
                message: "`include` is only allowed at the top level of a file".into(),
                snippet_origin: Some(current_file.to_path_buf()),
            }]);
        }
        o @ (HirStmt::Var { .. }
        | HirStmt::Expr(_)
        | HirStmt::Return { .. }
        | HirStmt::Assign { .. }
        | HirStmt::Throw(_)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Empty
        | HirStmt::Global { .. }) => o,
    })
}

/// Resolve `include("…")` / future `import` paths relative to `base_dir`: try the literal path, then — if it has no
/// extension — the same path with `.leek` (Java-style `utilities/Logger` → `utilities/Logger.leek`).
pub fn resolve_include_file(base_dir: &Path, include_path: &str) -> Result<PathBuf, Vec<PathBuf>> {
    let rel = Path::new(include_path);
    let primary = base_dir.join(rel);
    let mut tried = vec![primary.clone()];
    if primary.is_file() {
        return primary.canonicalize().map_err(|_| tried);
    }
    if rel.extension().is_none() {
        let with_leek = base_dir.join(rel.with_extension("leek"));
        if with_leek != primary {
            tried.push(with_leek.clone());
            if with_leek.is_file() {
                return with_leek.canonicalize().map_err(|_| tried);
            }
        }
    }
    Err(tried)
}

fn expand_stmt_list(
    stmts: Vec<HirStmt>,
    current_file: &Path,
    opts: &CompileOptions,
    stack: &mut Vec<PathBuf>,
    cache: &mut ModuleExpansionCache,
    loaded: &mut HashSet<PathBuf>,
    allow_include: bool,
) -> Result<(Vec<HirStmt>, Vec<PathBuf>), Vec<CompileDiagnostic>> {
    let base_dir = current_file.parent().unwrap_or_else(|| Path::new("."));
    let mut out = Vec::new();
    let mut sources = Vec::new();
    for s in stmts {
        if let HirStmt::Include { path, span } = s {
            if !allow_include {
                return Err(vec![CompileDiagnostic {
                    phase: CompilePhase::Hir,
                    reference: "INCLUDE_ONLY_IN_MAIN_BLOCK".into(),
                    span,
                    message: "`include` is only allowed at the top level of a file".into(),
                    snippet_origin: Some(current_file.to_path_buf()),
                }]);
            }
            let canon = match resolve_include_file(base_dir, &path) {
                Ok(p) => p,
                Err(tried) => {
                    return Err(vec![CompileDiagnostic {
                        phase: CompilePhase::Hir,
                        reference: "INCLUDE_NOT_FOUND".into(),
                        span,
                        message: format!(
                            "could not resolve include `{}` (tried: {})",
                            path,
                            tried
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        snippet_origin: Some(current_file.to_path_buf()),
                    }]);
                }
            };
            if stack.contains(&canon) {
                return Err(vec![CompileDiagnostic {
                    phase: CompilePhase::Hir,
                    reference: "INCLUDE_CIRCULAR".into(),
                    span,
                    message: format!("circular include `{}`", path),
                    snippet_origin: Some(current_file.to_path_buf()),
                }]);
            }
            if loaded.contains(&canon) {
                continue;
            }
            if let Some(unit) = cache.get(&canon) {
                out.extend(unit.stmts.iter().cloned());
                sources.extend(unit.stmt_sources.iter().cloned());
                loaded.insert(canon);
            } else {
                let child_src = std::fs::read_to_string(&canon).map_err(|e| {
                    vec![CompileDiagnostic {
                        phase: CompilePhase::Hir,
                        reference: "INCLUDE_READ_ERROR".into(),
                        span,
                        message: format!("could not read `{}`: {e}", canon.display()),
                        snippet_origin: Some(current_file.to_path_buf()),
                    }]
                })?;
                stack.push(canon.clone());
                let mut child_opts = opts.clone();
                child_opts.snippet_origin = Some(canon.clone());
                let pending = compile_pending_inner(&child_src, &child_opts)?;
                let (expanded_child, child_sources) =
                    expand_stmt_list(pending.hir.stmts, &canon, opts, stack, cache, loaded, true)?;
                stack.pop();
                cache.insert_expanded(
                    canon.clone(),
                    ExpandedSourceUnit {
                        stmts: expanded_child.clone(),
                        stmt_sources: child_sources.clone(),
                    },
                );
                out.extend(expanded_child);
                sources.extend(child_sources);
                loaded.insert(canon);
            }
        } else {
            sources.push(current_file.to_path_buf());
            out.push(recursively_expand_stmt(
                s,
                current_file,
                opts,
                stack,
                cache,
                loaded,
            )?);
        }
    }
    Ok((out, sources))
}

/// Run directives resolution, lexing, delimiter validation, and the statement/expression grammar.
///
/// For `include("…")`, set [`CompileOptions::source_path`] to the real `.leek` path (or rely on
/// `path_display` being resolvable via [`std::fs::canonicalize`]).
fn duplicate_constructor_diags(
    stmts: &[HirStmt],
    language_version: u8,
    snippet_origin: Option<PathBuf>,
) -> Vec<CompileDiagnostic> {
    let mut out = Vec::new();
    for s in stmts {
        let HirStmt::ClassDecl { name, members, .. } = s else {
            continue;
        };
        let n_ctor = members
            .iter()
            .filter(|m| matches!(m, HirClassMember::Constructor { .. }))
            .count();
        if n_ctor > 1 {
            out.push(CompileDiagnostic {
                phase: CompilePhase::Hir,
                reference: (if language_version >= 4 {
                    "DUPLICATED_CONSTRUCTOR"
                } else {
                    "NONE"
                })
                .into(),
                span: name.span,
                message: "duplicate constructor".into(),
                snippet_origin: snippet_origin.clone(),
            });
        }
    }
    out
}

pub fn compile_source(
    path_display: impl Into<String>,
    src: &str,
    opts: &CompileOptions,
) -> CompileOutcome {
    let path_display = path_display.into();
    let pending = compile_pending_inner(src, opts)?;
    let language_version = pending.language_version;

    let main_file_path = opts
        .source_path
        .as_ref()
        .cloned()
        .or_else(|| Path::new(&path_display).canonicalize().ok());

    let hir = if hir_contains_include(&pending.hir) {
        let Some(ref fp) = main_file_path else {
            let sp = first_include_span_hir(&pending.hir).unwrap_or_else(|| Span::point(0));
            return Err(vec![CompileDiagnostic {
                phase: CompilePhase::Hir,
                reference: "INCLUDE_REQUIRES_SOURCE_PATH".into(),
                span: sp,
                message: "`include` needs a resolvable source file path (set CompileOptions.source_path or compile an existing file path)".into(),
                snippet_origin: None,
            }]);
        };
        let mut stack = Vec::new();
        let mut cache = ModuleExpansionCache::new();
        let mut loaded = HashSet::new();
        let (stmts, stmt_sources) = expand_stmt_list(
            pending.hir.stmts,
            fp.as_path(),
            opts,
            &mut stack,
            &mut cache,
            &mut loaded,
            true,
        )?;
        HirFile {
            stmts,
            stmt_sources,
        }
    } else {
        pending.hir
    };

    let main_src_key = main_file_path
        .as_ref()
        .map(|p| p.as_path())
        .unwrap_or_else(|| Path::new(&path_display));

    let mut failures: Vec<CompileDiagnostic> = Vec::new();
    let resolve_errs = resolve_hir_with_extra_globals(
        &hir,
        main_src_key,
        &opts.signature_globals,
        language_version,
    );
    push_resolve_diags(&mut failures, &resolve_errs);
    if !resolve_errs.is_empty() {
        return Err(failures);
    }

    // Strict: validate declared return types even if not executed.
    if pending.strict_eff == Some(true) {
        for s in &hir.stmts {
            if let HirStmt::FnDecl {
                name,
                return_ty: Some(rt),
                body,
                ..
            } = s
            {
                let head = rt.split_whitespace().next().unwrap_or(rt.as_str());
                let bad = match head {
                    "void" => body
                        .iter()
                        .any(|st| matches!(st, HirStmt::Return { value: Some(_), .. })),
                    "null" => body
                        .iter()
                        .any(|st| matches!(st, HirStmt::Return { value: None, .. })),
                    _ => false,
                };
                if bad {
                    failures.push(CompileDiagnostic {
                        phase: CompilePhase::Types,
                        reference: "INCOMPATIBLE_TYPE".into(),
                        span: name.span,
                        message: "incompatible type".into(),
                        snippet_origin: opts
                            .snippet_origin
                            .clone()
                            .or_else(|| main_file_path.clone()),
                    });
                }
            }
        }
        if !failures.is_empty() {
            return Err(failures);
        }
    }

    failures.extend(duplicate_constructor_diags(
        &hir.stmts,
        language_version,
        opts.snippet_origin
            .clone()
            .or_else(|| main_file_path.clone()),
    ));
    if !failures.is_empty() {
        return Err(failures);
    }

    let type_errs = check_hir_types(&hir, main_src_key);
    push_type_diags(&mut failures, &type_errs);
    if !type_errs.is_empty() {
        return Err(failures);
    }

    Ok(CompiledUnit {
        path_display,
        language_version: pending.language_version,
        strict: pending.strict_eff,
        token_count: pending.token_count,
        fmt: pending.fmt,
        experimental: pending.experimental,
        syntax_root: pending.syntax_root,
        hir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{interpret_hir, Value};

    #[test]
    fn smoke_compiles() {
        let src = "// Example\nvar x = 0;\n";
        let r = compile_source("smoke.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(r.language_version, 4);
        assert!(r.token_count > 0);
        assert_eq!(r.syntax_root.text().to_string(), src);
    }

    #[test]
    fn debug_call_span_line_includes_leading_blank_lines() {
        use leekscript_hir::{HirExpr, HirStmt};
        use leekscript_span::line_col_at;

        // Mirrors `basic.leek` layout: blank lines before `debug(...)` so `line_col_at` must not
        // under-report the call site line (regression: would pin `debug*` farmer logs to an earlier stmt).
        let src = concat!("var enemy = 0;\n", "\n", "debug(\"w:\" + enemy);\n",);
        let path = std::path::PathBuf::from("/tmp/debug_span_test.leek");
        let opts = CompileOptions {
            source_path: Some(path),
            ..CompileOptions::default()
        };
        let u = compile_source("debug_span_test.leek", src, &opts).expect("compile");
        let mut found = false;
        for s in &u.hir.stmts {
            let HirStmt::Expr(HirExpr::Call { callee, args, span }) = s else {
                continue;
            };
            let HirExpr::Ident { name, .. } = callee.as_ref() else {
                continue;
            };
            if name != "debug" || args.len() != 1 {
                continue;
            }
            let (line, _) = line_col_at(src, span.start as usize);
            assert_eq!(line, 3, "debug() call line (span.start {})", span.start);
            found = true;
        }
        assert!(found);
    }

    #[test]
    fn strict_final_instance_field_dot_assign_errors() {
        use crate::{interpret_hir_with_strict, InterpretError};
        let src = "class A { final a = 12 } var a = new A() a.a = 15 return a.a\n";
        let u = compile_source(
            "x.leek",
            src,
            &CompileOptions {
                cli_language_version: Some(2),
                cli_strict: Some(true),
                ..CompileOptions::default()
            },
        )
        .unwrap();
        assert_eq!(u.strict, Some(true));
        let r = interpret_hir_with_strict(&u.hir, u.language_version, u.strict);
        let Err(InterpretError { reference, .. }) = r else {
            panic!("expected error, got {r:?}");
        };
        assert_eq!(reference, "CANNOT_ASSIGN_FINAL_FIELD");
    }

    #[test]
    fn debug_testobject_279_280_compile_run() {
        let opts = CompileOptions {
            cli_language_version: Some(2),
            cli_strict: Some(false),
            ..CompileOptions::default()
        };
        for (label, src) in [
            ("279", "class A { static f(x) {} static g() { f(1) } }\n"),
            ("280", "class A { static f(x) {} static g() { f() } }\n"),
            ("282", "class test { static boolean f1(Array<any> x) {} static boolean f2(any x) { f1(x) } }\n"),
        ] {
            let u = compile_source("t.leek", src, &opts);
            match &u {
                Ok(_) => eprintln!("{label} compile: ok"),
                Err(e) => eprintln!("{label} compile err: {e:?}"),
            }
            if let Ok(unit) = &u {
                let r = crate::interpret_hir_with_strict(&unit.hir, unit.language_version, unit.strict);
                eprintln!("{label} interpret: {:?}", r);
            }
        }
    }

    #[test]
    fn debug_v1_function_declared_return_type_roundtrip() {
        use crate::interpret_hir_with_strict;
        let src = "function f(real r) => real { return r } return f(12)\n";
        let u = compile_source(
            "t.leek",
            src,
            &CompileOptions {
                cli_language_version: Some(1),
                cli_strict: Some(false),
                ..CompileOptions::default()
            },
        )
        .expect("compile");
        // Ensure lowering preserves declared return type so runtime coercion can match Java export.
        let leekscript_hir::HirStmt::FnDecl { return_ty, .. } = &u.hir.stmts[0] else {
            panic!("expected FnDecl first stmt, got {:?}", u.hir.stmts.get(0));
        };
        assert_eq!(return_ty.as_deref(), Some("real"));
        let out = interpret_hir(&u.hir, u.language_version).expect("run");
        assert_eq!(out, Some(Value::RealDotZero(12.0)));
        let out2 = interpret_hir_with_strict(&u.hir, u.language_version, Some(false)).expect("run");
        assert_eq!(out2, Some(Value::RealDotZero(12.0)));
        let exp = crate::value_java_export(out2.as_ref().unwrap(), 1);
        assert_eq!(exp, "12.0");
    }

    #[test]
    fn parse_sig_leek_collects_integer_literals() {
        let src = "global integer WEAPON_PISTOL = 37;\nfunction foo() => void;\n";
        let u = parse_sig_leek(src, &CompileOptions::default()).unwrap();
        assert!(u.names.contains(&"WEAPON_PISTOL".to_string()));
        assert!(u.names.contains(&"foo".to_string()));
        assert_eq!(
            u.integer_globals
                .iter()
                .find(|(n, _)| n == "WEAPON_PISTOL")
                .map(|(_, v)| *v),
            Some(37)
        );
    }

    #[test]
    fn compile_signature_leek_collects_decls() {
        let src = concat!(
            "global GW = 1;\n",
            "global integer LW_X = 0;\n",
            "var v = 0;\n",
            "function getLife() { }\n",
            "function attack(damage) { }\n",
            "function getEntity() => integer;\n",
            "function include(string path) => void;\n",
        );
        let names = compile_signature_leek(src, &CompileOptions::default()).unwrap();
        assert!(names.iter().any(|n| n == "getLife"));
        assert!(names.iter().any(|n| n == "attack"));
        assert!(names.iter().any(|n| n == "GW"));
        assert!(names.iter().any(|n| n == "LW_X"));
        assert!(names.iter().any(|n| n == "v"));
        assert!(names.iter().any(|n| n == "getEntity"));
        assert!(names.iter().any(|n| n == "include"));
    }

    #[test]
    fn lexer_error_fails_before_parse() {
        let src = "\"unclosed\n";
        let e = compile_source("t.leek", src, &CompileOptions::default()).unwrap_err();
        assert!(e.iter().any(|d| d.phase == CompilePhase::Lexer));
    }

    #[test]
    fn unclosed_paren_is_parser_phase() {
        let src = "(1 + 2";
        let e = compile_source("t.leek", src, &CompileOptions::default()).unwrap_err();
        assert!(e.iter().any(|d| d.phase == CompilePhase::Parser));
    }

    #[test]
    fn duplicate_var_is_resolve_phase() {
        let src = "var x = 1;\nvar x = 2;\n";
        let e = compile_source("t.leek", src, &CompileOptions::default()).unwrap_err();
        assert!(e.iter().any(|d| d.phase == CompilePhase::Resolve));
    }

    /// Word-operator `xor` lowers to bitwise XOR (Java `WordCompiler`); full stack smoke.
    #[test]
    fn word_xor_compiles_and_runs() {
        let src = "return 5 xor 3;\n";
        let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&unit.hir, unit.language_version).unwrap(),
            Some(Value::Integer(6))
        );
    }

    /// Java `Operators.POWER` / `pow` — tighter than `*`, right-associative.
    #[test]
    fn power_operator_precedence_and_right_assoc() {
        let u =
            compile_source("t.leek", "return 2 + 3 ** 2;\n", &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(11))
        );
        let u2 = compile_source(
            "t.leek",
            "return 2 ** 3 ** 2;\n",
            &CompileOptions::default(),
        )
        .unwrap();
        assert_eq!(
            interpret_hir(&u2.hir, u2.language_version).unwrap(),
            Some(Value::Integer(512))
        );
    }

    /// Java `INTEGER_DIVISION` — truncating division after `getInt`-style coercion.
    #[test]
    fn intdiv_backslash_operator() {
        let u = compile_source("t.leek", "return 7 \\ 3;\n", &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(2))
        );
        // Truncates toward zero like Java `/` on `long`.
        let u2 =
            compile_source("t.leek", "return 0 - 7 \\ 3;\n", &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u2.hir, u2.language_version).unwrap(),
            Some(Value::Integer(-2))
        );
    }

    /// Java `COALESCE`: only `null` triggers the rhs (`0` and `false` are kept).
    #[test]
    fn java_style_ternary_and_typeof_and_not_in() {
        let u = compile_source(
            "t.leek",
            "return 1 ? 10 : 20;\n",
            &CompileOptions::default(),
        )
        .unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(10))
        );
        let u2 = compile_source(
            "t.leek",
            "return 0 ? 10 : 20;\n",
            &CompileOptions::default(),
        )
        .unwrap();
        assert_eq!(
            interpret_hir(&u2.hir, u2.language_version).unwrap(),
            Some(Value::Integer(20))
        );
        let u3 = compile_source(
            "t.leek",
            "return typeof null;\n",
            &CompileOptions::default(),
        )
        .unwrap();
        assert_eq!(
            interpret_hir(&u3.hir, u3.language_version).unwrap(),
            Some(Value::Integer(0))
        );
        let u4 = compile_source(
            "t.leek",
            "return 2 not in [1, 3, 4];\n",
            &CompileOptions::default(),
        )
        .unwrap();
        assert_eq!(
            interpret_hir(&u4.hir, u4.language_version).unwrap(),
            Some(Value::Bool(true))
        );
    }

    #[test]
    fn bitwise_and_shift_and_compound_assign() {
        let src = concat!(
            "var x = 6 & 3;\n",
            "var y = 1 | 2;\n",
            "var s = 1 << 3;\n",
            "var u = 8 >>> 2;\n",
            "x *= 2;\n",
            "return x + y + s + u;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(17))
        );
    }

    #[test]
    fn nullish_coalesce_short_circuits_on_null_only() {
        let u =
            compile_source("t.leek", "return null ?? 42;\n", &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(42))
        );
        let u2 = compile_source("t.leek", "return 0 ?? 99;\n", &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u2.hir, u2.language_version).unwrap(),
            Some(Value::Integer(0))
        );
        let u3 =
            compile_source("t.leek", "return false ?? 1;\n", &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u3.hir, u3.language_version).unwrap(),
            Some(Value::Bool(false))
        );
    }

    #[test]
    fn do_while_runs_body_once_when_cond_false() {
        let src = "var c = 0;\ndo { c = c + 1; } while (0);\nreturn c;\n";
        let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&unit.hir, unit.language_version).unwrap(),
            Some(Value::Integer(1))
        );
    }

    #[test]
    fn switch_break_fallthrough_and_default() {
        let src_match = "switch (2) {\ncase 1: return 1;\ncase 2: return 22;\n}\nreturn 0;\n";
        let u = compile_source("t.leek", src_match, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(22))
        );

        let src_fall = "switch (1) {\ncase 1:\ncase 2:\n return 99;\n}\nreturn 0;\n";
        let u = compile_source("t.leek", src_fall, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(99))
        );

        let src_def = "switch (9) {\ndefault: return 7;\n}\nreturn 0;\n";
        let u = compile_source("t.leek", src_def, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(7))
        );
    }

    #[test]
    fn continue_in_switch_exits_to_enclosing_while() {
        let src = "var i = 0;\nwhile (i < 3) {\n i = i + 1;\n switch (1) {\n case 1: continue;\n }\n}\nreturn i;\n";
        let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&unit.hir, unit.language_version).unwrap(),
            Some(Value::Integer(3))
        );
    }

    #[test]
    fn for_in_over_array_string_not_iterable() {
        let src = "var s = 0;\nfor (var x in [1, 2, 3]) { s = s + x; }\nreturn s;\n";
        let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&unit.hir, unit.language_version).unwrap(),
            Some(Value::Integer(6))
        );

        // Matches leekscript Java `AI.isIterable`: strings are not iterable in `for`-`in`.
        let src2 = "for (var c in \"hi\") { return 1; }\nreturn 0;\n";
        let unit2 = compile_source("t.leek", src2, &CompileOptions::default()).unwrap();
        let e = interpret_hir(&unit2.hir, unit2.language_version).unwrap_err();
        assert_eq!(e.reference, "NOT_ITERABLE");
    }

    #[test]
    fn for_in_key_value_over_array() {
        let src = "var s = 0;\nfor (var i : var v in [10, 20]) { s = s + i + v; }\nreturn s;\n";
        let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&unit.hir, unit.language_version).unwrap(),
            Some(Value::Integer(31))
        );
    }

    #[test]
    fn for_in_typed_header_runs() {
        let src = "var s = 0;\nfor (integer k in [1, 2, 3]) { s = s + k; }\nreturn s;\n";
        let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&unit.hir, unit.language_version).unwrap(),
            Some(Value::Integer(6))
        );
    }

    #[test]
    fn for_in_assigns_existing_var() {
        let src = "var x = 0;\nfor (x in [5]) { }\nreturn x;\n";
        let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&unit.hir, unit.language_version).unwrap(),
            Some(Value::Integer(5))
        );
    }

    /// Java-style `+`: string concatenation with coercion when either side is a string.
    #[test]
    fn string_concat_plus() {
        let src = "return \"a\" + 1 + \"_\" + true + null;\n";
        let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&unit.hir, unit.language_version).unwrap(),
            Some(Value::String("a1_truenull".into()))
        );
    }

    #[test]
    fn instanceof_runtime_checks() {
        for (src, expected) in [
            ("return [1] instanceof Array;\n", true),
            ("return \"x\" instanceof string;\n", true),
            ("return 1 instanceof integer;\n", true),
            ("return 1 instanceof string;\n", false),
            ("return null instanceof Array;\n", false),
        ] {
            let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
            assert_eq!(
                interpret_hir(&unit.hir, unit.language_version).unwrap(),
                Some(Value::Bool(expected)),
                "{src}"
            );
        }

        let src = "function id() { return 0; }\nreturn id instanceof Function;\n";
        let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&unit.hir, unit.language_version).unwrap(),
            Some(Value::Bool(true))
        );
    }

    /// Java `AI.isIterable`: map, set, interval work in `for`-`in`; simple `for (x in map)` yields **values**.
    #[test]
    fn map_set_interval_for_in_and_instanceof() {
        let src_map_vals = "var m = new Map(\"a\", 1, \"b\", 2);\nvar s = 0;\nfor (var v in m) { s = s + v; }\nreturn s;\n";
        let u = compile_source("t.leek", src_map_vals, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(3))
        );

        let src_map_kv = "var m = new Map(0, 10, 1, 20);\nvar s = 0;\nfor (var k : var v in m) { s = s + k + v; }\nreturn s;\n";
        let u = compile_source("t.leek", src_map_kv, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(31))
        );

        let src_set = "var st = new Set(1, 2, 2, 3);\nvar t = 0;\nfor (var x in st) { t = t + x; }\nreturn t;\n";
        let u = compile_source("t.leek", src_set, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(6))
        );

        let src_iv = "var iv = new Interval(true, 1, true, 3);\nvar t = 0;\nfor (var x in iv) { t = t + x; }\nreturn t;\n";
        let u = compile_source("t.leek", src_iv, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(6))
        );

        let src_iv_kv = "var iv = new Interval(true, 1, true, 3);\nvar t = 0;\nfor (var i : var x in iv) { t = t + i + x; }\nreturn t;\n";
        let u = compile_source("t.leek", src_iv_kv, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(9))
        );

        for (src, expected) in [
            ("return new Map() instanceof Map;\n", true),
            ("return new Set() instanceof Set;\n", true),
            ("return new Interval() instanceof Interval;\n", true),
            ("return [1] instanceof Map;\n", false),
        ] {
            let unit = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
            assert_eq!(
                interpret_hir(&unit.hir, unit.language_version).unwrap(),
                Some(Value::Bool(expected)),
                "{src}"
            );
        }
    }

    /// Bracket / angle literals lower like `new Map` / `new Set` / `new Interval` (Java `WordCompiler`).
    #[test]
    fn map_set_interval_literal_syntax_matches_new() {
        let src = "var m = [\"a\": 1, \"b\": 2];\nvar s = 0;\nfor (var v in m) { s = s + v; }\nreturn s;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(3))
        );

        let src = "return [:] instanceof Map;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Bool(true))
        );

        let src =
            "var st = <1, 2, 2, 3>;\nvar t = 0;\nfor (var x in st) { t = t + x; }\nreturn t;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(6))
        );

        let src = "var iv = [1..3];\nvar t = 0;\nfor (var x in iv) { t = t + x; }\nreturn t;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(6))
        );

        let src = "return [..] instanceof Interval;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Bool(true))
        );

        // Java `LeekInterval`: `[` closer means open upper bound (`maxClosed` false) — here1+2 only.
        let src = "var iv = [1..3[;\nvar t = 0;\nfor (var x in iv) { t = t + x; }\nreturn t;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(3))
        );
    }

    #[test]
    fn index_get_set_and_add_assign() {
        let src = "var a = [1, 2];\na[0] = 9;\na[1] += 3;\nreturn a[0] + a[1];\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(14))
        );
    }

    #[test]
    fn in_operator_array_map_set() {
        for (src, expected) in [
            ("return 2 in [1, 2, 3];\n", true),
            ("return 9 in [1, 2, 3];\n", false),
            ("var m = new Map(\"a\", 1); return \"a\" in m;\n", true),
            ("return \"b\" in new Map(\"a\", 1);\n", false),
            ("return 2 in new Set(1, 2);\n", true),
        ] {
            let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
            assert_eq!(
                interpret_hir(&u.hir, u.language_version).unwrap(),
                Some(Value::Bool(expected)),
                "{src}"
            );
        }
    }

    #[test]
    fn try_catch_throw() {
        let src = "var x = 0;\ntry { throw 5; x = 1; } catch (e) { x = e; }\nreturn x;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(5))
        );
    }

    #[test]
    fn try_catch_finally_runs_finally_after_catch() {
        let src = concat!(
            "var x = 0;\n",
            "try { throw 2; } catch (e) { x = e; } finally { x = x + 10; }\n",
            "return x;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(12))
        );
    }

    #[test]
    fn try_finally_without_catch() {
        let src = "var x = 1;\ntry { x = x + 1; } finally { x = x * 3; }\nreturn x;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(6))
        );
    }

    #[test]
    fn hex_and_binary_integer_literals_run() {
        let src = "return 0xA + 0b11;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(13))
        );
    }

    #[test]
    fn array_slice_half_open_range() {
        let src = concat!(
            "var a = [10, 20, 30, 40];\n",
            "var b = a[1:3];\n",
            "return b[0] + b[1];\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(50))
        );
    }

    #[test]
    fn array_slice_with_step() {
        let src = concat!(
            "var a = [0,1,2,3,4];\n",
            "var s = 0;\n",
            "for (var x in a[0:5:2]) { s = s + x; }\n",
            "return s;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(6))
        );
    }

    #[test]
    fn array_slice_full_range_with_step() {
        let src = concat!(
            "var a = [1,2,3,4,5];\n",
            "var s = 0;\n",
            "for (var x in a[::2]) { s = s + x; }\n",
            "return s;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(9))
        );
    }

    #[test]
    fn array_slice_reverse_full() {
        let src = concat!(
            "var a = [10, 20, 30];\n",
            "var s = 0;\n",
            "for (var x in a[::-1]) { s = s + x; }\n",
            "return s;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(60))
        );
    }

    #[test]
    fn array_slice_negative_step_range() {
        let src = concat!(
            "var a = [0,1,2,3,4,5,6];\n",
            "var s = 0;\n",
            "for (var x in a[5:2:-1]) { s = s + x; }\n",
            "return s;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(12))
        );
    }

    #[test]
    fn array_negative_index_read_and_assign() {
        let src = concat!(
            "var a = [10, 20, 30];\n",
            "var x = a[-1];\n",
            "a[-2] = 99;\n",
            "return x + a[1];\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(129))
        );
    }

    #[test]
    fn array_slice_negative_end_matches_java() {
        let src = concat!(
            "var a = [1, 2, 3, 4, 5];\n",
            "var s = 0;\n",
            "for (var x in a[1:-1]) { s = s + x; }\n",
            "return s;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(9))
        );
    }

    #[test]
    fn array_slice_java_normalizes_wide_bounds() {
        let src = concat!(
            "var a = [1, 2, 3];\n",
            "var s = 0;\n",
            "for (var x in a[-99:99]) { s = s + x; }\n",
            "return s;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(6))
        );
    }

    #[test]
    fn array_slice_step_zero_treated_as_one_like_java() {
        let src = concat!(
            "var a = [0,1,2,3];\n",
            "var s = 0;\n",
            "for (var x in a[0:4:0]) { s = s + x; }\n",
            "return s;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(6))
        );
    }

    #[test]
    fn global_stmt_installs_outer_binding() {
        let src = "{ global x = 7; }\nreturn x;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(7))
        );
    }

    #[test]
    fn include_without_resolvable_path_errors() {
        let src = "include(\"missing.leek\");\n";
        let e = compile_source("nope.leek", src, &CompileOptions::default()).unwrap_err();
        assert!(e
            .iter()
            .any(|d| d.reference == "INCLUDE_REQUIRES_SOURCE_PATH"));
    }

    #[test]
    fn include_expands_when_source_path_set() {
        let dir = std::env::temp_dir().join(format!("leek_inc_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("inc.leek");
        std::fs::write(&inc_path, "global shared = 100;\n").unwrap();
        let main_path = dir.join("main.leek");
        let main_src = "include(\"inc.leek\");\nreturn shared;\n";
        std::fs::write(&main_path, main_src).unwrap();
        let canon_main = std::fs::canonicalize(&main_path).unwrap();
        let opts = CompileOptions {
            source_path: Some(canon_main.clone()),
            ..Default::default()
        };
        let u = compile_source(canon_main.display().to_string(), main_src, &opts).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(100))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn include_once_per_module_dedupes_transitive_graph() {
        let dir = std::env::temp_dir().join(format!("leek_inc_dedup_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.leek"),
            "global A = 1;\nfunction onlyInA() { return A; }\n",
        )
        .unwrap();
        std::fs::write(dir.join("b.leek"), "include(\"a.leek\");\nglobal B = 2;\n").unwrap();
        let main_path = dir.join("main.leek");
        let main_src = concat!(
            "include(\"a.leek\");\n",
            "include(\"b.leek\");\n",
            "return A + B;\n",
        );
        std::fs::write(&main_path, main_src).unwrap();
        let canon_main = std::fs::canonicalize(&main_path).unwrap();
        let opts = CompileOptions {
            source_path: Some(canon_main.clone()),
            ..Default::default()
        };
        let u = compile_source(canon_main.display().to_string(), main_src, &opts).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(3))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn include_resolves_nested_path_without_leek_extension() {
        let dir = std::env::temp_dir().join(format!("leek_inc_nested_{}", std::process::id()));
        let sub = dir.join("utilities");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("Logger.leek"), "global LOG = 1;\n").unwrap();
        let main_path = dir.join("main.leek");
        let main_src = "include(\"utilities/Logger\");\nreturn LOG;\n";
        std::fs::write(&main_path, main_src).unwrap();
        let canon_main = std::fs::canonicalize(&main_path).unwrap();
        let opts = CompileOptions {
            source_path: Some(canon_main.clone()),
            ..Default::default()
        };
        let u = compile_source(canon_main.display().to_string(), main_src, &opts).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(1))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn include_rejects_extra_parens_around_path_like_java() {
        let dir = std::env::temp_dir().join(format!("leek_inc_dblpar_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("lib.leek"), "global n = 1;\n").unwrap();
        let main_path = dir.join("main.leek");
        let main_src = "include((\"lib.leek\"));\nreturn n;\n";
        std::fs::write(&main_path, main_src).unwrap();
        let canon_main = std::fs::canonicalize(&main_path).unwrap();
        let opts = CompileOptions {
            source_path: Some(canon_main.clone()),
            ..Default::default()
        };
        let e = compile_source(canon_main.display().to_string(), main_src, &opts).unwrap_err();
        assert!(e.iter().any(|d| d.reference == "AI_NAME_EXPECTED"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn include_without_semicolon_like_java() {
        let dir = std::env::temp_dir().join(format!("leek_inc_nosemi_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("lib.leek");
        std::fs::write(&inc_path, "global n = 5;\n").unwrap();
        let main_path = dir.join("entry.leek");
        let main_src = "include(\"lib.leek\")\nreturn n;\n";
        std::fs::write(&main_path, main_src).unwrap();
        let canon_main = std::fs::canonicalize(&main_path).unwrap();
        let opts = CompileOptions {
            source_path: Some(canon_main.clone()),
            ..Default::default()
        };
        let u = compile_source(canon_main.display().to_string(), main_src, &opts).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(5))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn include_inside_function_errors_include_only_in_main_block() {
        let dir = std::env::temp_dir().join(format!("leek_inc_nested_fn_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("inc.leek"), "global x = 1;\n").unwrap();
        let main_path = dir.join("main.leek");
        let main_src = "function f() { include(\"inc.leek\"); }\n";
        std::fs::write(&main_path, main_src).unwrap();
        let canon_main = std::fs::canonicalize(&main_path).unwrap();
        let opts = CompileOptions {
            source_path: Some(canon_main.clone()),
            ..Default::default()
        };
        let e = compile_source(canon_main.display().to_string(), main_src, &opts).unwrap_err();
        assert!(e.iter().any(|d| {
            d.phase == CompilePhase::Hir && d.reference == "INCLUDE_ONLY_IN_MAIN_BLOCK"
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn include_inside_top_level_block_errors() {
        let dir = std::env::temp_dir().join(format!("leek_inc_nested_blk_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("inc.leek"), "global x = 1;\n").unwrap();
        let main_path = dir.join("main.leek");
        let main_src = "{ include(\"inc.leek\"); }\n";
        std::fs::write(&main_path, main_src).unwrap();
        let canon_main = std::fs::canonicalize(&main_path).unwrap();
        let opts = CompileOptions {
            source_path: Some(canon_main.clone()),
            ..Default::default()
        };
        let e = compile_source(canon_main.display().to_string(), main_src, &opts).unwrap_err();
        assert!(e.iter().any(|d| {
            d.phase == CompilePhase::Hir && d.reference == "INCLUDE_ONLY_IN_MAIN_BLOCK"
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn include_chains_across_files_at_each_top_level() {
        let dir = std::env::temp_dir().join(format!("leek_inc_chain_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("leaf.leek"), "global v = 42;\n").unwrap();
        std::fs::write(dir.join("mid.leek"), "include(\"leaf.leek\");\n").unwrap();
        let main_path = dir.join("main.leek");
        let main_src = "include(\"mid.leek\");\nreturn v;\n";
        std::fs::write(&main_path, main_src).unwrap();
        let canon_main = std::fs::canonicalize(&main_path).unwrap();
        let opts = CompileOptions {
            source_path: Some(canon_main.clone()),
            ..Default::default()
        };
        let u = compile_source(canon_main.display().to_string(), main_src, &opts).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(42))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two branches include the same leaf; leaf has no bindings so duplicate expansion is still valid.
    /// Regression: leaf must be lexed/parsed/lowered once (cached), not once per include edge.
    #[test]
    fn include_diamond_shared_empty_leaf() {
        let dir = std::env::temp_dir().join(format!("leek_inc_diamond_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("leaf.leek"), "// shared\n").unwrap();
        std::fs::write(
            dir.join("left.leek"),
            "include(\"leaf.leek\");\nglobal xl = 10;\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("right.leek"),
            "include(\"leaf.leek\");\nglobal xr = 32;\n",
        )
        .unwrap();
        let main_path = dir.join("main.leek");
        let main_src = "include(\"left.leek\");\ninclude(\"right.leek\");\nreturn xl + xr;\n";
        std::fs::write(&main_path, main_src).unwrap();
        let canon_main = std::fs::canonicalize(&main_path).unwrap();
        let opts = CompileOptions {
            source_path: Some(canon_main.clone()),
            ..Default::default()
        };
        let u = compile_source(canon_main.display().to_string(), main_src, &opts).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(42))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn optional_semicolon_after_return_break_continue() {
        let src = concat!(
            "function f() {\n",
            "  return 1\n",
            "}\n",
            "var x = 0;\n",
            "while (true) {\n",
            "  x = x + 1\n",
            "  if (x > 2) { break }\n",
            "  continue\n",
            "}\n",
            "return f() + x;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(4))
        );
    }

    #[test]
    fn return_optional_question_java_conditional_return() {
        let src = concat!(
            "function pick() {\n",
            "  return ? 0\n",
            "  return ? 2\n",
            "  return 9\n",
            "}\n",
            "return pick();\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(2))
        );
    }

    #[test]
    fn interval_leading_rbracket_parses_and_runs() {
        let src = "var iv = ]1..3[;\nvar t = 0;\nfor (var x in iv) { t = t + x; }\nreturn t;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(2))
        );
    }

    #[test]
    fn stdlib_java_style_globals() {
        // Avoid float literals with `.` in source (lexer treats `.` as member access today).
        let src = "return abs(0-2) + min(1, 9) + max(0, 3) + floor(5/2) + ceil(3/2) + sqrt(16);\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        // `/` is real division: `ceil(3/2)` is `ceil(1.5)` →2, not `ceil(1)`.
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Real(14.0))
        );

        let src = "return typeOf(null) + typeOf(true) * 10 + typeOf(\"a\") * 100;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(320))
        );

        let src = "return number(\"2.5\") + number(false);\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Real(2.5))
        );

        let src = "return string(42);\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::String("42".into()))
        );

        let src = "return abs instanceof Function;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Bool(true))
        );
    }

    /// Java v2+ `{` `}` object literals: lowered like maps; `.field` reads/writes string keys.
    #[test]
    fn object_literal_member_access_and_assign() {
        let src = concat!(
            "var o = { a: 1, b: 2, };\n",
            "o.a = o.a + 5;\n",
            "o.b += 3;\n",
            "return o.a + o.b;\n",
        );
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(11))
        );
    }

    /// Java v1–v3: bracket map / list literals use `LegacyLeekArray` in `WordCompiler`; v4+ uses `LeekMap` / `LeekArray`.
    #[test]
    fn v3_lowering_uses_legacy_leek_array_constructors() {
        let opts = CompileOptions {
            cli_language_version: Some(3),
            ..Default::default()
        };
        let src = "return [\"a\": 1] instanceof LegacyLeekArray;\n";
        let u = compile_source("t.leek", src, &opts).unwrap();
        assert_eq!(u.language_version, 3);
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Bool(true))
        );

        let src = "return [1, 2] instanceof LegacyLeekArray;\n";
        let u = compile_source("t.leek", src, &opts).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Bool(true))
        );

        // Still a normal array for `instanceof Array`.
        let src = "return [1, 2] instanceof Array;\n";
        let u = compile_source("t.leek", src, &opts).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Bool(true))
        );

        let src = "return [1, 2] instanceof LegacyLeekArray;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Bool(false))
        );
    }

    #[test]
    fn class_new_method_and_field() {
        let src = r#"class Box {
  function Box(v) {
    this.v = v;
  }
  function get() {
    return this.v;
  }
}
var b = new Box(7);
return b.get();
"#;
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(7))
        );
    }

    #[test]
    fn class_constructor_keyword_works() {
        let src = r#"class Box {
  constructor(v) {
    this.v = v;
  }
  function get() {
    return this.v;
  }
}
var b = new Box(7);
return b.get();
"#;
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(7))
        );
    }

    #[test]
    fn constructor_precedence_over_legacy() {
        let src = r#"class Box {
  function Box(v) {
    this.v = 1;
  }
  constructor(v) {
    this.v = v;
  }
  function get() {
    return this.v;
  }
}
var b = new Box(9);
return b.get();
"#;
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(9))
        );
    }

    #[test]
    fn class_java_style_fields_and_typed_constructor() {
        let src = r#"class Box {
  integer x
  constructor(integer n) {
    this.x = n;
  }
  function get() {
    return this.x;
  }
}
var b = new Box(3);
return b.get();
"#;
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(3))
        );
    }

    #[test]
    fn instance_this_class_name() {
        let src = r#"class A {
  function name() {
    return this.class.name;
  }
}
var a = new A();
return a.name();
"#;
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::String("A".into()))
        );
    }

    #[test]
    fn postfix_increment_decrement_runs() {
        let src = r#"var i = 1;
i++;
i++;
i--;
return i;
"#;
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Integer(2))
        );
    }

    #[test]
    fn as_cast_basic_literals_and_precedence() {
        // Avoid float literals with `.` in source (lexer treats `.` as member access today).
        let src = "return (5 as real) + number(\"0.5\");\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Real(5.5))
        );

        let src = "return 5 as string;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::String("5".into()))
        );

        // Precedence check: `as` binds tighter than `+` here.
        let src = "return number(\"1.2\") + 2 as integer;\n";
        let u = compile_source("t.leek", src, &CompileOptions::default()).unwrap();
        assert_eq!(
            interpret_hir(&u.hir, u.language_version).unwrap(),
            Some(Value::Real(3.2))
        );
    }

    #[test]
    fn as_cast_impossible_rejected_by_type_phase() {
        let src = "return \"x\" as integer;\n";
        let e = compile_source("t.leek", src, &CompileOptions::default()).unwrap_err();
        assert!(e.iter().any(|d| d.phase == CompilePhase::Types));
        assert!(e.iter().any(|d| d.reference == "IMPOSSIBLE_CAST"));
    }
}
