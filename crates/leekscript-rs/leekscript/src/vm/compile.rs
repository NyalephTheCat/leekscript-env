//! Compile a tiny LeekScript subset from the CST into [`Bytecode`](super::bytecode::Bytecode).
//!
//! Covers numeric expressions, `null` / `true` / `false`, string literals, array / map literals
//! (`[]`, `[:]`, `[k: v]`), indexing `a[i]` and map member `m.field` (via `GetElem`), Java-style
//! `+` (string / array / map merge, `real` sum; `AI.add`-style operation charges for concat),
//! V4-style `==` / `!=` / `===` / `!==`, ordered comparisons (`AI.real` subset), `!`, short-circuit
//! `&&` / `||` / `and` / `or`, ternary `?:`, `if`, `while` / `do`-`while` / `for` (`for (;;)` /
//! `for (var i = 0; …; …)` — lowered on top of the same back-edge loop kernel as `while`, with a
//! for-only tail so `continue` hits the step before the condition), `for (x in arr)` on arrays
//! (via [`Opcode::ArrayLen`](super::opcode::Opcode::ArrayLen)), `global` / `const` declarations
//! (same local-slot model as `var`), `break` / `continue`, simple assignment `name = expr` and compound
//! `+=` / `-=` / `*=` / `/=` / `%=` (plain identifier LHS only), `var` with comma-separated
//! declarators, `return`, empty `;`, and expression statements.
//!
//! **Operation budget:** matches Java `AI.mOperations` — no generic per-opcode tick. Costs come from
//! [`Opcode::ChargeOps`](super::opcode::Opcode::ChargeOps) at statement boundaries (`if` / `while` /
//! `for` / `do`-`while` conditions, `return`, expression statements, `var`, assignments, `break` /
//! `continue`, for-step), plus runtime extras in the interpreter (e.g. string/array `+`, native calls).
//!
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use sipha::tree::ast::{AstNode, AstNodeExt, AstToken};
use sipha::tree::red::{SyntaxElement, SyntaxNode, SyntaxToken};
use sipha::types::{FromSyntaxKind, IntoSyntaxKind};

use crate::ast::{
    ArrayExpr, BinaryExpr, BracketMapExpr, CallExpr, CatchClause, ClassDecl, ClassMember, ConstDecl,
    DoWhileStmt, Expr, ForStmt, ForeachStmt, FunctionDecl, GlobalDecl, IfStmt, IndexExpr,
    IntervalExpr, LitStr, MemberExpr, ParenExpr, Root, Stmt, StmtBlock,
    SwitchStmt, TernaryExpr, ThrowStmt, TryStmt, UnaryExpr, VarDecl, WhileStmt,
};
use crate::ast::types::TypeExpr;
use crate::include;
use crate::parse::{
    ExperimentalFeatures, LanguageOptions, ParseError, Version, language_options_with_source_directives,
    parse_doc,
};
use crate::syntax::kinds::K;
use crate::syntax::syntax_el_is_trivia;

use super::bytecode::{Bytecode, BytecodeBuilder};
use super::java_ops;
use super::opcode::Opcode;
use super::value::Value;

/// One compiled `function` for [`Opcode::CallFunction`](super::opcode::Opcode::CallFunction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionEntry {
    pub name: String,
    pub entry_pc: usize,
    pub argc: u8,
    pub slot_base: u16,
    pub slot_count: u16,
}

/// Parse + bytecode + metadata needed to run on [`super::Vm`](super::Vm).
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledChunk {
    pub bytecode: Bytecode,
    /// Pass to [`super::Vm::set_local_count`](super::Vm::set_local_count) (returns [`super::VmError`](super::error::VmError) on RAM limit) before [`super::Vm::run`](super::Vm::run).
    pub local_slots: usize,
    /// Pass to [`super::Vm::set_functions`](super::interpreter::Vm::set_functions).
    pub functions: Vec<FunctionEntry>,
}

/// [`compile_chunk_v4_with_includes`] failure (I/O / merge / parse / compile).
#[derive(Debug)]
pub enum CompileChunkError {
    Load(include::IncludeLoadError),
    Merge(include::MergeIncludesError),
    Compile(CompileError),
}

impl fmt::Display for CompileChunkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(e) => write!(f, "{e}"),
            Self::Merge(e) => write!(f, "{e}"),
            Self::Compile(e) => write!(f, "{e}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CompileChunkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Load(e) => Some(e),
            Self::Merge(e) => Some(e),
            Self::Compile(e) => Some(e),
        }
    }
}

/// Failure to lower the CST to bytecode (unsupported syntax or parse error).
#[derive(Debug)]
pub enum CompileError {
    Parse(ParseError),
    Unsupported(&'static str),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e:?}"),
            Self::Unsupported(msg) => write!(f, "{msg}"),
        }
    }
}

impl From<ParseError> for CompileError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CompileError {}

fn vm_parse_options() -> LanguageOptions {
    LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            lexical_const: true,
            exceptions: true,
            ..ExperimentalFeatures::NONE
        },
    )
}

/// Parse `source` as V4 and compile all top-level statements into one bytecode chunk.
///
/// Parsing enables `const` ([`crate::parse::ExperimentalFeatures::lexical_const`]) and `try` /
/// `catch` / `throw` ([`ExperimentalFeatures::exceptions`](crate::parse::ExperimentalFeatures::exceptions)).
pub fn compile_chunk_v4(source: &str) -> Result<CompiledChunk, CompileError> {
    let doc = parse_doc(source, vm_parse_options())?;
    let root = Root::cast(doc.root().clone()).ok_or(CompileError::Unsupported(
        "parse tree root is not K::Root",
    ))?;
    compile_root(root)
}

/// Load `entry` from `project_root` with the same include rules as `leekscript check`, merge to one
/// source buffer, then compile like [`compile_chunk_v4`].
pub fn compile_chunk_v4_with_includes(
    project_root: &Path,
    entry: &Path,
) -> Result<CompiledChunk, CompileChunkError> {
    let lang = vm_parse_options();
    let project =
        include::load_project_with_includes(project_root, entry, lang).map_err(CompileChunkError::Load)?;
    let merged = include::merge_included_sources_to_single_file(project_root, &project)
        .map_err(CompileChunkError::Merge)?;
    let resolved = language_options_with_source_directives(&merged, lang);
    let doc = parse_doc(&merged, resolved).map_err(|e| CompileChunkError::Compile(e.into()))?;
    let root = Root::cast(doc.root().clone()).ok_or(CompileError::Unsupported(
        "parse tree root is not K::Root",
    ));
    let root = root.map_err(CompileChunkError::Compile)?;
    compile_root(root).map_err(CompileChunkError::Compile)
}

fn compile_root(root: Root) -> Result<CompiledChunk, CompileError> {
    let mut cx = CompileCtx::default();
    for stmt in AstNodeExt::children::<Stmt>(root.syntax()) {
        cx.compile_stmt(stmt)?;
    }
    Ok(CompiledChunk {
        bytecode: cx.builder.finish(),
        local_slots: usize::from(cx.next_local),
        functions: cx.functions,
    })
}

/// Identifiers bound by `for (a in …)` / `for (a : b in …)` (token scan aligned with scope analysis).
fn foreach_binding_idents(fe: &ForeachStmt) -> Vec<String> {
    let mut out = Vec::new();
    let mut after_for = false;
    for t in fe.syntax().child_tokens() {
        if syntax_el_is_trivia(&SyntaxElement::Token(t.clone())) {
            continue;
        }
        match K::from_syntax_kind(t.kind()) {
            Some(K::ForKw) => after_for = true,
            Some(K::InKw) => break,
            Some(K::Ident) if after_for => out.push(t.text().to_string()),
            _ => {}
        }
    }
    out
}

#[derive(Default)]
struct CompileCtx {
    builder: BytecodeBuilder,
    locals: HashMap<String, u16>,
    next_local: u16,
    break_scopes: Vec<BreakScope>,
    foreach_counter: u32,
    switch_tmp_id: u32,
    functions: Vec<FunctionEntry>,
    function_by_name: HashMap<String, u16>,
}

enum BreakScope {
    Loop {
        continue_fixups: Vec<usize>,
        break_fixups: Vec<usize>,
    },
    Switch {
        break_fixups: Vec<usize>,
    },
}

/// Elements between `(` and `)` in a [`K::ParenExpr`] (excluding the bracket tokens).
fn paren_expr_inner_elements(paren: &SyntaxNode) -> Result<Vec<SyntaxElement>, CompileError> {
    let full: Vec<_> = paren
        .children()
        .filter(|e| !syntax_el_is_trivia(e))
        .collect();
    let (Some(SyntaxElement::Token(open)), Some(SyntaxElement::Token(close))) =
        (full.first(), full.last())
    else {
        return Err(CompileError::Unsupported("parentheses shape"));
    };
    if open.kind() != K::LParen.into_syntax_kind()
        || close.kind() != K::RParen.into_syntax_kind()
    {
        return Err(CompileError::Unsupported("parentheses shape"));
    }
    if full.len() == 2 {
        return Err(CompileError::Unsupported("empty parentheses"));
    }
    Ok(full[1..full.len() - 1].to_vec())
}

