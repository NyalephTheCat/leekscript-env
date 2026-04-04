//! Compile a tiny LeekScript subset from the CST into [`Bytecode`](super::bytecode::Bytecode).
//!
//! Covers numeric expressions, `null` / `true` / `false`, string literals, array / map literals
//! (`[]`, `[:]`, `[k: v]`), Java-style `+` (string / array / map merge, `real` sum; `AI.add`-style
//! operation charges for concat), V4-style `==` / `!=` / `===` / `!==`,
//! ordered comparisons (`AI.real` subset), `!`, short-circuit `&&` / `||` / `and` / `or`, `if`,
//! `var` with comma-separated declarators, `return`, and expression statements.
//!
use std::collections::HashMap;
use std::fmt;

use sipha::tree::ast::{AstNode, AstNodeExt, AstToken};
use sipha::tree::red::{SyntaxElement, SyntaxNode, SyntaxToken};
use sipha::types::{FromSyntaxKind, IntoSyntaxKind};

use crate::ast::{
    ArrayExpr, BinaryExpr, BracketMapExpr, Expr, IfStmt, IntervalExpr, LitStr, ParenExpr, Root,
    Stmt, StmtBlock, UnaryExpr, VarDecl,
};
use crate::syntax::kinds::K;
use crate::syntax::syntax_el_is_trivia;
use crate::{ParseError, Version, parse_doc};

use super::bytecode::{Bytecode, BytecodeBuilder};
use super::opcode::Opcode;
use super::value::Value;

