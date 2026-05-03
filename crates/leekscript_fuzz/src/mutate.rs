//! CST-aware mutation and policy filters.

use crate::{MutantAcceptance, MutateError, MutateOutcome, MutateSettings, OutcomeKind};
use leekscript_lexer::{Lexer, LexerConfig};
use leekscript_parser::parse_file_green;
use leekscript_run::compile_source;
use leekscript_syntax::{LeekLanguage, LeekSyntaxKind};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::Rng;
use rowan::{NodeOrToken, SyntaxNode, TextRange};

type SynNode = SyntaxNode<LeekLanguage>;

/// `true` if lexer + grammar parse succeed for at least one language version (4 down to 1).
#[must_use]
pub fn source_parses_any_version(src: &str) -> bool {
    parse_best(src).is_some()
}

/// Best-effort parse: first lexer version in `4..=1` that yields a green tree.
#[must_use]
pub fn parse_best(src: &str) -> Option<SynNode> {
    for version in [4u8, 3, 2, 1] {
        let cfg = LexerConfig { version };
        let (tokens, lex_err) = Lexer::new(src, cfg).tokenize();
        if !lex_err.is_empty() {
            continue;
        }
        if let Ok(root) = parse_file_green(src, &tokens) {
            return Some(root);
        }
    }
    None
}

/// One randomized mutant using the same rules as historical `leekgen` fuzz (`AcceptAll` path).
///
/// - `0`: unchanged
/// - `1`: trailing comment marker
/// - `2` / `3`: CST mutations when the input parses; otherwise comment + optional digit perturbation
/// - `4`: heavier CST pass (more edit sites / parentheses wrapping)
pub fn generate_mutant_candidate(src: &str, rng: &mut StdRng, level: u8) -> String {
    generate_mutant_candidate_with_settings(src, rng, level, &MutateSettings::default())
}

/// Like [`generate_mutant_candidate`], but uses [`MutateSettings`] (injection knobs).
pub fn generate_mutant_candidate_with_settings(
    src: &str,
    rng: &mut StdRng,
    level: u8,
    settings: &MutateSettings,
) -> String {
    match level {
        0 => src.to_string(),
        1 => append_fuzz_comment(src, rng),
        2..=4 => mutate_ast_or_fallback(src, rng, level, settings),
        _ => mutate_ast_or_fallback(src, rng, 4, settings),
    }
}

/// Mutate with an acceptance policy (parse / compile retries).
pub fn mutate_leek_source(
    src: &str,
    rng: &mut StdRng,
    level: u8,
    settings: &MutateSettings,
) -> Result<MutateOutcome, MutateError> {
    if level == 0 {
        return Ok(MutateOutcome {
            source: src.to_string(),
            kind: OutcomeKind::NoOp,
        });
    }

    if settings.acceptance == MutantAcceptance::RequireCompilable && settings.compile.is_none() {
        return Err(MutateError::MissingCompileContext);
    }

    if settings.acceptance == MutantAcceptance::AcceptAll {
        return Ok(MutateOutcome {
            source: generate_mutant_candidate_with_settings(src, rng, level, settings),
            kind: OutcomeKind::Mutated,
        });
    }

    let max = settings.max_attempts.max(1);
    if settings.acceptance == MutantAcceptance::RequireParseable {
        for _ in 0..max {
            let candidate = generate_mutant_candidate_with_settings(src, rng, level, settings);
            if source_parses_any_version(&candidate) {
                return Ok(MutateOutcome {
                    source: candidate,
                    kind: OutcomeKind::Mutated,
                });
            }
        }
    } else {
        let ctx = settings
            .compile
            .as_ref()
            .expect("validated RequireCompilable + compile above");
        for _ in 0..max {
            let candidate = generate_mutant_candidate_with_settings(src, rng, level, settings);
            if compile_source(&ctx.path_display, &candidate, &ctx.options).is_ok() {
                return Ok(MutateOutcome {
                    source: candidate,
                    kind: OutcomeKind::Mutated,
                });
            }
        }
    }

    Ok(MutateOutcome {
        source: src.to_string(),
        kind: OutcomeKind::RejectedAllAttempts { attempts: max },
    })
}

fn append_fuzz_comment(src: &str, rng: &mut StdRng) -> String {
    let mut s = src.to_string();
    s.push_str(&format!("\n// leekgen-fuzz:{:016x}\n", rng.gen::<u64>()));
    s
}