/// Sipha may place `K::BinaryExpr` siblings next to an inner `K::Expr` under parentheses; peel one
/// `K::Expr` layer so infix-chain lowering sees `[lhs, BinaryExpr, …]`.
fn flatten_one_expr_layer(items: &[SyntaxElement]) -> Vec<SyntaxElement> {
    let mut out = Vec::new();
    for el in items {
        if let SyntaxElement::Node(node) = el {
            if node.kind() == K::Expr.into_syntax_kind() {
                for c in node.children() {
                    if syntax_el_is_trivia(&c) {
                        continue;
                    }
                    out.push(c.clone());
                }
                continue;
            }
        }
        out.push(el.clone());
    }
    out
}

/// If `semantic` is the non-trivia children of a `K::ArrayExpr`, detect map literal
/// shapes `[:]` or `[key: BracketMapExpr…]` and return key/value pairs in source order.
fn try_extract_map_literal_pairs(
    semantic: &[SyntaxElement],
) -> Result<Option<Vec<(Expr, Expr)>>, CompileError> {
    if semantic.len() < 3 {
        return Ok(None);
    }
    match (&semantic[0], semantic.last()) {
        (SyntaxElement::Token(lb), Some(SyntaxElement::Token(rb))) => {
            if lb.kind() != K::LBracket.into_syntax_kind()
                || rb.kind() != K::RBracket.into_syntax_kind()
            {
                return Ok(None);
            }
        }
        _ => return Ok(None),
    }

    let inner = &semantic[1..semantic.len() - 1];

    if inner.len() == 1 {
        if let SyntaxElement::Node(n) = &inner[0] {
            if let Some(b) = BracketMapExpr::cast(n.clone()) {
                let values: Vec<Expr> = AstNodeExt::children::<Expr>(b.syntax()).collect();
                if values.is_empty() {
                    return Ok(Some(Vec::new()));
                }
            }
        }
        return Ok(None);
    }

    // `[key: value, …]` — the grammar emits `EXPR` then `COLON` then `BRACKET_MAP_EXPR` under
    // `K::ArrayExpr` (see `bracket_list_or_map_inner`).
    if inner.len() >= 3 {
        if let (SyntaxElement::Node(nk), SyntaxElement::Token(col), SyntaxElement::Node(nb)) =
            (&inner[0], &inner[1], &inner[2])
        {
            if col.kind() == K::Colon.into_syntax_kind() {
                let key = Expr::cast(nk.clone());
                let bracket = BracketMapExpr::cast(nb.clone());
                if let (Some(k), Some(b)) = (key, bracket) {
                    let inner_vals: Vec<Expr> = AstNodeExt::children::<Expr>(b.syntax()).collect();
                    if inner_vals.is_empty() {
                        return Err(CompileError::Unsupported(
                            "map literal missing value after key",
                        ));
                    }
                    let mut pairs = vec![(k, inner_vals[0].clone())];
                    let rest = &inner_vals[1..];
                    if rest.len() % 2 != 0 {
                        return Err(CompileError::Unsupported("map literal"));
                    }
                    for ch in rest.chunks_exact(2) {
                        pairs.push((ch[0].clone(), ch[1].clone()));
                    }
                    return Ok(Some(pairs));
                }
            }
        }
    }

    Ok(None)
}

/// `for (… ident = …)` — name token immediately before `=` in the header.
fn ident_before_eq_in_for_header(syn: &SyntaxNode) -> Option<String> {
    let v: Vec<_> = syn
        .children()
        .filter(|e| !syntax_el_is_trivia(e))
        .collect();
    for i in 1..v.len() {
        if let SyntaxElement::Token(t) = &v[i] {
            if t.kind() == K::Eq.into_syntax_kind() {
                if let SyntaxElement::Token(prev) = &v[i - 1] {
                    if prev.kind() == K::Ident.into_syntax_kind() {
                        return Some(prev.text().to_string());
                    }
                }
                return None;
            }
        }
    }
    None
}

fn expr_element_as_plain_ident(el: &SyntaxElement) -> Option<String> {
    match el {
        SyntaxElement::Token(t) if t.kind() == K::Ident.into_syntax_kind() => {
            Some(t.text().to_string())
        }
        SyntaxElement::Node(n) => {
            let mut it = n.children().filter(|e| !syntax_el_is_trivia(e));
            let only = it.next()?;
            if it.next().is_some() {
                return None;
            }
            expr_element_as_plain_ident(&only)
        }
        _ => None,
    }
}

fn expr_plain_ident_from_expr(e: &Expr) -> Option<String> {
    let syn = e.syntax();
    let ch: Vec<_> = syn
        .children()
        .filter(|x| !syntax_el_is_trivia(x))
        .collect();
    match ch.as_slice() {
        [SyntaxElement::Token(t)] if t.kind() == K::Ident.into_syntax_kind() => {
            Some(t.text().to_string())
        }
        [SyntaxElement::Node(n)] => Expr::cast(n.clone()).and_then(|e| expr_plain_ident_from_expr(&e)),
        _ => None,
    }
}

/// Plain `ident = rhs` assignment (no `+=`, no chained `a = b = c`).
fn try_simple_assign_from_expr_parts(parts: &[SyntaxElement]) -> Option<(String, Expr)> {
    let len = parts.len();
    if len != 3 {
        return None;
    }
    let SyntaxElement::Token(t) = &parts[1] else {
        return None;
    };
    if t.kind() != K::Eq.into_syntax_kind() {
        return None;
    }
    let SyntaxElement::Node(rhs_n) = &parts[2] else {
        return None;
    };
    let rhs = Expr::cast(rhs_n.clone())?;
    let name = expr_element_as_plain_ident(&parts[0])?;
    Some((name, rhs))
}

fn compound_assign_binop(k: K) -> Option<K> {
    match k {
        K::PlusEq => Some(K::Plus),
        K::MinusEq => Some(K::Minus),
        K::StarEq => Some(K::Star),
        K::SlashEq => Some(K::Slash),
        K::PercentEq => Some(K::Percent),
        _ => None,
    }
}

/// Plain `ident += rhs` (and related); same shape as [`try_simple_assign_from_expr_parts`].
fn try_compound_assign_from_expr_parts(parts: &[SyntaxElement]) -> Option<(String, K, Expr)> {
    if parts.len() != 3 {
        return None;
    }
    let SyntaxElement::Token(t) = &parts[1] else {
        return None;
    };
    let Some(assign_k) = K::from_syntax_kind(t.kind()) else {
        return None;
    };
    compound_assign_binop(assign_k)?;
    let SyntaxElement::Node(rhs_n) = &parts[2] else {
        return None;
    };
    let rhs = Expr::cast(rhs_n.clone())?;
    let name = expr_element_as_plain_ident(&parts[0])?;
    Some((name, assign_k, rhs))
}

/// Shared loop lowering: one loop head (optional condition + exit jump), one body, then either a
/// `while`-style back-edge (`continue` → head) or a `for`-style tail (`continue` → step, then jump
/// to head). Keeps opcode sequences identical to the previous per-statement compilers so operation
/// counts stay the same (one charge per opcode dispatch, plus opcode-internal extras like string
/// `+`).
enum LoopTail {
    /// `while (cond)`: `continue` re-enters at the condition.
    WhileStyle,
    /// `for (; …; step)`: `continue` runs `step` (if any) before the condition; `step == None` is
    /// an empty step (same PCs as before).
    ForStyle {
        step: Option<Expr>,
    },
}

impl CompileCtx {
    fn alloc_local(&mut self, name: &str) -> u16 {
        if let Some(&i) = self.locals.get(name) {
            return i;
        }
        let i = self.next_local;
        self.next_local = self.next_local.saturating_add(1);
        self.locals.insert(name.to_string(), i);
        i
    }

    fn push_literal_token(&mut self, t: &SyntaxToken) -> Result<bool, CompileError> {
        let kind = t.kind();
        if kind == K::Number.into_syntax_kind() {
            let x = t
                .text()
                .parse::<f64>()
                .map_err(|_| CompileError::Unsupported("invalid number literal"))?;
            self.builder.emit_push_const(Value::Number(x));
            return Ok(true);
        }
        if kind == K::TrueKw.into_syntax_kind() {
            self.builder.emit_push_const(Value::Bool(true));
            return Ok(true);
        }
        if kind == K::FalseKw.into_syntax_kind() {
            self.builder.emit_push_const(Value::Bool(false));
            return Ok(true);
        }
        if kind == K::NullKw.into_syntax_kind() {
            self.builder.emit_opcode(Opcode::PushNull);
            return Ok(true);
        }
        if kind == K::String.into_syntax_kind() {
            let lit = LitStr::cast(t.clone()).ok_or(CompileError::Unsupported("string literal"))?;
            self.builder
                .emit_push_const(Value::String(lit.value()));
            return Ok(true);
        }
        Ok(false)
    }