/// Parse + bytecode + metadata needed to run on [`super::Vm`](super::Vm).
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledChunk {
    pub bytecode: Bytecode,
    /// Pass to [`super::Vm::set_local_count`](super::Vm::set_local_count) (returns [`super::VmError`](super::error::VmError) on RAM limit) before [`super::Vm::run`](super::Vm::run).
    pub local_slots: usize,
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

/// Parse `source` as V4 and compile all top-level statements into one bytecode chunk.
pub fn compile_chunk_v4(source: &str) -> Result<CompiledChunk, CompileError> {
    let doc = parse_doc(source, Version::V4)?;
    let root = Root::cast(doc.root().clone()).ok_or(CompileError::Unsupported(
        "parse tree root is not K::Root",
    ))?;
    let mut cx = CompileCtx::default();
    for stmt in AstNodeExt::children::<Stmt>(root.syntax()) {
        cx.compile_stmt(stmt)?;
    }
    Ok(CompiledChunk {
        bytecode: cx.builder.finish(),
        local_slots: usize::from(cx.next_local),
    })
}

#[derive(Default)]
struct CompileCtx {
    builder: BytecodeBuilder,
    locals: HashMap<String, u16>,
    next_local: u16,
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
                    self.compile_expr(e)?;
                } else {
                    self.builder.emit_opcode(Opcode::PushNull);
                }
                self.builder.emit_return();
            }
            Stmt::Expr(es) => {
                if let Some(e) = es.expr() {
                    self.compile_expr(e)?;
                    self.builder.emit_opcode(Opcode::Pop);
                }
            }
            Stmt::VarDecl(v) => {
                self.compile_var_decl(v)?;
            }
            Stmt::If(i) => {
                self.compile_if_stmt(i)?;
            }
            _ => {
                return Err(CompileError::Unsupported(
                    "statement kind not supported by VM compiler",
                ));
            }
        }
        Ok(())
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
        self.compile_expr(cond)?;
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
        while i < elts.len() {
            if let SyntaxElement::Token(t) = &elts[i] {
                if matches!(K::from_syntax_kind(t.kind()), Some(K::Semi)) {
                    break;
                }
            }
            let SyntaxElement::Token(name_t) = &elts[i] else {
                return Err(CompileError::Unsupported(
                    "typed or complex var decl not supported by VM compiler",
                ));
            };
            if name_t.kind() != K::Ident.into_syntax_kind() {
                return Err(CompileError::Unsupported(
                    "var decl: expected identifier",
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
                                "var decl missing initializer expression",
                            ));
                        };
                        let expr = Expr::cast(n.clone()).ok_or(CompileError::Unsupported(
                            "var decl malformed initializer",
                        ))?;
                        self.compile_expr(expr)?;
                        i += 1;
                        initialized = true;
                    }
                }
            }
            if !initialized {
                self.builder.emit_opcode(Opcode::PushNull);
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
            if parts.len() >= 2 && self.try_compile_infix_chain_on_parts(&parts)? {
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
            return self.compile_binary_fragment(&n);
        }
        if let Some(arr) = ArrayExpr::cast(n.clone()) {
            return self.compile_array_literal(&arr);
        }
        if let Some(u) = UnaryExpr::cast(n.clone()) {
            return self.compile_unary(&u);
        }
        if let Some(p) = ParenExpr::cast(n.clone()) {
            let inner = paren_expr_inner_elements(p.syntax())?;
            let flat = flatten_one_expr_layer(&inner);
            if flat.len() >= 2 && self.try_compile_infix_chain_on_parts(&flat)? {
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
                    return Ok(false);
                }
            }
            SyntaxElement::Node(first) => {
                if BinaryExpr::can_cast(first.kind()) {
                    return Ok(false);
                }
                self.compile_expr_from_syntax(first.clone())?;
            }
        }
        for p in parts.iter().skip(1) {
            let SyntaxElement::Node(node) = p else {
                return Ok(false);
            };
            if !BinaryExpr::can_cast(node.kind()) {
                return Ok(false);
            }
        }
        for p in parts.iter().skip(1) {
            let SyntaxElement::Node(bin) = p else {
                unreachable!("validated above");
            };
            let Some(be) = BinaryExpr::cast(bin.clone()) else {
                return Err(CompileError::Unsupported("infix chain BinaryExpr"));
            };
            self.compile_binary_fragment(be.syntax())?;
        }
        Ok(true)
    }

    /// One [`BinaryExpr`](BinaryExpr): stack already holds its left operand; emit the suffix after
    /// the operator token, then the opcode.
    fn compile_binary_fragment(&mut self, bin: &SyntaxNode) -> Result<(), CompileError> {
        let op = first_binary_op_token(bin).ok_or(CompileError::Unsupported(
            "binary expression missing operator",
        ))?;
        let suff = suffix_after_first_binary_op(bin);
        match op {
            K::AndAnd => self.compile_short_circuit_and(&suff),
            K::OrOr => self.compile_short_circuit_or(&suff),
            _ => {
                self.compile_infix_suffix(&suff)?;
                self.emit_binop(op)
            }
        }
    }

    fn compile_short_circuit_and(&mut self, rhs: &[SyntaxElement]) -> Result<(), CompileError> {
        self.builder.emit_opcode(Opcode::Dup);
        let jif_op = self.builder.emit_jump_if_false_placeholder();
        self.builder.emit_opcode(Opcode::Pop);
        self.compile_infix_suffix(rhs)?;
        let merge_pc = self.builder.len();
        let after_jif = jif_op + 4;
        self.builder
            .patch_i32_operand_at(jif_op, merge_pc as i32 - after_jif as i32);
        Ok(())
    }

    fn compile_short_circuit_or(&mut self, rhs: &[SyntaxElement]) -> Result<(), CompileError> {
        self.builder.emit_opcode(Opcode::Dup);
        let jif_op = self.builder.emit_jump_if_false_placeholder();
        let jmp_op = self.builder.emit_jump_placeholder();
        let l_rhs = self.builder.len();
        self.builder.emit_opcode(Opcode::Pop);
        self.compile_infix_suffix(rhs)?;
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
        for p in parts.iter().skip(1) {
            let SyntaxElement::Node(bin) = p else {
                unreachable!("validated above");
            };
            let Some(be) = BinaryExpr::cast(bin.clone()) else {
                return Err(CompileError::Unsupported("infix suffix BinaryExpr"));
            };
            self.compile_binary_fragment(be.syntax())?;
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

fn binary_op_kind(k: K) -> bool {
    matches!(
        k,
        K::Plus
            | K::Minus
            | K::Star
            | K::Slash
            | K::Percent
            | K::EqEq
            | K::NotEq
            | K::EqEqEq
            | K::NotEqEq
            | K::Lt
            | K::Lte
            | K::Gt
            | K::Gte
            | K::AndAnd
            | K::OrOr
    )
}

fn first_binary_op_token(bin: &SyntaxNode) -> Option<K> {
    for el in bin.children() {
        if syntax_el_is_trivia(&el) {
            continue;
        }
        if let SyntaxElement::Token(t) = &el {
            if let Some(k) = K::from_syntax_kind(t.kind()) {
                if binary_op_kind(k) {
                    return Some(k);
                }
            }
        }
    }
    None
}

/// Non-trivia [`SyntaxElement`](SyntaxElement)s after the first binary operator token under `bin`.
fn suffix_after_first_binary_op(bin: &SyntaxNode) -> Vec<SyntaxElement> {
    let mut after_op = false;
    let mut out = Vec::new();
    for el in bin.children() {
        if syntax_el_is_trivia(&el) {
            continue;
        }
        if !after_op {
            if let SyntaxElement::Token(t) = &el {
                if let Some(k) = K::from_syntax_kind(t.kind()) {
                    if binary_op_kind(k) {
                        after_op = true;
                    }
                }
            }
            continue;
        }
        out.push(el.clone());
    }
    out
}