fn mutate_ast_or_fallback(
    src: &str,
    rng: &mut StdRng,
    level: u8,
    settings: &MutateSettings,
) -> String {
    if let Some(mut out) = try_cst_mutations(src, rng, level, settings) {
        out.push_str(&format!("\n// leekgen-fuzz:{:016x}\n", rng.gen::<u64>()));
        return out;
    }
    let mut s = src.to_string();
    s.push_str(&format!("\n// leekgen-fuzz:{:016x}\n", rng.gen::<u64>()));
    if rng.gen_bool(0.35) {
        if let Some(t) = perturb_random_ascii_digit(&s, rng) {
            s = t;
        }
    }
    s
}

fn try_cst_mutations(
    src: &str,
    rng: &mut StdRng,
    level: u8,
    settings: &MutateSettings,
) -> Option<String> {
    let root = parse_best(src)?;
    let _scope_idents = collect_declared_identifiers(&root);
    let mut candidates = collect_mutation_candidates(src, &root, level, rng, settings);
    if candidates.is_empty() {
        return None;
    }
    let max_ops = match level {
        2 => rng.gen_range(2..=6),
        3 => rng.gen_range(8..=18),
        _ => rng.gen_range(16..=36),
    };
    candidates.shuffle(rng);
    let picked = pick_disjoint(candidates, max_ops);
    if picked.is_empty() {
        return None;
    }
    apply_replacements(src, &picked)
}

#[derive(Clone)]
enum MutationCandidate {
    Replace { range: TextRange, new_text: String },
}

impl MutationCandidate {
    fn range(&self) -> TextRange {
        match self {
            MutationCandidate::Replace { range, .. } => *range,
        }
    }
}

/// `include("p")` must keep a bare string inside the first `(` (Java `AI_NAME_EXPECTED` otherwise).
/// Parenthesis-wrapping the path literal would produce `include(("p"))`, which is invalid.
fn literal_expr_is_include_path(lit: &SynNode) -> bool {
    let mut p = lit.parent();
    while let Some(n) = p {
        if n.kind() == LeekSyntaxKind::IncludeStmt {
            return true;
        }
        p = n.parent();
    }
    false
}

fn literal_expr_is_string_literal(src: &str, lit: &SynNode) -> bool {
    let r = lit.text_range();
    byte_slice(src, r).is_some_and(|s| s.trim_start().starts_with('"'))
}

/// Paren- or nudge-mutating a literal that participates in `++` / `--` can make the operand
/// not a valid update target and cause `INVALID_ASSIGN_TARGET` at compile.
fn node_has_pre_post_update_ancestor(n: &SynNode) -> bool {
    let mut p = n.parent();
    while let Some(x) = p {
        if matches!(
            x.kind(),
            LeekSyntaxKind::PreUpdateExpr | LeekSyntaxKind::PostUpdateExpr
        ) {
            return true;
        }
        p = x.parent();
    }
    false
}

fn token_has_pre_post_update_ancestor(t: &rowan::SyntaxToken<LeekLanguage>) -> bool {
    let mut p = t.parent();
    while let Some(n) = p {
        if matches!(
            n.kind(),
            LeekSyntaxKind::PreUpdateExpr | LeekSyntaxKind::PostUpdateExpr
        ) {
            return true;
        }
        p = n.parent();
    }
    false
}

/// Start of the `for (...) { ... }` **body** block (after the closing `)` of the header).
fn for_stmt_body_block_start(for_node: &SynNode) -> Option<rowan::TextSize> {
    // Emit order: `for` `(` … `)` `body` — the loop body is the last `Block` child.
    for_node
        .children()
        .filter(|c| c.kind() == LeekSyntaxKind::Block)
        .last()
        .map(|b| b.text_range().start())
}

/// True when `node` lies **inside the `for ( … )` clause** (before the loop body `{`), for some enclosing `for`.
///
/// Paren-wrapping or nudging literals there can produce forms the official Java parser rejects
/// (e.g. `for (var i =( 0); …`) while Rust still parses and runs, which desyncs `fight.actions`
/// (extra `say` / different `[1002, …]` ordering vs the generator).
fn node_strictly_before_enclosing_for_body(node: &SynNode) -> bool {
    let mut cur = node.parent();
    while let Some(ancestor) = cur {
        if ancestor.kind() == LeekSyntaxKind::ForStmt {
            if let Some(body_start) = for_stmt_body_block_start(&ancestor) {
                if node.text_range().start() < body_start {
                    return true;
                }
            }
        }
        cur = ancestor.parent();
    }
    false
}