    fn compile_stmt(&mut self, stmt: Stmt) -> Result<(), CompileError> {
        match stmt {
            Stmt::Return(r) => {
                if let Some(e) = r.expr() {
                    // `LeekReturnInstruction`: `ops(getOperations());` before evaluating the value.
                    let o = java_ops::java_analyzed_ops(&e);
                    if o > 0 {
                        self.builder.emit_charge_ops(o);
                    }
                    self.compile_expr(e)?;
                } else {
                    self.builder.emit_opcode(Opcode::PushNull);
                }
                self.builder.emit_return();
            }
            Stmt::Expr(es) => {
                if let Some(e) = es.expr() {
                    // `LeekExpressionInstruction`: `ops(expr, getOperations())` — eval then charge.
                    self.compile_expr(e.clone())?;
                    let o = java_ops::java_analyzed_ops(&e);
                    if o > 0 {
                        self.builder.emit_charge_ops(o);
                    }
                    self.builder.emit_opcode(Opcode::Pop);
                }
            }
            Stmt::VarDecl(v) => {
                self.compile_var_decl(v)?;
            }
            Stmt::If(i) => {
                self.compile_if_stmt(i)?;
            }
            Stmt::While(w) => {
                self.compile_while_stmt(w)?;
            }
            Stmt::DoWhile(d) => {
                self.compile_do_while_stmt(d)?;
            }
            Stmt::For(f) => {
                self.compile_for_stmt(f)?;
            }
            Stmt::Foreach(fe) => {
                self.compile_foreach_stmt(fe)?;
            }
            Stmt::Global(g) => {
                self.compile_global_decl(g)?;
            }
            Stmt::Const(c) => {
                self.compile_const_decl(c)?;
            }
            Stmt::Break(_) => {
                self.compile_break_stmt()?;
            }
            Stmt::Continue(_) => {
                self.compile_continue_stmt()?;
            }
            Stmt::Empty(_) => {}
            Stmt::Function(fd) => {
                self.compile_function_decl(fd)?;
            }
            Stmt::Switch(sw) => {
                self.compile_switch_stmt(sw)?;
            }
            Stmt::Try(t) => {
                self.compile_try_stmt(t)?;
            }
            Stmt::Throw(th) => {
                self.compile_throw_stmt(th)?;
            }
            Stmt::Class(c) => {
                self.compile_class_decl(c)?;
            }
            Stmt::Include(_) => {
                return Err(CompileError::Unsupported(
                    "top-level include: use compile_chunk_v4_with_includes",
                ));
            }
            _ => {
                return Err(CompileError::Unsupported(
                    "statement kind not supported by VM compiler",
                ));
            }
        }
        Ok(())
    }

    fn compile_class_decl(&mut self, c: ClassDecl) -> Result<(), CompileError> {
        if c.template_params().is_some() {
            return Err(CompileError::Unsupported("generic class"));
        }
        if c.extends().is_some() {
            return Err(CompileError::Unsupported("class extends"));
        }
        let Some(body) = c.body() else {
            return Ok(());
        };
        for n in body.syntax().child_nodes() {
            if ClassMember::can_cast(n.kind()) {
                return Err(CompileError::Unsupported("class member"));
            }
        }
        if body.stmts().next().is_some() {
            return Err(CompileError::Unsupported("class body statement"));
        }
        Ok(())
    }

    fn compile_throw_stmt(&mut self, th: ThrowStmt) -> Result<(), CompileError> {
        let Some(e) = th.expr() else {
            return Err(CompileError::Unsupported("throw without value"));
        };
        let o = java_ops::java_analyzed_ops(&e);
        if o > 0 {
            self.builder.emit_charge_ops(o);
        }
        self.compile_expr(e)?;
        self.builder.emit_throw();
        Ok(())
    }

    fn compile_try_stmt(&mut self, t: TryStmt) -> Result<(), CompileError> {
        if t.finally_block().is_some() {
            return Err(CompileError::Unsupported("try/finally"));
        }
        let catches: Vec<CatchClause> = t.catch_clauses().collect();
        if catches.len() != 1 {
            return Err(CompileError::Unsupported("try/catch: need exactly one catch"));
        }
        let catch = &catches[0];
        let param = catch
            .param_name()
            .ok_or(CompileError::Unsupported("catch without parameter name"))?;
        let try_b = t
            .try_block()
            .ok_or(CompileError::Unsupported("try without block"))?;

        let try_off = self.builder.emit_try_begin_placeholder();
        for s in try_b.stmts() {
            self.compile_stmt(s)?;
        }
        self.builder.emit_try_end();
        let j_skip_catch = self.builder.emit_jump_placeholder();
        let catch_pc = self.builder.len();
        self.builder.patch_u32_at(try_off, catch_pc as u32);

        let pe = self.alloc_local(&param);
        self.builder.emit_opcode(Opcode::SetLocal);
        self.builder.emit_u16_operand(pe);
        let cb = catch
            .block()
            .ok_or(CompileError::Unsupported("catch without block"))?;
        for s in cb.stmts() {
            self.compile_stmt(s)?;
        }
        let merge = self.builder.len();
        self.builder.patch_i32_operand_at(
            j_skip_catch,
            merge as i32 - (j_skip_catch + 4) as i32,
        );
        Ok(())
    }

    fn compile_function_decl(&mut self, f: FunctionDecl) -> Result<(), CompileError> {
        let Some(name) = f.name() else {
            return Err(CompileError::Unsupported("function without name"));
        };
        if f.template_params().is_some() {
            return Err(CompileError::Unsupported("generic function"));
        }
        let params: Vec<_> = f.fn_params().collect();
        for p in &params {
            if p.default_expr().is_some() {
                return Err(CompileError::Unsupported("default function parameter"));
            }
        }
        let argc = u8::try_from(params.len())
            .map_err(|_| CompileError::Unsupported("too many parameters"))?;

        let j_skip = self.builder.emit_jump_placeholder();
        let entry_pc = self.builder.len();

        let slot_base = self.next_local;
        let saved_locals = core::mem::take(&mut self.locals);
        let saved_water = self.next_local;
        self.next_local = slot_base;
        self.locals = HashMap::new();
        for p in &params {
            let pn = p
                .name()
                .ok_or(CompileError::Unsupported("function parameter without name"))?;
            self.alloc_local(&pn);
        }
        let body = f
            .body()
            .ok_or(CompileError::Unsupported("function without body"))?;
        for s in body.stmts() {
            self.compile_stmt(s)?;
        }
        self.builder.emit_opcode(Opcode::PushNull);
        self.builder.emit_return();

        let slot_count = self
            .next_local
            .checked_sub(slot_base)
            .ok_or(CompileError::Unsupported("function locals"))?;
        let slot_count = u16::try_from(slot_count)
            .map_err(|_| CompileError::Unsupported("function frame too large"))?;

        let id = u16::try_from(self.functions.len())
            .map_err(|_| CompileError::Unsupported("too many functions"))?;
        self.functions.push(FunctionEntry {
            name: name.clone(),
            entry_pc,
            argc,
            slot_base,
            slot_count,
        });
        self.function_by_name.insert(name, id);

        self.locals = saved_locals;
        self.next_local = saved_water.max(slot_base.saturating_add(slot_count));

        let after_fn = self.builder.len();
        self.builder.patch_i32_operand_at(
            j_skip,
            after_fn as i32 - (j_skip + 4) as i32,
        );
        Ok(())
    }

    fn compile_switch_stmt(&mut self, sw: SwitchStmt) -> Result<(), CompileError> {
        let disc = sw
            .expr()
            .ok_or(CompileError::Unsupported("switch without expression"))?;
        let tmp_slot = self.alloc_local(&format!("__sw{}", self.switch_tmp_id));
        self.switch_tmp_id = self.switch_tmp_id.saturating_add(1);
        let dco = java_ops::java_analyzed_ops(&disc);
        if dco > 0 {
            self.builder.emit_charge_ops(dco);
        }
        self.compile_expr(disc)?;
        self.builder.emit_opcode(Opcode::SetLocal);
        self.builder.emit_u16_operand(tmp_slot);

        self.break_scopes.push(BreakScope::Switch {
            break_fixups: Vec::new(),
        });

        let arms: Vec<_> = sw.arms().collect();
        let mut default_ix: Option<usize> = None;
        for (ix, arm) in arms.iter().enumerate() {
            if arm.case_exprs().next().is_none() {
                default_ix = Some(ix);
            }
        }

        let mut to_next_arm: Vec<usize> = Vec::new();
        let mut to_merge: Vec<usize> = Vec::new();

        for (i, arm) in arms.iter().enumerate() {
            if Some(i) == default_ix {
                continue;
            }
            for off in to_next_arm.drain(..) {
                self.builder.patch_i32_operand_at(
                    off,
                    self.builder.len() as i32 - (off + 4) as i32,
                );
            }

            let cases: Vec<_> = arm.case_exprs().collect();
            let mut prev_false: Option<usize> = None;
            let mut hits: Vec<usize> = Vec::new();
            for ce in &cases {
                if let Some(pf) = prev_false.take() {
                    self.builder.patch_i32_operand_at(
                        pf,
                        self.builder.len() as i32 - (pf + 4) as i32,
                    );
                }
                self.builder.emit_opcode(Opcode::GetLocal);
                self.builder.emit_u16_operand(tmp_slot);
                self.compile_expr(ce.clone())?;
                self.builder.emit_opcode(Opcode::EqEquals);
                prev_false = Some(self.builder.emit_jump_if_false_placeholder());
                hits.push(self.builder.emit_jump_placeholder());
            }
            if let Some(pf) = prev_false {
                self.builder.patch_i32_operand_at(
                    pf,
                    self.builder.len() as i32 - (pf + 4) as i32,
                );
            }
            to_next_arm.push(self.builder.emit_jump_placeholder());

            let body_pc = self.builder.len();
            for h in hits {
                self.builder
                    .patch_i32_operand_at(h, body_pc as i32 - (h + 4) as i32);
            }
            for st in arm.stmts() {
                self.compile_stmt(st)?;
            }
            to_merge.push(self.builder.emit_jump_placeholder());
        }

        for off in to_next_arm.drain(..) {
            self.builder.patch_i32_operand_at(
                off,
                self.builder.len() as i32 - (off + 4) as i32,
            );
        }

        if let Some(dix) = default_ix {
            for st in arms[dix].stmts() {
                self.compile_stmt(st)?;
            }
            to_merge.push(self.builder.emit_jump_placeholder());
        }

        let merge_pc = self.builder.len();
        for off in to_merge {
            self.builder.patch_i32_operand_at(
                off,
                merge_pc as i32 - (off + 4) as i32,
            );
        }

        let break_fixups = match self.break_scopes.pop() {
            Some(BreakScope::Switch { break_fixups }) => break_fixups,
            _ => panic!("switch scope"),
        };
        for off in break_fixups {
            self.builder.patch_i32_operand_at(
                off,
                merge_pc as i32 - (off + 4) as i32,
            );
        }
        Ok(())
    }

    fn compile_break_stmt(&mut self) -> Result<(), CompileError> {
        self.builder.emit_charge_ops(1);
        let scope = self.break_scopes.last_mut().ok_or(CompileError::Unsupported(
            "break outside switch or loop",
        ))?;
        let off = self.builder.emit_jump_placeholder();
        match scope {
            BreakScope::Loop { break_fixups, .. } | BreakScope::Switch { break_fixups } => {
                break_fixups.push(off);
            }
        }
        Ok(())
    }

    fn compile_continue_stmt(&mut self) -> Result<(), CompileError> {
        self.builder.emit_charge_ops(1);
        for scope in self.break_scopes.iter_mut().rev() {
            if let BreakScope::Loop { continue_fixups, .. } = scope {
                let off = self.builder.emit_jump_placeholder();
                continue_fixups.push(off);
                return Ok(());
            }
        }
        Err(CompileError::Unsupported("continue outside any loop"))
    }

    fn patch_continue_fixups(&mut self, target_pc: usize) {
        for scope in self.break_scopes.iter_mut().rev() {
            if let BreakScope::Loop { continue_fixups, .. } = scope {
                for off in continue_fixups.iter().copied() {
                    self.builder.patch_i32_operand_at(
                        off,
                        target_pc as i32 - (off + 4) as i32,
                    );
                }
                continue_fixups.clear();
                return;
            }
        }
    }

    fn peel_loop_cond_syntax(&self, mut n: SyntaxNode) -> SyntaxNode {
        loop {
            if n.kind() == K::Expr.into_syntax_kind() {
                let non_triv: Vec<_> = n
                    .children()
                    .filter(|e| !syntax_el_is_trivia(e))
                    .collect();
                if non_triv.len() == 1 {
                    if let SyntaxElement::Node(c) = &non_triv[0] {
                        n = c.clone();
                        continue;
                    }
                }
            }
            if let Some(p) = ParenExpr::cast(n.clone()) {
                if let Ok(inner) = paren_expr_inner_elements(p.syntax()) {
                    if inner.len() == 1 {
                        if let SyntaxElement::Node(c) = &inner[0] {
                            n = c.clone();
                            continue;
                        }
                    }
                }
            }
            break;
        }
        n
    }

    fn compile_cond_chain_lhs(&mut self, parts: &[SyntaxElement]) -> Result<(), CompileError> {
        if parts.len() >= 2 && self.try_compile_infix_chain_on_parts(parts)? {
            return Ok(());
        }
        if parts.len() == 1 {
            return self.compile_suffix_atom(&parts[0]);
        }
        Err(CompileError::Unsupported("loop condition left-hand side"))
    }