fn token_strictly_before_enclosing_for_body(t: &rowan::SyntaxToken<LeekLanguage>) -> bool {
    let mut cur = t.parent();
    while let Some(ancestor) = cur {
        if ancestor.kind() == LeekSyntaxKind::ForStmt {
            if let Some(body_start) = for_stmt_body_block_start(&ancestor) {
                if t.text_range().start() < body_start {
                    return true;
                }
            }
        }
        cur = ancestor.parent();
    }
    false
}

fn token_has_index_expr_ancestor(t: &rowan::SyntaxToken<LeekLanguage>) -> bool {
    let mut cur = t.parent();
    while let Some(ancestor) = cur {
        if ancestor.kind() == LeekSyntaxKind::IndexExpr {
            return true;
        }
        cur = ancestor.parent();
    }
    false
}

/// Direct children of [`LeekSyntaxKind::SourceFile`] that are whole statements (not trivia / expr fragments).
fn is_top_level_statement_kind(k: LeekSyntaxKind) -> bool {
    matches!(
        k,
        LeekSyntaxKind::VarDecl
            | LeekSyntaxKind::TypedVarDecl
            | LeekSyntaxKind::ExprStmt
            | LeekSyntaxKind::ReturnStmt
            | LeekSyntaxKind::Block
            | LeekSyntaxKind::FunctionDecl
            | LeekSyntaxKind::IfStmt
            | LeekSyntaxKind::WhileStmt
            | LeekSyntaxKind::DoWhileStmt
            | LeekSyntaxKind::ForStmt
            | LeekSyntaxKind::ForInStmt
            | LeekSyntaxKind::ForInKeyValueStmt
            | LeekSyntaxKind::SwitchStmt
            | LeekSyntaxKind::TryStmt
            | LeekSyntaxKind::ThrowStmt
            | LeekSyntaxKind::BreakStmt
            | LeekSyntaxKind::ContinueStmt
            | LeekSyntaxKind::EmptyStmt
            | LeekSyntaxKind::GlobalStmt
            | LeekSyntaxKind::IncludeStmt
            | LeekSyntaxKind::AssignStmt
            | LeekSyntaxKind::ClassDecl
    )
}

/// How many top-level statements the file has. Used to skip statement-inject/wrap on one-liner `include`
/// targets (`say("…");` only) — wrapping them breaks the official Java parser while Rust still accepts the tree.
fn source_file_top_level_stmt_count(root: &SynNode) -> usize {
    if root.kind() != LeekSyntaxKind::SourceFile {
        return usize::MAX;
    }
    root.children()
        .filter(|c| is_top_level_statement_kind(c.kind()))
        .count()
}

fn node_has_index_expr_ancestor(n: &SynNode) -> bool {
    let mut cur = n.parent();
    while let Some(ancestor) = cur {
        if ancestor.kind() == LeekSyntaxKind::IndexExpr {
            return true;
        }
        cur = ancestor.parent();
    }
    false
}

fn collect_declared_identifiers(root: &SynNode) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for el in root.descendants_with_tokens() {
        let NodeOrToken::Token(t) = el else { continue };
        if t.kind() != LeekSyntaxKind::Ident {
            continue;
        }
        // Best-effort: include identifiers that appear in var decls / typed decls / function params.
        // This is not a perfect scope model, but is good enough to generate “real code” that reuses names.
        let mut p = t.parent();
        let mut ok = false;
        while let Some(n) = p {
            match n.kind() {
                LeekSyntaxKind::VarDecl
                | LeekSyntaxKind::TypedVarDecl
                | LeekSyntaxKind::FunctionDecl => {
                    ok = true;
                    break;
                }
                LeekSyntaxKind::SourceFile => break,
                _ => {}
            }
            p = n.parent();
        }
        if ok {
            let s = t.text().trim().to_string();
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn collect_mutation_candidates(
    src: &str,
    root: &SynNode,
    level: u8,
    rng: &mut StdRng,
    settings: &MutateSettings,
) -> Vec<MutationCandidate> {
    let inject = &settings.inject;
    let parity_safe = settings.parity_safe;
    let mut out = Vec::new();

    for el in root.descendants_with_tokens() {
        let NodeOrToken::Token(t) = el else {
            continue;
        };
        if t.kind().is_trivia() {
            continue;
        }
        if t.kind() == LeekSyntaxKind::Number {
            // Parity-safe mode: avoid changing numeric literals.
            // In practice this can accidentally turn bounded loops into unbounded ones (or vice versa),
            // causing Java vs Rust to diverge in when/if the ops budget is exhausted.
            if parity_safe {
                continue;
            }
            if token_has_pre_post_update_ancestor(&t) {
                continue;
            }
            if token_strictly_before_enclosing_for_body(&t) {
                continue;
            }
            // Nudging `arr[0]` indices can desync Java vs Rust (`getWeapons()[-(1)]`, etc.).
            if token_has_index_expr_ancestor(&t) {
                continue;
            }
            let r = t.text_range();
            if let Some(slice) = byte_slice(src, r) {
                if let Some(nu) = nudge_number_with_rng(slice, rng) {
                    out.push(MutationCandidate::Replace {
                        range: r,
                        new_text: nu,
                    });
                }
            }
        } else if t.kind() == LeekSyntaxKind::Kw {
            if token_strictly_before_enclosing_for_body(&t) {
                continue;
            }
            let txt = t.text();
            if txt == "true" {
                out.push(MutationCandidate::Replace {
                    range: t.text_range(),
                    new_text: "false".to_string(),
                });
            } else if txt == "false" {
                out.push(MutationCandidate::Replace {
                    range: t.text_range(),
                    new_text: "true".to_string(),
                });
            }
        }
    }

    for node in root.descendants() {
        if node.kind() == LeekSyntaxKind::BinaryExpr {
            if node_strictly_before_enclosing_for_body(&node) {
                continue;
            }
            if node_has_index_expr_ancestor(&node) {
                continue;
            }
            // Swapping operands under `++` / `--` can change the update target shape and trip `INVALID_ASSIGN_TARGET`.
            if node_has_pre_post_update_ancestor(&node) {
                continue;
            }
            if let Some(new_s) = binary_swap_replacement(src, &node, parity_safe) {
                out.push(MutationCandidate::Replace {
                    range: node.text_range(),
                    new_text: new_s,
                });
            }
            if !parity_safe {
                if let Some(new_s) = binary_eq_ne_flip_replacement(src, &node) {
                    out.push(MutationCandidate::Replace {
                        range: node.text_range(),
                        new_text: new_s,
                    });
                }
            }
        }
        if node.kind() == LeekSyntaxKind::LiteralExpr {
            if literal_expr_is_include_path(&node) {
                continue;
            }
            if node_has_pre_post_update_ancestor(&node) {
                continue;
            }
            if node_strictly_before_enclosing_for_body(&node) {
                continue;
            }
            // Extra redundant parens around string literals (e.g. `say((("x")))` can diverge from Java.
            if literal_expr_is_string_literal(src, &node) {
                continue;
            }
            if node_has_index_expr_ancestor(&node) {
                continue;
            }
            let paren_chance = if parity_safe {
                0.0
            } else {
                match level {
                    2 => 0.18,
                    3 => 0.45,
                    _ => 0.65,
                }
            };
            if rng.gen_bool(paren_chance) {
                let r = node.text_range();
                if let Some(slice) = byte_slice(src, r) {
                    if !slice.is_empty() {
                        out.push(MutationCandidate::Replace {
                            range: r,
                            new_text: format!("({slice})"),
                        });
                    }
                }
            }
        }
    }

    let stmt_inject_allowed =
        level >= 4 && inject.complexity > 0 && source_file_top_level_stmt_count(root) > 1;

    if stmt_inject_allowed {
        for node in root.descendants() {
            let k = node.kind();
            // Only wrap “flat” statements. Wrapping `Block` / `if` / `while` / … produces `{{` or
            // `}{` shapes the official Java toolchain rejects (`OPEN_BLOC_REMAINING`, `Invalid AI`),
            // while Rust still parses — desyncing `fight.actions` vs the generator.
            let inject_wrap_stmt = matches!(
                k,
                LeekSyntaxKind::ExprStmt
                    | LeekSyntaxKind::AssignStmt
                    | LeekSyntaxKind::ReturnStmt
                    | LeekSyntaxKind::ThrowStmt
                    | LeekSyntaxKind::BreakStmt
                    | LeekSyntaxKind::ContinueStmt
                    | LeekSyntaxKind::EmptyStmt
                    | LeekSyntaxKind::GlobalStmt
            );
            if !inject_wrap_stmt {
                continue;
            }
            let r = node.text_range();
            let Some(slice) = byte_slice(src, r) else {
                continue;
            };
            if slice.trim().is_empty() {
                continue;
            }
            if rng.gen_range(0u8..=100) > inject.wrap_percent {
                continue;
            }
            // Scope-aware injection: reuse identifiers already declared in the file.
            let scope_idents = collect_declared_identifiers(root);
            let injected = {
                let n = inject.max_injected_stmts.clamp(1, 16);
                let count = rng.gen_range(1..=n);
                let tag = rng.gen::<u64>();
                let name = format!("__leekgen_fuzz_{tag:016x}");
                let mut outb = String::new();
                outb.push_str(&format!(
                    "var {name} = {};",
                    rng.gen_range(-10_000..=10_000)
                ));
                for _ in 1..count {
                    outb.push('\n');
                    outb.push_str(&injected_stmt(
                        rng,
                        inject.complexity,
                        &name,
                        &scope_idents,
                        inject.scope_aware_percent,
                    ));
                }
                outb
            };
            // Wider statement-level shapes for non-parity fuzzing.
            // These introduce more control-flow variation than a plain `{ ... }` wrapper.
            if !parity_safe && inject.complexity >= 3 && rng.gen_bool(0.35) {
                let wrapped = match rng.gen_range(0..=3u8) {
                    0 => format!("if (true) {{\n{slice}\n{injected}\n}} else {{ ; }}"),
                    1 => format!("if (false) {{ ; }} else {{\n{slice}\n{injected}\n}}"),
                    2 => format!("try {{\n{slice}\n{injected}\n}} catch (e) {{ ; }}"),
                    _ => format!(
                        "switch (0) {{ case 0: {{\n{slice}\n{injected}\n}} break; default: ; }}"
                    ),
                };
                out.push(MutationCandidate::Replace {
                    range: r,
                    new_text: wrapped,
                });
            } else {
                out.push(MutationCandidate::Replace {
                    range: r,
                    new_text: format!("{{\n{slice}\n{injected}\n}}"),
                });
            }
        }
    }

    out
}

fn injected_stmt(
    rng: &mut StdRng,
    complexity: u8,
    name: &str,
    scope_idents: &[String],
    scope_aware_percent: u8,
) -> String {
    let c = complexity.min(5);
    let roll = rng.gen_range(0..=20u8);
    let scope_aware = !scope_idents.is_empty() && rng.gen_range(0u8..=100) <= scope_aware_percent;

    if scope_aware && c >= 2 {
        let pick = scope_idents
            .choose(rng)
            .cloned()
            .unwrap_or_else(|| name.to_string());
        match rng.gen_range(0..=6u8) {
            0 => return format!("{pick} = {pick};"),
            1 => return format!("{pick} = {name};"),
            2 => return format!("{pick} = {pick} + 1;"),
            3 => return format!("if ({pick}) {{ {name} = {name}; }}"),
            4 => return format!("{name} = {pick};"),
            5 => return format!("debug({pick});"),
            _ => return format!("say({pick});"),
        }
    }
    match (c, roll) {
        (_, 0) => ";".to_string(),
        (_, 1) => format!("{name} = {name} + {};", rng.gen_range(-50..=50)),
        (_, 2) => format!("{name} = {name} * {};", rng.gen_range(-10..=10)),
        // Do not redeclare `var {name}` — `injected_block` already did.
        (_, 3) => format!(
            "{name} = [{}, {}, {}];",
            rng.gen_range(-9..=9),
            rng.gen_range(-9..=9),
            rng.gen_range(-9..=9)
        ),
        (_, 4) => format!(
            "{name} = ({} < {} ? {} : {});",
            rng.gen_range(-9..=9),
            rng.gen_range(-9..=9),
            rng.gen_range(-9..=9),
            rng.gen_range(-9..=9)
        ),
        (0 | 1, _) => format!(
            "{} + {};",
            rng.gen_range(-10_000..=10_000),
            rng.gen_range(-10_000..=10_000)
        ),
        (_, 5) => "if (true) { ; }".to_string(),
        (_, 6) => "if (false) { ; } else { ; }".to_string(),
        (_, 7) => "while (false) { ; }".to_string(),
        (_, 8) => "do { ; } while (false);".to_string(),
        (2..=5, 9) => format!("for (var i = 0; i < 0; i = i + 1) {{ {name} = i; }}"),
        (2..=5, 10) => "switch (0) { case 0: ; break; default: ; }".to_string(),
        (3..=5, 11) => "try { ; } catch (e) { ; }".to_string(),
        (3..=5, 12) => format!(
            "if (({} + {}) == {}) {{ {name} = {name}; }}",
            rng.gen_range(-9..=9),
            rng.gen_range(-9..=9),
            rng.gen_range(-9..=9)
        ),
        (4..=5, 13) => "var __tmp = [1,2,3]; __tmp[0] = __tmp[0] + 1;".to_string(),
        (4..=5, 14) => {
            "var __tmp = 0; for (var j = 0; j < 0; j = j + 1) { __tmp = __tmp + j; }".to_string()
        }
        (4..=5, 15) => "var __s = \"x\" + \"y\";".to_string(),
        (4..=5, 16) => "debug(\"leekgen-fuzz\");".to_string(),
        _ => format!(
            "{} * {};",
            rng.gen_range(-1000..=1000),
            rng.gen_range(-1000..=1000)
        ),
    }
}

fn pick_disjoint(candidates: Vec<MutationCandidate>, max: usize) -> Vec<MutationCandidate> {
    let mut picked = Vec::new();
    for c in candidates {
        let r = c.range();
        if picked
            .iter()
            .all(|p: &MutationCandidate| !ranges_overlap(p.range(), r))
        {
            picked.push(c);
            if picked.len() >= max {
                break;
            }
        }
    }
    picked
}

fn ranges_overlap(a: TextRange, b: TextRange) -> bool {
    a.start() < b.end() && b.start() < a.end()
}

fn apply_replacements(src: &str, picked: &[MutationCandidate]) -> Option<String> {
    let mut reps: Vec<(TextRange, String)> = picked
        .iter()
        .map(|c| match c {
            MutationCandidate::Replace { range, new_text } => (*range, new_text.clone()),
        })
        .collect();
    reps.sort_by_key(|(range, _)| std::cmp::Reverse(range.start()));
    let mut out = src.to_string();
    for (range, rep) in reps {
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        out.get(start..end)?;
        out.replace_range(start..end, &rep);
    }
    Some(out)
}

fn byte_slice(s: &str, r: TextRange) -> Option<&str> {
    let a: usize = r.start().into();
    let b: usize = r.end().into();
    s.get(a..b)
}

fn binary_swap_replacement(src: &str, node: &SynNode, parity_safe: bool) -> Option<String> {
    let (left, op_tok, right) = binary_expr_parts(node)?;
    if op_tok.kind() != LeekSyntaxKind::Operator {
        return None;
    }
    let op = op_tok.text();
    let ok = if parity_safe {
        op == "+" || op == "*" || op == "==" || op == "!="
    } else {
        op == "+" || op == "*" || op == "==" || op == "!=" || op == "&" || op == "|" || op == "^"
    };
    if !ok {
        return None;
    }
    let lr = left.text_range();
    let or = op_tok.text_range();
    let rr = right.text_range();
    let ls = byte_slice(src, lr)?;
    let os = byte_slice(src, or)?;
    let rs = byte_slice(src, rr)?;
    Some(format!("{rs}{os}{ls}"))
}

/// Flip `==` ↔ `!=` on the operator token, preserving surrounding trivia inside the binary node.
fn binary_eq_ne_flip_replacement(src: &str, node: &SynNode) -> Option<String> {
    let (_, op_tok, _) = binary_expr_parts(node)?;
    if op_tok.kind() != LeekSyntaxKind::Operator {
        return None;
    }
    let new_op = match op_tok.text() {
        "==" => "!=",
        "!=" => "==",
        _ => return None,
    };
    let nr = node.text_range();
    let full = byte_slice(src, nr)?;
    let n0: usize = nr.start().into();
    let op_s: usize = op_tok.text_range().start().into();
    let op_e: usize = op_tok.text_range().end().into();
    let mut out = full.to_string();
    out.replace_range((op_s - n0)..(op_e - n0), new_op);
    Some(out)
}

fn binary_expr_parts(
    node: &SynNode,
) -> Option<(SynNode, rowan::SyntaxToken<LeekLanguage>, SynNode)> {
    let mut exprs: Vec<SynNode> = Vec::new();
    let mut op: Option<rowan::SyntaxToken<LeekLanguage>> = None;
    for el in node.children_with_tokens() {
        match el {
            NodeOrToken::Node(n) if n.kind() == LeekSyntaxKind::Expr => exprs.push(n),
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Operator => op = Some(t),
            _ => {}
        }
    }
    if exprs.len() == 2 {
        let o = op?;
        Some((exprs[0].clone(), o, exprs[1].clone()))
    } else {
        None
    }
}

fn nudge_number_with_rng(text: &str, rng: &mut StdRng) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.starts_with("0x") || t.starts_with("0X") {
        return None;
    }
    if let Ok(i) = t.parse::<i64>() {
        let d = rng.gen_range(-4_i64..=4_i64);
        return Some(i.saturating_add(d).to_string());
    }
    if let Ok(f) = t.parse::<f64>() {
        if !f.is_finite() {
            return None;
        }
        let mag = f.abs().max(1e-6);
        let delta = rng.gen_range((-0.02 * mag)..=(0.02 * mag));
        let v = f + delta;
        if v == 0.0 {
            return Some("0".to_string());
        }
        if (v - v.round()).abs() < 1e-9 {
            return Some(format!("{}", v.round() as i64));
        }
        let s = format!("{v:.8}");
        return Some(s.trim_end_matches('0').trim_end_matches('.').to_string());
    }
    None
}

fn perturb_random_ascii_digit(s: &str, rng: &mut StdRng) -> Option<String> {
    let bs = s.as_bytes();
    let mut idx: Vec<usize> = Vec::new();
    for (i, &b) in bs.iter().enumerate() {
        if b.is_ascii_digit() {
            idx.push(i);
        }
    }
    if idx.is_empty() {
        return None;
    }
    let at = idx[rng.gen_range(0..idx.len())];
    let new_d = rng.gen_range(b'0'..=b'9');
    let mut v = bs.to_vec();
    v[at] = new_d;
    String::from_utf8(v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InjectSettings;
    use crate::MutateSettings;
    use leekscript_parser::parse_file_green;
    use rand::SeedableRng;

    fn assert_reparses(src: &str, seed: u64, level: u8) {
        let mut rng = StdRng::seed_from_u64(seed);
        let out = generate_mutant_candidate(src, &mut rng, level);
        for version in [4u8, 3, 2, 1] {
            let cfg = LexerConfig { version };
            let (tok, e) = Lexer::new(&out, cfg).tokenize();
            if !e.is_empty() {
                continue;
            }
            if parse_file_green(&out, &tok).is_ok() {
                return;
            }
        }
        panic!("re-parse failed for all versions\n---\n{out}\n---");
    }

    fn assert_many_reparse(srcs: &[&str], seeds: &[u64], levels: &[u8]) {
        for &src in srcs {
            for &level in levels {
                for &seed in seeds {
                    assert_reparses(src, seed, level);
                }
            }
        }
    }

    #[test]
    fn cst_mutate_keeps_simple_file_valid() {
        let src = "var x = 1 + 2;\nreturn x == 3;\n";
        assert_reparses(src, 42, 3);
        assert_reparses(src, 4242, 4);
    }

    /// Wider corpus for reparse invariants (more ops/branches/strings/includes).
    /// This intentionally does not assert semantic properties; only that mutation output stays syntactically valid.
    #[test]
    fn cst_mutate_keeps_a_small_corpus_valid_across_seeds_and_levels() {
        let corpus = [
            "var x = 1 + 2;\nreturn x == 3;\n",
            "var a = 1 == 1;\nvar b = 2 != 3;\nvar c = (4 & 5) | (6 ^ 7);\n",
            "var s = \"hello\";\ndebug(\"x:\" + s);\n",
            "var n = 0;\nwhile (n < 2) { n = n + 1; }\nreturn n;\n",
            concat!(
                "include(\"subfolder/library.leek\");\n",
                "var enemy = getNearestEnemy();\n",
                "if (enemy) { moveToward(enemy); }\n",
                "debug(\"w:\" + getWeapons());\n",
            ),
        ];
        // Keep runtime small but meaningfully wide.
        let seeds = [
            0u64,
            1,
            2,
            3,
            4,
            5,
            7,
            11,
            42,
            4242,
            99_001,
            0xDEADBEEF,
            0x0123_4567_89AB_CDEF,
        ];
        let levels = [2u8, 3, 4];
        assert_many_reparse(&corpus, &seeds, &levels);
    }

    #[test]
    fn cst_mutate_keeps_eq_ne_and_bitwise_valid() {
        let src = "var a = 1 == 1;\nvar b = 2 != 3;\nvar c = (4 & 5) | (6 ^ 7);\n";
        assert_reparses(src, 11, 3);
        assert_reparses(src, 22, 4);
    }

    #[test]
    fn cst_mutate_does_not_wrap_include_path_literal() {
        let src = concat!(
            "include(\"subfolder/library.leek\");\n",
            "var enemy =0;\n",
            "debug(\"w:\" + enemy);\n",
        );
        for seed in [42u64, 4242, 99_001, 0xDEADBEEF] {
            assert_reparses(src, seed, 3);
            assert_reparses(src, seed ^ 0xA5A5A5A5, 4);
        }
    }

    #[test]
    fn level1_only_comment() {
        let src = "var x = 1;\n";
        let mut rng = StdRng::seed_from_u64(7);
        let out = generate_mutant_candidate(src, &mut rng, 1);
        assert!(out.contains("leekgen-fuzz:"));
        assert!(out.starts_with("var x = 1;"));
    }

    #[test]
    fn require_parseable_rejects_invalid_fallback() {
        let src = "this is not leekscript {{{";
        let settings = MutateSettings::require_parseable();
        let mut rng = StdRng::seed_from_u64(1);
        let out = mutate_leek_source(src, &mut rng, 3, &settings).unwrap();
        assert_eq!(out.source, src);
        assert!(matches!(out.kind, OutcomeKind::RejectedAllAttempts { .. }));
    }

    /// Statement-inject must not run on one-liner libraries (`say` only) — Java rejects the wrapped shape.
    #[test]
    fn stmt_inject_only_when_multiple_top_level_statements() {
        let settings = MutateSettings {
            inject: InjectSettings {
                complexity: 5,
                wrap_percent: 100,
                max_injected_stmts: 3,
                scope_aware_percent: 100,
            },
            ..MutateSettings::default()
        };
        let mut rng = StdRng::seed_from_u64(0);

        let one_stmt = "say(\"a\");\n";
        let root1 = parse_best(one_stmt).unwrap();
        assert_eq!(super::source_file_top_level_stmt_count(&root1), 1);
        let c1 = super::collect_mutation_candidates(one_stmt, &root1, 4, &mut rng, &settings);
        assert!(
            !c1.iter().any(|m| match m {
                MutationCandidate::Replace { new_text, .. } => new_text.contains("__leekgen_fuzz"),
            }),
            "single top-level stmt: no inject"
        );

        let two_stmt = "var x = 1;\ndebug(x);\n";
        let root2 = parse_best(two_stmt).unwrap();
        assert!(super::source_file_top_level_stmt_count(&root2) >= 2);
        let c2 = super::collect_mutation_candidates(two_stmt, &root2, 4, &mut rng, &settings);
        assert!(
            c2.iter().any(|m| match m {
                MutationCandidate::Replace { new_text, .. } => new_text.contains("__leekgen_fuzz"),
            }),
            "multiple stmts: inject wrap allowed"
        );
    }

    #[test]
    fn require_compilable_errors_without_context() {
        let mut settings = MutateSettings::require_parseable();
        settings.acceptance = MutantAcceptance::RequireCompilable;
        let mut rng = StdRng::seed_from_u64(0);
        let err = mutate_leek_source("var x = 1;", &mut rng, 2, &settings).unwrap_err();
        assert_eq!(err, MutateError::MissingCompileContext);
    }
}