    fn emit_loop_logical_chain(
        &mut self,
        op: K,
        lhs_parts: &[SyntaxElement],
        bin: &SyntaxNode,
    ) -> Result<bool, CompileError> {
        let suff = java_ops::suffix_after_first_binary_op(bin);
        match op {
            K::AndAnd => {
                self.compile_cond_chain_lhs(lhs_parts)?;
                let lo = java_ops::java_ops_expr_flat(lhs_parts);
                self.builder.emit_charge_ops(lo.saturating_add(1));
                self.builder.emit_opcode(Opcode::Dup);
                let jif = self.builder.emit_jump_if_false_placeholder();
                self.builder.emit_opcode(Opcode::Pop);
                self.compile_infix_suffix(&suff)?;
                let ro = java_ops::java_ops_infix_suffix(&suff);
                if ro > 0 {
                    self.builder.emit_charge_ops(ro);
                }
                let merge_pc = self.builder.len();
                let after_jif = jif + 4;
                self.builder
                    .patch_i32_operand_at(jif, merge_pc as i32 - after_jif as i32);
                Ok(true)
            }
            K::OrOr => {
                self.compile_cond_chain_lhs(lhs_parts)?;
                let lo = java_ops::java_ops_expr_flat(lhs_parts);
                self.builder.emit_charge_ops(lo.saturating_add(1));
                self.builder.emit_opcode(Opcode::Dup);
                let jif = self.builder.emit_jump_if_false_placeholder();
                let jmp = self.builder.emit_jump_placeholder();
                let l_rhs = self.builder.len();
                self.builder.emit_opcode(Opcode::Pop);
                self.compile_infix_suffix(&suff)?;
                let ro = java_ops::java_ops_infix_suffix(&suff);
                if ro > 0 {
                    self.builder.emit_charge_ops(ro);
                }
                let merge_pc = self.builder.len();
                let after_jif = jif + 4;
                self.builder
                    .patch_i32_operand_at(jif, l_rhs as i32 - after_jif as i32);
                let after_jmp = jmp + 4;
                self.builder
                    .patch_i32_operand_at(jmp, merge_pc as i32 - after_jmp as i32);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn try_compile_loop_short_circuit_cond(&mut self, n: &SyntaxNode) -> Result<bool, CompileError> {
        if BinaryExpr::can_cast(n.kind()) {
            let Some(op) = java_ops::first_binary_op_token(n) else {
                return Ok(false);
            };
            if matches!(op, K::AndAnd | K::OrOr) {
                let lhs = java_ops::prefix_before_first_binary_op(n);
                if lhs.is_empty() {
                    return Ok(false);
                }
                return self.emit_loop_logical_chain(op, &lhs, n);
            }
        }
        if n.kind() == K::Expr.into_syntax_kind() {
            let parts: Vec<_> = n
                .children()
                .filter(|e| !syntax_el_is_trivia(e))
                .collect();
            if parts.len() < 2 {
                return Ok(false);
            }
            let SyntaxElement::Node(bin) = parts.last().expect("len >= 2") else {
                return Ok(false);
            };
            if !BinaryExpr::can_cast(bin.kind()) {
                return Ok(false);
            }
            let Some(op) = java_ops::first_binary_op_token(bin) else {
                return Ok(false);
            };
            if !matches!(op, K::AndAnd | K::OrOr) {
                return Ok(false);
            }
            let lhs_parts = &parts[..parts.len() - 1];
            return self.emit_loop_logical_chain(op, lhs_parts, bin);
        }
        Ok(false)
    }

    /// Boolean condition for `if` / `while` / `for` / `do`-`while`: Java `ops(bool(e), e.getOperations())`
    /// (with `&&` / `||` split like `LeekExpression.writeJavaCode`).
    fn compile_boolean_condition_header(&mut self, cond: Expr) -> Result<(), CompileError> {
        let n = self.peel_loop_cond_syntax(cond.syntax().clone());
        if self.try_compile_loop_short_circuit_cond(&n)? {
            return Ok(());
        }
        self.compile_expr_from_syntax(n.clone())?;
        let charge = java_ops::java_analyzed_ops_syntax(&n);
        if charge > 0 {
            self.builder.emit_charge_ops(charge);
        }
        Ok(())
    }

    /// `while` and `for` share this layout; see [`LoopTail`].
    fn compile_back_edge_loop(
        &mut self,
        head_check: Option<Expr>,
        body: StmtBlock,
        tail: LoopTail,
    ) -> Result<(), CompileError> {
        let head_pc = self.builder.len();
        self.break_scopes.push(BreakScope::Loop {
            continue_fixups: Vec::new(),
            break_fixups: Vec::new(),
        });

        let j_exit = if let Some(cond) = head_check {
            self.compile_boolean_condition_header(cond)?;
            Some(self.builder.emit_jump_if_false_placeholder())
        } else {
            None
        };

        self.builder.emit_charge_ops(1);

        self.compile_stmt_block(&body)?;

        match tail {
            LoopTail::WhileStyle => {
                self.patch_continue_fixups(head_pc);
            }
            LoopTail::ForStyle { step } => {
                let step_start = self.builder.len();
                self.patch_continue_fixups(step_start);
                if let Some(e) = step {
                    self.compile_expr(e.clone())?;
                    self.builder.emit_opcode(Opcode::Pop);
                    let co = java_ops::java_analyzed_ops(&e);
                    if co > 0 {
                        self.builder.emit_charge_ops(co);
                    }
                }
            }
        }

        let j_back = self.builder.emit_jump_placeholder();
        self.builder.patch_i32_operand_at(
            j_back,
            head_pc as i32 - (j_back + 4) as i32,
        );

        let after = self.builder.len();
        if let Some(j) = j_exit {
            self.builder
                .patch_i32_operand_at(j, after as i32 - (j + 4) as i32);
        }

        let frame = match self.break_scopes.pop() {
            Some(BreakScope::Loop { break_fixups, .. }) => break_fixups,
            _ => panic!("loop stack"),
        };
        for off in frame {
            self.builder
                .patch_i32_operand_at(off, after as i32 - (off + 4) as i32);
        }
        Ok(())
    }

    fn compile_while_stmt(&mut self, w: WhileStmt) -> Result<(), CompileError> {
        let Some(cond) = w.condition() else {
            return Err(CompileError::Unsupported("while without condition"));
        };
        let body = w
            .body()
            .ok_or(CompileError::Unsupported("while without body"))?;
        self.compile_back_edge_loop(Some(cond), body, LoopTail::WhileStyle)
    }

    fn compile_do_while_stmt(&mut self, d: DoWhileStmt) -> Result<(), CompileError> {
        let body = d
            .body()
            .ok_or(CompileError::Unsupported("do-while without body"))?;
        let Some(cond) = d.condition() else {
            return Err(CompileError::Unsupported("do-while without condition"));
        };

        let body_start = self.builder.len();
        self.break_scopes.push(BreakScope::Loop {
            continue_fixups: Vec::new(),
            break_fixups: Vec::new(),
        });

        self.builder.emit_charge_ops(1);
        self.compile_stmt_block(&body)?;

        let cond_pc = self.builder.len();
        self.patch_continue_fixups(cond_pc);

        self.compile_boolean_condition_header(cond)?;
        let j_exit = self.builder.emit_jump_if_false_placeholder();
        let j_repeat = self.builder.emit_jump_placeholder();
        self.builder.patch_i32_operand_at(
            j_repeat,
            body_start as i32 - (j_repeat + 4) as i32,
        );

        let after = self.builder.len();
        self.builder
            .patch_i32_operand_at(j_exit, after as i32 - (j_exit + 4) as i32);

        let frame = match self.break_scopes.pop() {
            Some(BreakScope::Loop { break_fixups, .. }) => break_fixups,
            _ => panic!("loop stack"),
        };
        for off in frame {
            self.builder
                .patch_i32_operand_at(off, after as i32 - (off + 4) as i32);
        }
        Ok(())
    }

    fn compile_for_stmt(&mut self, f: ForStmt) -> Result<(), CompileError> {
        if let Some(name) = ident_before_eq_in_for_header(f.syntax()) {
            if let Some(init_e) = f.init_expr() {
                let slot = self.alloc_local(&name);
                self.compile_expr(init_e.clone())?;
                self.builder.emit_charge_ops(
                    1u32.saturating_add(java_ops::java_analyzed_ops(&init_e)),
                );
                self.builder.emit_opcode(Opcode::SetLocal);
                self.builder.emit_u16_operand(slot);
            }
        } else if let Some(init_e) = f.init_expr() {
            self.compile_expr(init_e.clone())?;
            let o = java_ops::java_analyzed_ops(&init_e);
            if o > 0 {
                self.builder.emit_charge_ops(o);
            }
            self.builder.emit_opcode(Opcode::Pop);
        }

        let body = f
            .body()
            .ok_or(CompileError::Unsupported("for without body"))?;
        self.compile_back_edge_loop(
            f.condition_expr(),
            body,
            LoopTail::ForStyle {
                step: f.step_expr(),
            },
        )
    }

    fn compile_assign_local(&mut self, name: &str, rhs: Expr) -> Result<(), CompileError> {
        let slot = *self
            .locals
            .get(name)
            .ok_or(CompileError::Unsupported("undefined variable"))?;
        self.compile_expr(rhs.clone())?;
        let o = java_ops::java_analyzed_ops(&rhs);
        if o > 0 {
            self.builder.emit_charge_ops(o);
        }
        self.builder.emit_opcode(Opcode::Dup);
        self.builder.emit_opcode(Opcode::SetLocal);
        self.builder.emit_u16_operand(slot);
        Ok(())
    }

    fn compile_compound_assign_local(
        &mut self,
        name: &str,
        assign_op: K,
        rhs: Expr,
    ) -> Result<(), CompileError> {
        let bin = compound_assign_binop(assign_op).ok_or(CompileError::Unsupported(
            "compound assignment operator not supported by VM",
        ))?;
        let slot = *self
            .locals
            .get(name)
            .ok_or(CompileError::Unsupported("undefined variable"))?;
        self.builder.emit_opcode(Opcode::GetLocal);
        self.builder.emit_u16_operand(slot);
        self.compile_expr(rhs.clone())?;
        self.emit_binop(bin)?;
        self.builder.emit_opcode(Opcode::Dup);
        self.builder.emit_opcode(Opcode::SetLocal);
        self.builder.emit_u16_operand(slot);
        let c = java_ops::java_analyzed_ops(&rhs)
            .saturating_add(java_ops::compound_assign_bin_extra(assign_op));
        if c > 0 {
            self.builder.emit_charge_ops(c);
        }
        Ok(())
    }

    /// Emit the left operand of an infix chain or postfix receiver. `Ok(None)` means this element
    /// cannot start such a chain (caller should try another strategy).
    fn emit_expr_head_operand(
        &mut self,
        el: &SyntaxElement,
    ) -> Result<Option<()>, CompileError> {
        match el {
            SyntaxElement::Token(t) => {
                if self.push_literal_token(t)? {
                    return Ok(Some(()));
                }
                if t.kind() == K::Ident.into_syntax_kind() {
                    let name = t.text().to_string();
                    let slot = *self
                        .locals
                        .get(&name)
                        .ok_or(CompileError::Unsupported("undefined variable"))?;
                    self.builder.emit_opcode(Opcode::GetLocal);
                    self.builder.emit_u16_operand(slot);
                    return Ok(Some(()));
                }
                Ok(None)
            }
            SyntaxElement::Node(first) => {
                if BinaryExpr::can_cast(first.kind()) {
                    return Ok(None);
                }
                self.compile_expr_from_syntax(first.clone())?;
                Ok(Some(()))
            }
        }
    }

    fn try_compile_expr_parts_dispatch(&mut self, parts: &[SyntaxElement]) -> Result<bool, CompileError> {
        if let Some((name, rhs)) = try_simple_assign_from_expr_parts(parts) {
            self.compile_assign_local(&name, rhs)?;
            return Ok(true);
        }
        if let Some((name, op, rhs)) = try_compound_assign_from_expr_parts(parts) {
            self.compile_compound_assign_local(&name, op, rhs)?;
            return Ok(true);
        }
        if self.try_compile_ternary_suffix(parts)? {
            return Ok(true);
        }
        if self.try_compile_postfix_chain_on_parts(parts)? {
            return Ok(true);
        }
        if parts.len() >= 2 && self.try_compile_infix_chain_on_parts(parts)? {
            return Ok(true);
        }
        Ok(false)
    }

    fn compile_subexpr_from_parts(&mut self, parts: &[SyntaxElement]) -> Result<(), CompileError> {
        if parts.is_empty() {
            return Err(CompileError::Unsupported("empty expression"));
        }
        if self.try_compile_expr_parts_dispatch(parts)? {
            return Ok(());
        }
        if parts.len() == 1 {
            return self.compile_suffix_atom(&parts[0]);
        }
        Err(CompileError::Unsupported("expression shape not supported"))
    }

    fn try_compile_ternary_suffix(&mut self, parts: &[SyntaxElement]) -> Result<bool, CompileError> {
        let Some(SyntaxElement::Node(tn)) = parts.last() else {
            return Ok(false);
        };
        if !TernaryExpr::can_cast(tn.kind()) {
            return Ok(false);
        }
        if parts.len() < 2 {
            return Err(CompileError::Unsupported("ternary without condition"));
        }
        let tern = TernaryExpr::cast(tn.clone()).ok_or(CompileError::Unsupported("ternary"))?;
        let arms: Vec<Expr> = AstNodeExt::children::<Expr>(tern.syntax()).collect();
        if arms.len() != 2 {
            return Err(CompileError::Unsupported("ternary"));
        }
        let cond_parts = &parts[..parts.len() - 1];
        self.compile_subexpr_from_parts(cond_parts)?;
        let jelse = self.builder.emit_jump_if_false_placeholder();
        self.compile_expr(arms[0].clone())?;
        let jend = self.builder.emit_jump_placeholder();
        let else_pc = self.builder.len();
        self.builder.patch_i32_operand_at(
            jelse,
            else_pc as i32 - (jelse + 4) as i32,
        );
        self.compile_expr(arms[1].clone())?;
        let end_pc = self.builder.len();
        self.builder.patch_i32_operand_at(
            jend,
            end_pc as i32 - (jend + 4) as i32,
        );
        Ok(true)
    }

    fn index_expr_single_subscript(ix: &IndexExpr) -> Result<Expr, CompileError> {
        let syn = ix.syntax();
        for el in syn.children() {
            if syntax_el_is_trivia(&el) {
                continue;
            }
            if let SyntaxElement::Token(t) = &el {
                if t.kind() == K::Colon.into_syntax_kind() {
                    return Err(CompileError::Unsupported("slice index"));
                }
            }
        }
        let exprs: Vec<Expr> = AstNodeExt::children::<Expr>(syn).collect();
        match exprs.len() {
            0 => Err(CompileError::Unsupported("empty index")),
            1 => Ok(exprs[0].clone()),
            _ => Err(CompileError::Unsupported("multi-part index")),
        }
    }

    fn member_expr_field_name(m: &MemberExpr) -> Result<String, CompileError> {
        for el in m.syntax().children() {
            if syntax_el_is_trivia(&el) {
                continue;
            }
            if let SyntaxElement::Token(t) = &el {
                if t.kind() == K::Ident.into_syntax_kind() {
                    return Ok(t.text().to_string());
                }
            }
        }
        Err(CompileError::Unsupported(
            "member access needs a simple identifier field",
        ))
    }

    fn compile_index_suffix(&mut self, ix: &IndexExpr) -> Result<(), CompileError> {
        let sub = Self::index_expr_single_subscript(ix)?;
        self.compile_expr(sub)?;
        self.builder.emit_opcode(Opcode::GetElem);
        Ok(())
    }

    fn compile_member_suffix(&mut self, m: &MemberExpr) -> Result<(), CompileError> {
        let field = Self::member_expr_field_name(m)?;
        self.builder.emit_push_const(Value::String(field));
        self.builder.emit_opcode(Opcode::GetElem);
        Ok(())
    }

    fn try_compile_postfix_chain_on_parts(
        &mut self,
        parts: &[SyntaxElement],
    ) -> Result<bool, CompileError> {
        // Sipha postfix parsing: `ident ( args )` is `[Ident, CallExpr]` — the call node has only
        // argument expressions (callee is the preceding operand), not a callee child.
        if parts.len() == 2 {
            if let SyntaxElement::Node(n) = &parts[1] {
                if let Some(call) = CallExpr::cast(n.clone()) {
                    if let Some(name) = expr_element_as_plain_ident(&parts[0]) {
                        if self.locals.contains_key(&name) {
                            return Err(CompileError::Unsupported(
                                "indirect calls (calling through a local variable) are not supported by the VM compiler",
                            ));
                        }
                        let Some(&fid) = self.function_by_name.get(&name) else {
                            return Err(CompileError::Unsupported("call to unknown function"));
                        };
                        let args: Vec<Expr> =
                            AstNodeExt::children::<Expr>(call.syntax()).collect();
                        let argc = u8::try_from(args.len())
                            .map_err(|_| CompileError::Unsupported("too many call arguments"))?;
                        for a in &args {
                            self.compile_expr(a.clone())?;
                        }
                        let o = java_ops::java_analyzed_ops_syntax(call.syntax());
                        if o > 0 {
                            self.builder.emit_charge_ops(o);
                        }
                        self.builder.emit_call_function(fid, argc);
                        return Ok(true);
                    }
                }
            }
        }
        if parts.len() < 2 {
            return Ok(false);
        }
        for p in &parts[1..] {
            let SyntaxElement::Node(n) = p else {
                return Ok(false);
            };
            let k = n.kind();
            if !IndexExpr::can_cast(k) && !MemberExpr::can_cast(k) {
                return Ok(false);
            }
        }
        match self.emit_expr_head_operand(&parts[0])? {
            None => return Ok(false),
            Some(()) => {}
        }
        for p in &parts[1..] {
            let SyntaxElement::Node(n) = p else {
                unreachable!("validated");
            };
            if let Some(ix) = IndexExpr::cast(n.clone()) {
                self.compile_index_suffix(&ix)?;
            } else if let Some(mx) = MemberExpr::cast(n.clone()) {
                self.compile_member_suffix(&mx)?;
            } else {
                unreachable!("validated");
            }
        }
        Ok(true)
    }

    fn compile_stmt_block(&mut self, sb: &StmtBlock) -> Result<(), CompileError> {
        match sb {
            StmtBlock::Block(b) => {
                for s in b.stmts() {
                    self.compile_stmt(s)?;
                }
                Ok(())
            }
            StmtBlock::Wrapped(st) => self.compile_stmt(st.clone()),
        }
    }

    fn compile_if_stmt(&mut self, i: IfStmt) -> Result<(), CompileError> {
        let Some(cond) = i.condition() else {
            return Err(CompileError::Unsupported("if without condition"));
        };
        self.compile_boolean_condition_header(cond)?;
        let jif_op = self.builder.emit_jump_if_false_placeholder();
        let then_sb = i
            .then_branch()
            .ok_or(CompileError::Unsupported("if without body"))?;
        self.compile_stmt_block(&then_sb)?;
        if let Some(else_sb) = i.else_branch() {
            let jmp_end = self.builder.emit_jump_placeholder();
            let else_start = self.builder.len();
            self.builder.patch_i32_operand_at(
                jif_op,
                else_start as i32 - (jif_op + 4) as i32,
            );
            self.compile_stmt_block(&else_sb)?;
            let merge = self.builder.len();
            self.builder.patch_i32_operand_at(
                jmp_end,
                merge as i32 - (jmp_end + 4) as i32,
            );
        } else {
            let merge = self.builder.len();
            self.builder.patch_i32_operand_at(
                jif_op,
                merge as i32 - (jif_op + 4) as i32,
            );
        }
        Ok(())
    }

    fn compile_var_decl(&mut self, v: VarDecl) -> Result<(), CompileError> {
        let elts: Vec<_> = v
            .syntax()
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();
        let mut i = 0usize;
        if let Some(SyntaxElement::Token(t)) = elts.get(i) {
            if matches!(
                K::from_syntax_kind(t.kind()),
                Some(K::VarKw) | Some(K::LetKw)
            ) {
                i += 1;
            }
        }
        self.compile_declarator_list(&elts[i..], false)
    }

    fn compile_const_decl(&mut self, c: ConstDecl) -> Result<(), CompileError> {
        let elts: Vec<_> = c
            .syntax()
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();
        let mut i = 0usize;
        if let Some(SyntaxElement::Token(t)) = elts.get(i) {
            if K::from_syntax_kind(t.kind()) == Some(K::ConstKw) {
                i += 1;
            }
        }
        self.compile_declarator_list(&elts[i..], true)
    }

    fn compile_global_decl(&mut self, g: GlobalDecl) -> Result<(), CompileError> {
        let elts: Vec<_> = g
            .syntax()
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();
        let mut i = 0usize;
        if let Some(SyntaxElement::Token(t)) = elts.get(i) {
            if K::from_syntax_kind(t.kind()) == Some(K::GlobalKw) {
                i += 1;
            }
        }
        if let Some(SyntaxElement::Node(n)) = elts.get(i) {
            if TypeExpr::can_cast(n.kind()) {
                i += 1;
            }
        }
        self.compile_declarator_list(&elts[i..], false)
    }

    /// Comma-separated `ident (= expr)?` after the leading keyword / optional type (`var` / `const` / `global`).
    fn compile_declarator_list(
        &mut self,
        elts: &[SyntaxElement],
        require_initializer: bool,
    ) -> Result<(), CompileError> {
        let mut i = 0usize;
        while i < elts.len() {
            if let SyntaxElement::Token(t) = &elts[i] {
                if matches!(K::from_syntax_kind(t.kind()), Some(K::Semi)) {
                    break;
                }
            }
            let SyntaxElement::Token(name_t) = &elts[i] else {
                return Err(CompileError::Unsupported(
                    "typed or complex declarator not supported by VM compiler",
                ));
            };
            if name_t.kind() != K::Ident.into_syntax_kind() {
                return Err(CompileError::Unsupported(
                    "declarator: expected identifier",
                ));
            }
            let name = name_t.text().to_string();
            i += 1;
            let slot = self.alloc_local(&name);
            let mut initialized = false;
            if i < elts.len() {
                if let SyntaxElement::Token(t) = &elts[i] {
                    if t.kind() == K::Eq.into_syntax_kind() {
                        i += 1;
                        let Some(SyntaxElement::Node(n)) = elts.get(i) else {
                            return Err(CompileError::Unsupported(
                                "declarator missing initializer expression",
                            ));
                        };
                        let expr = Expr::cast(n.clone()).ok_or(CompileError::Unsupported(
                            "declarator malformed initializer",
                        ))?;
                        self.compile_expr(expr.clone())?;
                        self.builder.emit_charge_ops(
                            1u32.saturating_add(java_ops::java_analyzed_ops(&expr)),
                        );
                        i += 1;
                        initialized = true;
                    }
                }
            }
            if !initialized {
                if require_initializer {
                    return Err(CompileError::Unsupported(
                        "const declaration requires initializer",
                    ));
                }
                self.builder.emit_opcode(Opcode::PushNull);
                self.builder.emit_charge_ops(1);
            }
            self.builder.emit_opcode(Opcode::SetLocal);
            self.builder.emit_u16_operand(slot);
            if i < elts.len() {
                if let SyntaxElement::Token(t) = &elts[i] {
                    if t.kind() == K::Comma.into_syntax_kind() {
                        i += 1;
                        continue;
                    }
                    if t.kind() == K::Semi.into_syntax_kind() {
                        break;
                    }
                }
            }
            break;
        }
        Ok(())
    }

    fn compile_foreach_stmt(&mut self, fe: ForeachStmt) -> Result<(), CompileError> {
        let binds = foreach_binding_idents(&fe);
        if binds.is_empty() {
            return Err(CompileError::Unsupported("foreach without binding"));
        }
        if binds.len() > 2 {
            return Err(CompileError::Unsupported("foreach binding"));
        }
        let Some(iter_e) = fe.iterable() else {
            return Err(CompileError::Unsupported("foreach without iterable"));
        };
        let body = fe
            .body()
            .ok_or(CompileError::Unsupported("foreach without body"))?;

        let id = self.foreach_counter;
        self.foreach_counter = self.foreach_counter.saturating_add(1);
        let cont_slot = self.alloc_local(&format!("__fe{id}_c"));
        let i_slot = self.alloc_local(&format!("__fe{id}_i"));

        let (elem_k_slot, elem_v_slot, use_map) = if binds.len() == 2 {
            (
                Some(self.alloc_local(&binds[0])),
                Some(self.alloc_local(&binds[1])),
                true,
            )
        } else {
            (None, Some(self.alloc_local(&binds[0])), false)
        };
        let v_slot = elem_v_slot.expect("one or two binds");

        self.compile_expr(iter_e.clone())?;
        self.builder.emit_charge_ops(
            1u32.saturating_add(java_ops::java_analyzed_ops(&iter_e)),
        );
        self.builder.emit_opcode(Opcode::SetLocal);
        self.builder.emit_u16_operand(cont_slot);
        self.builder.emit_push_const(Value::Number(0.0));
        self.builder.emit_opcode(Opcode::SetLocal);
        self.builder.emit_u16_operand(i_slot);

        let head_pc = self.builder.len();
        self.break_scopes.push(BreakScope::Loop {
            continue_fixups: Vec::new(),
            break_fixups: Vec::new(),
        });

        self.builder.emit_opcode(Opcode::GetLocal);
        self.builder.emit_u16_operand(i_slot);
        self.builder.emit_opcode(Opcode::GetLocal);
        self.builder.emit_u16_operand(cont_slot);
        if use_map {
            self.builder.emit_map_len();
        } else {
            self.builder.emit_array_len();
        }
        self.builder.emit_opcode(Opcode::Lt);
        self.builder.emit_charge_ops(2);
        let j_exit = self.builder.emit_jump_if_false_placeholder();

        self.builder.emit_charge_ops(1);

        self.builder.emit_opcode(Opcode::GetLocal);
        self.builder.emit_u16_operand(cont_slot);
        self.builder.emit_opcode(Opcode::GetLocal);
        self.builder.emit_u16_operand(i_slot);
        if use_map {
            self.builder.emit_map_entry_at();
            self.builder.emit_opcode(Opcode::SetLocal);
            self.builder.emit_u16_operand(v_slot);
            self.builder.emit_opcode(Opcode::SetLocal);
            self.builder.emit_u16_operand(elem_k_slot.expect("map foreach"));
        } else {
            self.builder.emit_opcode(Opcode::GetElem);
            self.builder.emit_opcode(Opcode::SetLocal);
            self.builder.emit_u16_operand(v_slot);
        }

        self.compile_stmt_block(&body)?;

        let step_pc = self.builder.len();
        self.patch_continue_fixups(step_pc);

        self.builder.emit_opcode(Opcode::GetLocal);
        self.builder.emit_u16_operand(i_slot);
        self.builder.emit_push_const(Value::Number(1.0));
        self.builder.emit_opcode(Opcode::Add);
        self.builder.emit_opcode(Opcode::SetLocal);
        self.builder.emit_u16_operand(i_slot);

        let j_back = self.builder.emit_jump_placeholder();
        self.builder.patch_i32_operand_at(
            j_back,
            head_pc as i32 - (j_back + 4) as i32,
        );

        let after = self.builder.len();
        self.builder
            .patch_i32_operand_at(j_exit, after as i32 - (j_exit + 4) as i32);

        let frame = match self.break_scopes.pop() {
            Some(BreakScope::Loop { break_fixups, .. }) => break_fixups,
            _ => panic!("loop stack"),
        };
        for off in frame {
            self.builder
                .patch_i32_operand_at(off, after as i32 - (off + 4) as i32);
        }
        Ok(())
    }

    fn compile_call_expr(&mut self, call: &CallExpr) -> Result<(), CompileError> {
        let subs: Vec<Expr> = AstNodeExt::children::<Expr>(call.syntax()).collect();
        let Some(callee) = subs.first() else {
            return Err(CompileError::Unsupported("empty call"));
        };
        let args = &subs[1..];
        let name = expr_plain_ident_from_expr(callee).ok_or(CompileError::Unsupported(
            "call callee must be a simple identifier",
        ))?;
        let Some(&fid) = self.function_by_name.get(&name) else {
            return Err(CompileError::Unsupported("call to unknown function"));
        };
        let argc = u8::try_from(args.len())
            .map_err(|_| CompileError::Unsupported("too many call arguments"))?;
        for a in args {
            self.compile_expr(a.clone())?;
        }
        let expr = Expr::Call(call.clone());
        let o = java_ops::java_analyzed_ops(&expr);
        if o > 0 {
            self.builder.emit_charge_ops(o);
        }
        self.builder.emit_call_function(fid, argc);
        Ok(())
    }

    fn compile_array_literal(&mut self, arr: &ArrayExpr) -> Result<(), CompileError> {
        let syn = arr.syntax();
        let semantic: Vec<_> = syn
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();
        for el in &semantic {
            if let SyntaxElement::Node(n) = el {
                if IntervalExpr::can_cast(n.kind()) {
                    return Err(CompileError::Unsupported(
                        "interval literals are not supported by the VM compiler",
                    ));
                }
            }
        }

        if let Some(pairs) = try_extract_map_literal_pairs(&semantic)? {
            let n = u16::try_from(pairs.len())
                .map_err(|_| CompileError::Unsupported("map literal too large"))?;
            for (k, v) in pairs {
                self.compile_expr(k)?;
                self.compile_expr(v)?;
            }
            self.builder.emit_map_build(n);
            return Ok(());
        }

        let items: Vec<Expr> = AstNodeExt::children::<Expr>(syn).collect();
        let cnt = u16::try_from(items.len())
            .map_err(|_| CompileError::Unsupported("array literal too large"))?;
        for e in items {
            self.compile_expr(e)?;
        }
        self.builder.emit_array_build(cnt);
        Ok(())
    }

    fn compile_expr(&mut self, expr: Expr) -> Result<(), CompileError> {
        self.compile_expr_from_syntax(expr.syntax().clone())
    }

    /// Lower an expression given any [`SyntaxNode`](SyntaxNode) that appears under `K::Expr`.
    ///
    /// Sipha’s [`left_assoc_infix_level`](sipha::parse::expr::left_assoc_infix_level) produces two
    /// shapes we handle:
    /// - **Level root:** `[lhs, BinaryExpr, BinaryExpr, …]` (left operand + repeated `op rhs` bins).
    /// - **Inside each [`BinaryExpr`](BinaryExpr):** `op` token then a **suffix** (`NUMBER`, nested
    ///   `BinaryExpr`, …) — not always a single rhs subtree (e.g. `+` then `3` then `* 4`).
    fn compile_expr_from_syntax(&mut self, n: SyntaxNode) -> Result<(), CompileError> {
        if n.kind() == K::Expr.into_syntax_kind() {
            let parts: Vec<_> = n
                .children()
                .filter(|e| !syntax_el_is_trivia(e))
                .collect();
            if self.try_compile_expr_parts_dispatch(&parts)? {
                return Ok(());
            }
            for el in n.children() {
                if syntax_el_is_trivia(&el) {
                    continue;
                }
                if let SyntaxElement::Node(c) = el {
                    return self.compile_expr_from_syntax(c.clone());
                }
                break;
            }
        }
        if self.try_compile_infix_chain(&n)? {
            return Ok(());
        }
        if BinaryExpr::can_cast(n.kind()) {
            let lhs = java_ops::prefix_before_first_binary_op(&n);
            let lhs_o = java_ops::java_ops_expr_flat(&lhs);
            return self.compile_binary_fragment(&n, lhs_o);
        }
        if let Some(arr) = ArrayExpr::cast(n.clone()) {
            return self.compile_array_literal(&arr);
        }
        if let Some(call) = CallExpr::cast(n.clone()) {
            return self.compile_call_expr(&call);
        }
        if let Some(u) = UnaryExpr::cast(n.clone()) {
            return self.compile_unary(&u);
        }
        if let Some(p) = ParenExpr::cast(n.clone()) {
            let inner = paren_expr_inner_elements(p.syntax())?;
            let flat = flatten_one_expr_layer(&inner);
            if self.try_compile_expr_parts_dispatch(&flat)? {
                return Ok(());
            }
            if flat.len() == 1 {
                return match &flat[0] {
                    SyntaxElement::Node(c) => self.compile_expr_from_syntax(c.clone()),
                    SyntaxElement::Token(t) => {
                        if self.push_literal_token(t)? {
                            return Ok(());
                        }
                        if t.kind() == K::Ident.into_syntax_kind() {
                            let name = t.text().to_string();
                            let slot = *self
                                .locals
                                .get(&name)
                                .ok_or(CompileError::Unsupported("undefined variable"))?;
                            self.builder.emit_opcode(Opcode::GetLocal);
                            self.builder.emit_u16_operand(slot);
                            return Ok(());
                        }
                        Err(CompileError::Unsupported(
                            "expression shape not supported",
                        ))
                    }
                };
            }
            return Err(CompileError::Unsupported("empty parentheses"));
        }
        let semantic: Vec<_> = n
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();
        if semantic.len() == 1 {
            match &semantic[0] {
                SyntaxElement::Node(c) => return self.compile_expr_from_syntax(c.clone()),
                SyntaxElement::Token(t) => {
                    if self.push_literal_token(t)? {
                        return Ok(());
                    }
                    if t.kind() == K::Ident.into_syntax_kind() {
                        let name = t.text().to_string();
                        let slot = *self
                            .locals
                            .get(&name)
                            .ok_or(CompileError::Unsupported("undefined variable"))?;
                        self.builder.emit_opcode(Opcode::GetLocal);
                        self.builder.emit_u16_operand(slot);
                        return Ok(());
                    }
                }
            }
        }
        Err(CompileError::Unsupported(
            "expression shape not supported",
        ))
    }

    fn try_compile_infix_chain(&mut self, n: &SyntaxNode) -> Result<bool, CompileError> {
        let parts: Vec<_> = n
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();
        self.try_compile_infix_chain_on_parts(&parts)
    }

    fn try_compile_infix_chain_on_parts(
        &mut self,
        parts: &[SyntaxElement],
    ) -> Result<bool, CompileError> {
        if parts.len() < 2 {
            return Ok(false);
        }
        match self.emit_expr_head_operand(&parts[0])? {
            None => return Ok(false),
            Some(()) => {}
        }
        for p in parts.iter().skip(1) {
            let SyntaxElement::Node(node) = p else {
                return Ok(false);
            };
            if !BinaryExpr::can_cast(node.kind()) {
                return Ok(false);
            }
        }
        let mut prefix_len = 1usize;
        for p in parts.iter().skip(1) {
            let SyntaxElement::Node(bin) = p else {
                unreachable!("validated above");
            };
            let Some(be) = BinaryExpr::cast(bin.clone()) else {
                return Err(CompileError::Unsupported("infix chain BinaryExpr"));
            };
            let lhs_ops = java_ops::java_ops_expr_flat(&parts[..prefix_len]);
            self.compile_binary_fragment(be.syntax(), lhs_ops)?;
            prefix_len += 1;
        }
        Ok(true)
    }

    /// One [`BinaryExpr`](BinaryExpr): stack already holds its left operand; emit the suffix after
    /// the operator token, then the opcode. `lhs_ops` is Java `getOperations()` for the value
    /// already on the stack (prefix of the infix chain, or lhs inside a lone `BinaryExpr`).
    fn compile_binary_fragment(
        &mut self,
        bin: &SyntaxNode,
        lhs_ops: u32,
    ) -> Result<(), CompileError> {
        let op = java_ops::first_binary_op_token(bin).ok_or(CompileError::Unsupported(
            "binary expression missing operator",
        ))?;
        let suff = java_ops::suffix_after_first_binary_op(bin);
        match op {
            K::AndAnd => self.compile_short_circuit_and(&suff, lhs_ops),
            K::OrOr => self.compile_short_circuit_or(&suff, lhs_ops),
            _ => {
                self.compile_infix_suffix(&suff)?;
                self.emit_binop(op)
            }
        }
    }

    fn compile_short_circuit_and(
        &mut self,
        rhs: &[SyntaxElement],
        lhs_ops: u32,
    ) -> Result<(), CompileError> {
        self.builder
            .emit_charge_ops(lhs_ops.saturating_add(1));
        self.builder.emit_opcode(Opcode::Dup);
        let jif_op = self.builder.emit_jump_if_false_placeholder();
        self.builder.emit_opcode(Opcode::Pop);
        self.compile_infix_suffix(rhs)?;
        let ro = java_ops::java_ops_infix_suffix(rhs);
        if ro > 0 {
            self.builder.emit_charge_ops(ro);
        }
        let merge_pc = self.builder.len();
        let after_jif = jif_op + 4;
        self.builder
            .patch_i32_operand_at(jif_op, merge_pc as i32 - after_jif as i32);
        Ok(())
    }

    fn compile_short_circuit_or(
        &mut self,
        rhs: &[SyntaxElement],
        lhs_ops: u32,
    ) -> Result<(), CompileError> {
        self.builder
            .emit_charge_ops(lhs_ops.saturating_add(1));
        self.builder.emit_opcode(Opcode::Dup);
        let jif_op = self.builder.emit_jump_if_false_placeholder();
        let jmp_op = self.builder.emit_jump_placeholder();
        let l_rhs = self.builder.len();
        self.builder.emit_opcode(Opcode::Pop);
        self.compile_infix_suffix(rhs)?;
        let ro = java_ops::java_ops_infix_suffix(rhs);
        if ro > 0 {
            self.builder.emit_charge_ops(ro);
        }
        let merge_pc = self.builder.len();
        let after_jif = jif_op + 4;
        self.builder
            .patch_i32_operand_at(jif_op, l_rhs as i32 - after_jif as i32);
        let after_jmp = jmp_op + 4;
        self.builder
            .patch_i32_operand_at(jmp_op, merge_pc as i32 - after_jmp as i32);
        Ok(())
    }

    /// Suffix of a flat infix chain: `[atom, BinaryExpr, …]` (same shape as a level root).
    fn compile_infix_suffix(&mut self, parts: &[SyntaxElement]) -> Result<(), CompileError> {
        if parts.is_empty() {
            return Err(CompileError::Unsupported("empty expression suffix"));
        }
        if parts.len() == 1 {
            return self.compile_suffix_atom(&parts[0]);
        }
        match &parts[0] {
            SyntaxElement::Token(t) => {
                if self.push_literal_token(t)? {
                    // ok
                } else if t.kind() == K::Ident.into_syntax_kind() {
                    let name = t.text().to_string();
                    let slot = *self
                        .locals
                        .get(&name)
                        .ok_or(CompileError::Unsupported("undefined variable"))?;
                    self.builder.emit_opcode(Opcode::GetLocal);
                    self.builder.emit_u16_operand(slot);
                } else {
                    return Err(CompileError::Unsupported(
                        "expression suffix starts with unsupported token",
                    ));
                }
            }
            SyntaxElement::Node(first) => {
                if BinaryExpr::can_cast(first.kind()) {
                    return Err(CompileError::Unsupported(
                        "expression suffix starts with BinaryExpr",
                    ));
                }
                self.compile_expr_from_syntax(first.clone())?;
            }
        }
        for p in parts.iter().skip(1) {
            let SyntaxElement::Node(node) = p else {
                return Err(CompileError::Unsupported("infix suffix tail must be BinaryExpr"));
            };
            if !BinaryExpr::can_cast(node.kind()) {
                return Err(CompileError::Unsupported("infix suffix tail must be BinaryExpr"));
            }
        }
        let mut prefix_len = 1usize;
        for p in parts.iter().skip(1) {
            let SyntaxElement::Node(bin) = p else {
                unreachable!("validated above");
            };
            let Some(be) = BinaryExpr::cast(bin.clone()) else {
                return Err(CompileError::Unsupported("infix suffix BinaryExpr"));
            };
            let lhs_ops = java_ops::java_ops_expr_flat(&parts[..prefix_len]);
            self.compile_binary_fragment(be.syntax(), lhs_ops)?;
            prefix_len += 1;
        }
        Ok(())
    }

    fn compile_suffix_atom(&mut self, el: &SyntaxElement) -> Result<(), CompileError> {
        match el {
            SyntaxElement::Token(t) => {
                if self.push_literal_token(t)? {
                    return Ok(());
                }
                if t.kind() == K::Ident.into_syntax_kind() {
                    let name = t.text().to_string();
                    let slot = *self
                        .locals
                        .get(&name)
                        .ok_or(CompileError::Unsupported("undefined variable"))?;
                    self.builder.emit_opcode(Opcode::GetLocal);
                    self.builder.emit_u16_operand(slot);
                    return Ok(());
                }
                Err(CompileError::Unsupported("unsupported atomic suffix"))
            }
            SyntaxElement::Node(n) => self.compile_expr_from_syntax(n.clone()),
        }
    }

    fn emit_binop(&mut self, op: K) -> Result<(), CompileError> {
        let opc = match op {
            K::Plus => Opcode::Add,
            K::Minus => Opcode::Sub,
            K::Star => Opcode::Mul,
            K::Slash => Opcode::Div,
            K::Percent => Opcode::Mod,
            K::EqEq | K::EqEqEq => Opcode::EqEquals,
            K::NotEq | K::NotEqEq => Opcode::NeEquals,
            K::Lt => Opcode::Lt,
            K::Lte => Opcode::Lte,
            K::Gt => Opcode::Gt,
            K::Gte => Opcode::Gte,
            _ => {
                return Err(CompileError::Unsupported(
                    "binary operator not supported by VM",
                ));
            }
        };
        self.builder.emit_opcode(opc);
        Ok(())
    }

    fn compile_unary(&mut self, u: &UnaryExpr) -> Result<(), CompileError> {
        let n = u.syntax();
        let minus = K::Minus.into_syntax_kind();
        let bang = K::Bang.into_syntax_kind();
        let not_kw = K::NotKw.into_syntax_kind();
        let semantic: Vec<_> = n
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();
        let mut i = 0usize;
        let mut has_minus = false;
        let mut has_not = false;
        while i < semantic.len() {
            let SyntaxElement::Token(t) = &semantic[i] else {
                break;
            };
            let k = t.kind();
            if k == minus {
                has_minus = true;
                i += 1;
                continue;
            }
            if k == bang || k == not_kw {
                has_not = true;
                i += 1;
                continue;
            }
            break;
        }
        let operand = &semantic[i..];
        match operand {
            [SyntaxElement::Node(inner)] => {
                self.compile_expr_from_syntax(inner.clone())?;
            }
            [SyntaxElement::Token(t)] => {
                if self.push_literal_token(t)? {
                    // ok
                } else if t.kind() == K::Ident.into_syntax_kind() {
                    let name = t.text().to_string();
                    let slot = *self
                        .locals
                        .get(&name)
                        .ok_or(CompileError::Unsupported("undefined variable"))?;
                    self.builder.emit_opcode(Opcode::GetLocal);
                    self.builder.emit_u16_operand(slot);
                } else {
                    return Err(CompileError::Unsupported("unary operand"));
                }
            }
            _ => return Err(CompileError::Unsupported("unary without operand")),
        }
        if has_not {
            self.builder.emit_opcode(Opcode::Not);
        }
        if has_minus {
            self.builder.emit_opcode(Opcode::Neg);
        }
        if !has_minus && !has_not {
            return Err(CompileError::Unsupported(
                "unary operator not supported by VM compiler",
            ));
        }
        Ok(())
    }
}

