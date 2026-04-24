//! Lower [`crate::ast::ParsedFile`] to a rowan green tree (same trivia layout as tokenization gaps).

use crate::ast::{
    ArrowFnBody, AssignStmt, Block, BreakStmt, CaseLabel, ClassDecl, ClassFieldDecl, ClassMember,
    ConstructorDecl, ContinueStmt,
    DoWhileStmt, ElseBranch, Expr, ForAssign, ForInBinding, ForInKeyValueStmt, ForInStmt, ForInit,
    ForStmt, ForUpdate, FunctionBody, FunctionDecl, GlobalDecl, IfStmt, IncludeStmt,
    ParsedFile, ReturnStmt, Stmt,
    StmtBody,
    SwitchClause, SwitchStmt, ThrowStmt, TryStmt, TypedVarDecl, VarDecl, VarDeclarator, VarDeclFor,
    WhileStmt,
};
use leekscript_lexer::{Token, TokenKind};
use leekscript_syntax::green::emit_token_with_trivia;
use leekscript_syntax::{LeekLanguage, LeekSyntaxKind};
use rowan::{GreenNodeBuilder, SyntaxNode};

pub fn emit_file(src: &str, tokens: &[Token], file: &ParsedFile) -> SyntaxNode<LeekLanguage> {
    let mut builder = GreenNodeBuilder::new();
    let mut last_end = 0usize;
    builder.start_node(rowan::SyntaxKind(LeekSyntaxKind::SourceFile as u16));
    for stmt in &file.stmts {
        emit_stmt(&mut builder, src, tokens, &mut last_end, stmt);
    }
    leekscript_syntax::green::push_trivia(&mut builder, &src[last_end..]);
    builder.finish_node();
    SyntaxNode::new_root(builder.finish())
}

fn emit_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    stmt: &Stmt,
) {
    match stmt {
        Stmt::Var(v) => emit_var_decl(b, src, tokens, last_end, v),
        Stmt::TypedVar(v) => emit_typed_var_decl(b, src, tokens, last_end, v),
        Stmt::Return(r) => emit_return(b, src, tokens, last_end, r),
        Stmt::Block(bl) => emit_block(b, src, tokens, last_end, bl),
        Stmt::Function(f) => emit_function_decl(b, src, tokens, last_end, f),
        Stmt::If(i) => emit_if_stmt(b, src, tokens, last_end, i),
        Stmt::While(w) => emit_while_stmt(b, src, tokens, last_end, w),
        Stmt::DoWhile(d) => emit_do_while_stmt(b, src, tokens, last_end, d),
        Stmt::Switch(sw) => emit_switch_stmt(b, src, tokens, last_end, sw),
        Stmt::For(f) => emit_for_stmt(b, src, tokens, last_end, f),
        Stmt::ForIn(f) => emit_for_in_stmt(b, src, tokens, last_end, f),
        Stmt::ForInKeyValue(f) => emit_for_in_key_value_stmt(b, src, tokens, last_end, f),
        Stmt::Empty { semi } => emit_empty_stmt(b, src, tokens, last_end, *semi),
        Stmt::Assign(a) => emit_assign_stmt(b, src, tokens, last_end, a),
        Stmt::Try(t) => emit_try_stmt(b, src, tokens, last_end, t),
        Stmt::Throw(t) => emit_throw_stmt(b, src, tokens, last_end, t),
        Stmt::Class(c) => emit_class_decl(b, src, tokens, last_end, c),
        Stmt::Break(x) => emit_break_stmt(b, src, tokens, last_end, x),
        Stmt::Continue(x) => emit_continue_stmt(b, src, tokens, last_end, x),
        Stmt::ExprSemi(e, semi) => emit_expr_stmt(b, src, tokens, last_end, e, semi.as_ref()),
        Stmt::Global(g) => emit_global_decl(b, src, tokens, last_end, g),
        Stmt::Include(i) => emit_include_stmt(b, src, tokens, last_end, i),
    }
}

fn emit_typed_var_decl(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    v: &TypedVarDecl,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::TypedVarDecl as u16));
    for i in &v.type_tokens {
        emit_token_with_trivia(b, src, last_end, &tokens[*i]);
    }
    emit_token_with_trivia(b, src, last_end, &tokens[v.name]);
    if let (Some(eq), Some(ref init)) = (v.eq, v.init.as_ref()) {
        emit_token_with_trivia(b, src, last_end, &tokens[eq]);
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
        emit_expr(b, src, tokens, last_end, init);
        b.finish_node();
    }
    if let Some(semi) = v.semi {
        emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    }
    b.finish_node();
}

fn emit_global_decl(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    g: &GlobalDecl,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::GlobalStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[g.global_kw]);
    if !g.leading_type_tokens.is_empty() {
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::GlobalLeadingType as u16));
        for i in &g.leading_type_tokens {
            emit_token_with_trivia(b, src, last_end, &tokens[*i]);
        }
        b.finish_node();
    }
    for (i, item) in g.items.iter().enumerate() {
        emit_token_with_trivia(b, src, last_end, &tokens[item.name]);
        if let (Some(eq), Some(ref init)) = (item.eq, item.init.as_ref()) {
            emit_token_with_trivia(b, src, last_end, &tokens[eq]);
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
            emit_expr(b, src, tokens, last_end, init);
            b.finish_node();
        }
        if i + 1 < g.items.len() {
            emit_token_with_trivia(b, src, last_end, &tokens[g.item_commas[i]]);
        }
    }
    if let Some(semi) = g.semi {
        emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    }
    b.finish_node();
}

fn emit_include_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    inc: &IncludeStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::IncludeStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[inc.include_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[inc.open_paren]);
    for i in &inc.inner_open_parens {
        emit_token_with_trivia(b, src, last_end, &tokens[*i]);
    }
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::LiteralExpr as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[inc.path]);
    b.finish_node();
    for i in &inc.close_parens {
        emit_token_with_trivia(b, src, last_end, &tokens[*i]);
    }
    if let Some(semi) = inc.semi {
        emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    }
    b.finish_node();
}

fn emit_assign_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    a: &AssignStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::AssignStmt as u16));
    emit_expr(b, src, tokens, last_end, &a.target);
    emit_token_with_trivia(b, src, last_end, &tokens[a.op]);
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
    emit_expr(b, src, tokens, last_end, &a.value);
    b.finish_node();
    if let Some(semi) = a.semi {
        emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    }
    b.finish_node();
}

fn emit_try_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    t: &TryStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::TryStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[t.try_kw]);
    emit_block(b, src, tokens, last_end, &t.try_body);
    if let Some(c) = &t.catch {
        emit_token_with_trivia(b, src, last_end, &tokens[c.catch_kw]);
        emit_token_with_trivia(b, src, last_end, &tokens[c.open]);
        emit_token_with_trivia(b, src, last_end, &tokens[c.param]);
        emit_token_with_trivia(b, src, last_end, &tokens[c.close]);
        emit_block(b, src, tokens, last_end, &c.body);
    }
    if let Some((finally_kw, fb)) = &t.finally_block {
        emit_token_with_trivia(b, src, last_end, &tokens[*finally_kw]);
        emit_block(b, src, tokens, last_end, fb);
    }
    b.finish_node();
}

fn emit_throw_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    t: &ThrowStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ThrowStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[t.throw_kw]);
    if let Some(ref e) = t.value {
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
        emit_expr(b, src, tokens, last_end, e);
        b.finish_node();
    }
    if let Some(semi) = t.semi {
        emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    }
    b.finish_node();
}

fn emit_class_decl(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    c: &ClassDecl,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ClassDecl as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[c.class_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[c.name]);
    if let Some((extends_kw, super_name)) = &c.extends {
        emit_token_with_trivia(b, src, last_end, &tokens[*extends_kw]);
        emit_token_with_trivia(b, src, last_end, &tokens[*super_name]);
    }
    emit_token_with_trivia(b, src, last_end, &tokens[c.open_brace]);
    for m in &c.members {
        match m {
            ClassMember::Method(f) => emit_function_decl(b, src, tokens, last_end, f),
            ClassMember::Constructor(k) => emit_constructor_decl(b, src, tokens, last_end, k),
            ClassMember::Field(f) => emit_class_field_decl(b, src, tokens, last_end, f),
        }
    }
    emit_token_with_trivia(b, src, last_end, &tokens[c.close_brace]);
    b.finish_node();
}

fn emit_constructor_decl(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    c: &ConstructorDecl,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ConstructorDecl as u16));
    for m in &c.member_modifiers {
        emit_token_with_trivia(b, src, last_end, &tokens[*m]);
    }
    emit_token_with_trivia(b, src, last_end, &tokens[c.constructor_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[c.open_paren]);
    for (i, p) in c.params.iter().enumerate() {
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::FnParam as u16));
        for tt in &c.param_type_tokens[i] {
            emit_token_with_trivia(b, src, last_end, &tokens[*tt]);
        }
        if let Some(at) = c.param_at.get(i).copied().flatten() {
            emit_token_with_trivia(b, src, last_end, &tokens[at]);
        }
        emit_token_with_trivia(b, src, last_end, &tokens[*p]);
        if let Some(d) = &c.param_defaults[i] {
            emit_token_with_trivia(b, src, last_end, &tokens[d.eq]);
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
            emit_expr(b, src, tokens, last_end, &d.value);
            b.finish_node();
        }
        b.finish_node();
        if i + 1 < c.params.len() {
            emit_token_with_trivia(b, src, last_end, &tokens[c.param_commas[i]]);
        }
    }
    emit_token_with_trivia(b, src, last_end, &tokens[c.close_paren]);
    emit_block(b, src, tokens, last_end, &c.body);
    b.finish_node();
}

fn emit_class_field_decl(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    f: &ClassFieldDecl,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ClassFieldDecl as u16));
    for m in &f.modifiers {
        emit_token_with_trivia(b, src, last_end, &tokens[*m]);
    }
    for i in &f.type_tokens {
        emit_token_with_trivia(b, src, last_end, &tokens[*i]);
    }
    emit_token_with_trivia(b, src, last_end, &tokens[f.name]);
    if let Some(init) = &f.init {
        emit_token_with_trivia(b, src, last_end, &tokens[init.eq]);
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
        emit_expr(b, src, tokens, last_end, &init.value);
        b.finish_node();
    }
    if let Some(semi) = f.semi {
        emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    }
    b.finish_node();
}

fn emit_break_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    x: &BreakStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::BreakStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[x.break_kw]);
    if let Some(semi) = x.semi {
        emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    }
    b.finish_node();
}

fn emit_continue_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    x: &ContinueStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ContinueStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[x.continue_kw]);
    if let Some(semi) = x.semi {
        emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    }
    b.finish_node();
}

fn emit_if_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    i: &IfStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::IfStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[i.if_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[i.open_paren]);
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
    emit_expr(b, src, tokens, last_end, &i.cond);
    b.finish_node();
    emit_token_with_trivia(b, src, last_end, &tokens[i.close_paren]);
    emit_stmt_body(b, src, tokens, last_end, &i.then_body);
    if let (Some(ek), Some(branch)) = (i.else_kw, i.else_branch.as_ref()) {
        emit_token_with_trivia(b, src, last_end, &tokens[ek]);
        match branch {
            ElseBranch::Body(bl) => emit_stmt_body(b, src, tokens, last_end, bl),
            ElseBranch::If(inner) => emit_if_stmt(b, src, tokens, last_end, inner),
        }
    }
    b.finish_node();
}

fn emit_stmt_body(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    body: &StmtBody,
) {
    match body {
        StmtBody::Block(bl) => emit_block(b, src, tokens, last_end, bl),
        StmtBody::Single(st) => emit_stmt(b, src, tokens, last_end, st),
    }
}

fn emit_while_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    w: &WhileStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::WhileStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[w.while_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[w.open_paren]);
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
    emit_expr(b, src, tokens, last_end, &w.cond);
    b.finish_node();
    emit_token_with_trivia(b, src, last_end, &tokens[w.close_paren]);
    match &w.body {
        StmtBody::Block(bl) => emit_block(b, src, tokens, last_end, bl),
        StmtBody::Single(s) => emit_stmt(b, src, tokens, last_end, s),
    }
    b.finish_node();
}

fn emit_do_while_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    d: &DoWhileStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::DoWhileStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[d.do_kw]);
    emit_block(b, src, tokens, last_end, &d.body);
    emit_token_with_trivia(b, src, last_end, &tokens[d.while_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[d.open_paren]);
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
    emit_expr(b, src, tokens, last_end, &d.cond);
    b.finish_node();
    emit_token_with_trivia(b, src, last_end, &tokens[d.close_paren]);
    // `do { ... } while (cond)` may omit the trailing `;` (parser stores `semi == close_paren`).
    if d.semi != d.close_paren {
        emit_token_with_trivia(b, src, last_end, &tokens[d.semi]);
    }
    b.finish_node();
}

fn emit_switch_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    sw: &SwitchStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::SwitchStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[sw.switch_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[sw.open_paren]);
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
    emit_expr(b, src, tokens, last_end, &sw.discr);
    b.finish_node();
    emit_token_with_trivia(b, src, last_end, &tokens[sw.close_paren]);
    emit_token_with_trivia(b, src, last_end, &tokens[sw.open_brace]);
    for cl in &sw.clauses {
        match cl {
            SwitchClause::Case { labels, body } => {
                emit_switch_case_clause(b, src, tokens, last_end, labels, body)
            }
            SwitchClause::Default {
                default_kw,
                colon,
                body,
            } => emit_switch_default_clause(b, src, tokens, last_end, *default_kw, *colon, body),
        }
    }
    emit_token_with_trivia(b, src, last_end, &tokens[sw.close_brace]);
    b.finish_node();
}

fn emit_switch_case_clause(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    labels: &[CaseLabel],
    body: &[Stmt],
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::SwitchCaseClause as u16));
    for lab in labels {
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::CaseLabel as u16));
        emit_token_with_trivia(b, src, last_end, &tokens[lab.case_kw]);
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
        emit_expr(b, src, tokens, last_end, &lab.value);
        b.finish_node();
        emit_token_with_trivia(b, src, last_end, &tokens[lab.colon]);
        b.finish_node();
    }
    for st in body {
        emit_stmt(b, src, tokens, last_end, st);
    }
    b.finish_node();
}

fn emit_switch_default_clause(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    default_kw: usize,
    colon: usize,
    body: &[Stmt],
) {
    b.start_node(rowan::SyntaxKind(
        LeekSyntaxKind::SwitchDefaultClause as u16,
    ));
    emit_token_with_trivia(b, src, last_end, &tokens[default_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[colon]);
    for st in body {
        emit_stmt(b, src, tokens, last_end, st);
    }
    b.finish_node();
}

fn emit_empty_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    semi: usize,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::EmptyStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    b.finish_node();
}

fn emit_for_in_binding(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    binding: &ForInBinding,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ForInBinding as u16));
    if let Some(tt) = &binding.type_tokens {
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ForInTypeAnn as u16));
        for i in tt {
            emit_token_with_trivia(b, src, last_end, &tokens[*i]);
        }
        b.finish_node();
    }
    if let Some(var_kw) = binding.var_kw {
        emit_token_with_trivia(b, src, last_end, &tokens[var_kw]);
    }
    if let Some(at_kw) = binding.at_kw {
        emit_token_with_trivia(b, src, last_end, &tokens[at_kw]);
    }
    emit_leaf(b, src, tokens, last_end, binding.name);
    b.finish_node();
}

fn emit_for_in_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    f: &ForInStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ForInStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[f.for_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[f.open_paren]);
    emit_for_in_binding(b, src, tokens, last_end, &f.binding);
    emit_token_with_trivia(b, src, last_end, &tokens[f.in_kw]);
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
    emit_expr(b, src, tokens, last_end, &f.container);
    b.finish_node();
    emit_token_with_trivia(b, src, last_end, &tokens[f.close_paren]);
    emit_stmt_body(b, src, tokens, last_end, &f.body);
    b.finish_node();
}

fn emit_for_in_key_value_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    f: &ForInKeyValueStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ForInKeyValueStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[f.for_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[f.open_paren]);
    emit_for_in_binding(b, src, tokens, last_end, &f.key);
    emit_token_with_trivia(b, src, last_end, &tokens[f.colon]);
    emit_for_in_binding(b, src, tokens, last_end, &f.value);
    emit_token_with_trivia(b, src, last_end, &tokens[f.in_kw]);
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
    emit_expr(b, src, tokens, last_end, &f.container);
    b.finish_node();
    emit_token_with_trivia(b, src, last_end, &tokens[f.close_paren]);
    emit_stmt_body(b, src, tokens, last_end, &f.body);
    b.finish_node();
}

fn emit_for_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    f: &ForStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ForStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[f.for_kw]);
    emit_token_with_trivia(b, src, last_end, &tokens[f.open_paren]);
    if let Some(init) = &f.init {
        match init {
            ForInit::Var(v) => emit_for_init_var(b, src, tokens, last_end, v),
            ForInit::Assign(a) => emit_for_assign_clause(b, src, tokens, last_end, a),
        }
    }
    emit_token_with_trivia(b, src, last_end, &tokens[f.first_semi]);
    if let Some(ref c) = f.cond {
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
        emit_expr(b, src, tokens, last_end, c);
        b.finish_node();
    }
    emit_token_with_trivia(b, src, last_end, &tokens[f.second_semi]);
    if let Some(ref u) = f.update {
        match u {
            ForUpdate::Assign(a) => emit_for_assign_clause(b, src, tokens, last_end, a),
            ForUpdate::Expr(e) => {
                b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
                emit_expr(b, src, tokens, last_end, e);
                b.finish_node();
            }
        }
    }
    emit_token_with_trivia(b, src, last_end, &tokens[f.close_paren]);
    emit_stmt_body(b, src, tokens, last_end, &f.body);
    b.finish_node();
}

fn emit_for_init_var(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    v: &VarDeclFor,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ForInitVar as u16));
    if let Some(ref tt) = v.type_tokens {
        for i in tt {
            emit_token_with_trivia(b, src, last_end, &tokens[*i]);
        }
    }
    if let Some(vk) = v.var_kw {
        emit_token_with_trivia(b, src, last_end, &tokens[vk]);
    }
    emit_token_with_trivia(b, src, last_end, &tokens[v.name]);
    emit_token_with_trivia(b, src, last_end, &tokens[v.eq]);
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
    emit_expr(b, src, tokens, last_end, &v.init);
    b.finish_node();
    b.finish_node();
}

fn emit_for_assign_clause(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    a: &ForAssign,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ForAssign as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[a.name]);
    emit_token_with_trivia(b, src, last_end, &tokens[a.op]);
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
    emit_expr(b, src, tokens, last_end, &a.value);
    b.finish_node();
    b.finish_node();
}

fn emit_function_decl(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    f: &FunctionDecl,
) {
       b.start_node(rowan::SyntaxKind(LeekSyntaxKind::FunctionDecl as u16));
    for m in &f.member_modifiers {
        emit_token_with_trivia(b, src, last_end, &tokens[*m]);
    }
    if let Some(fk) = f.function_kw {
        emit_token_with_trivia(b, src, last_end, &tokens[fk]);
    }
    for i in &f.return_type_tokens {
        emit_token_with_trivia(b, src, last_end, &tokens[*i]);
    }
    emit_token_with_trivia(b, src, last_end, &tokens[f.name]);
    emit_token_with_trivia(b, src, last_end, &tokens[f.open_paren]);
    for (i, &param) in f.params.iter().enumerate() {
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::FnParam as u16));
        for tt in &f.param_type_tokens[i] {
            emit_token_with_trivia(b, src, last_end, &tokens[*tt]);
        }
        if let Some(at) = f.param_at.get(i).copied().flatten() {
            emit_token_with_trivia(b, src, last_end, &tokens[at]);
        }
        emit_token_with_trivia(b, src, last_end, &tokens[param]);
        if let Some(d) = &f.param_defaults[i] {
            emit_token_with_trivia(b, src, last_end, &tokens[d.eq]);
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
            emit_expr(b, src, tokens, last_end, &d.value);
            b.finish_node();
        }
        b.finish_node();
        if i + 1 < f.params.len() {
            emit_token_with_trivia(b, src, last_end, &tokens[f.param_commas[i]]);
        }
    }
    emit_token_with_trivia(b, src, last_end, &tokens[f.close_paren]);
    if let Some((arrow, rtoks)) = &f.arrow_return {
        emit_token_with_trivia(b, src, last_end, &tokens[*arrow]);
        for i in rtoks {
            emit_token_with_trivia(b, src, last_end, &tokens[*i]);
        }
    }
    match &f.body {
        FunctionBody::Block(block) => emit_block(b, src, tokens, last_end, block),
        FunctionBody::SignatureStub { semi } => {
            emit_token_with_trivia(b, src, last_end, &tokens[*semi]);
        }
    }
    b.finish_node();
}

fn emit_var_decl(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    v: &VarDecl,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::VarDecl as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[v.var_kw]);
    for (i, d) in v.decls.iter().enumerate() {
        if i > 0 {
            emit_token_with_trivia(b, src, last_end, &tokens[v.commas[i - 1]]);
        }
        emit_var_declarator(b, src, tokens, last_end, d);
    }
    if let Some(semi) = v.semi {
        emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    }
    b.finish_node();
}

fn emit_var_declarator(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    d: &VarDeclarator,
) {
    emit_token_with_trivia(b, src, last_end, &tokens[d.name]);
    if let (Some(eq), Some(ref init)) = (d.eq, d.init.as_ref()) {
        emit_token_with_trivia(b, src, last_end, &tokens[eq]);
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
        emit_expr(b, src, tokens, last_end, init);
        b.finish_node();
    }
}

fn emit_return(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    r: &ReturnStmt,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ReturnStmt as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[r.return_kw]);
    if let Some(q) = r.optional_question {
        emit_token_with_trivia(b, src, last_end, &tokens[q]);
    }
    if let Some(at) = r.at_kw {
        emit_token_with_trivia(b, src, last_end, &tokens[at]);
    }
    if let Some(ref e) = r.value {
        b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
        emit_expr(b, src, tokens, last_end, e);
        b.finish_node();
    }
    if let Some(semi) = r.semi {
        emit_token_with_trivia(b, src, last_end, &tokens[semi]);
    }
    b.finish_node();
}

fn emit_block(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    bl: &Block,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Block as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[bl.open]);
    for s in &bl.stmts {
        emit_stmt(b, src, tokens, last_end, s);
    }
    emit_token_with_trivia(b, src, last_end, &tokens[bl.close]);
    b.finish_node();
}

fn emit_expr_stmt(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    e: &Expr,
    semi: Option<&usize>,
) {
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ExprStmt as u16));
    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
    emit_expr(b, src, tokens, last_end, e);
    b.finish_node();
    if let Some(semi) = semi {
        emit_token_with_trivia(b, src, last_end, &tokens[*semi]);
    }
    b.finish_node();
}

fn emit_expr(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    e: &Expr,
) {
    match e {
        Expr::ClassSelf { class_kw } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::LiteralExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[*class_kw]);
            b.finish_node();
        }
        Expr::Leaf(i) => emit_leaf(b, src, tokens, last_end, *i),
        Expr::Unary { op, expr } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::UnaryExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[*op]);
            emit_expr(b, src, tokens, last_end, expr);
            b.finish_node();
        }
        Expr::Ref { at_kw, expr } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::UnaryExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[*at_kw]);
            emit_expr(b, src, tokens, last_end, expr);
            b.finish_node();
        }
        Expr::Binary(left, op, right) => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::BinaryExpr as u16));
            emit_expr(b, src, tokens, last_end, left);
            emit_token_with_trivia(b, src, last_end, &tokens[*op]);
            emit_expr(b, src, tokens, last_end, right);
            b.finish_node();
        }
        Expr::Paren { open, expr, close } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ParenExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[*open]);
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
            emit_expr(b, src, tokens, last_end, expr);
            b.finish_node();
            emit_token_with_trivia(b, src, last_end, &tokens[*close]);
            b.finish_node();
        }
        Expr::Call {
            callee,
            open,
            args,
            arg_commas,
            close,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::CallExpr as u16));
            emit_expr(b, src, tokens, last_end, callee);
            emit_token_with_trivia(b, src, last_end, &tokens[*open]);
            for (i, arg) in args.iter().enumerate() {
                emit_expr(b, src, tokens, last_end, arg);
                if i + 1 < args.len() {
                    if let Some(ci) = arg_commas[i] {
                        emit_token_with_trivia(b, src, last_end, &tokens[ci]);
                    }
                }
            }
            emit_token_with_trivia(b, src, last_end, &tokens[*close]);
            b.finish_node();
        }
        Expr::ArrayLiteral {
            open,
            elements,
            commas,
            close,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ArrayLiteralExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[*open]);
            for (i, el) in elements.iter().enumerate() {
                emit_expr(b, src, tokens, last_end, el);
                if i + 1 < elements.len() && i < commas.len() {
                    emit_token_with_trivia(b, src, last_end, &tokens[commas[i]]);
                }
            }
            emit_token_with_trivia(b, src, last_end, &tokens[*close]);
            b.finish_node();
        }
        Expr::MapLiteral {
            open,
            entries,
            commas,
            close,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::MapLiteralExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[*open]);
            for (i, e) in entries.iter().enumerate() {
                emit_expr(b, src, tokens, last_end, &e.key);
                emit_token_with_trivia(b, src, last_end, &tokens[e.colon]);
                emit_expr(b, src, tokens, last_end, &e.value);
                if i + 1 < entries.len() {
                    emit_token_with_trivia(b, src, last_end, &tokens[commas[i]]);
                }
            }
            emit_token_with_trivia(b, src, last_end, &tokens[*close]);
            b.finish_node();
        }
        Expr::IntervalLiteral {
            open,
            min,
            dotdot,
            max,
            close,
        } => {
            b.start_node(rowan::SyntaxKind(
                LeekSyntaxKind::IntervalLiteralExpr as u16,
            ));
            emit_token_with_trivia(b, src, last_end, &tokens[*open]);
            if let Some(ref m) = min {
                emit_expr(b, src, tokens, last_end, m);
            }
            emit_token_with_trivia(b, src, last_end, &tokens[*dotdot]);
            if let Some(ref m) = max {
                emit_expr(b, src, tokens, last_end, m);
            }
            emit_token_with_trivia(b, src, last_end, &tokens[*close]);
            b.finish_node();
        }
        Expr::SetLiteral {
            open,
            elements,
            commas,
            close,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::SetLiteralExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[*open]);
            for (i, el) in elements.iter().enumerate() {
                emit_expr(b, src, tokens, last_end, el);
                if i + 1 < elements.len() && i < commas.len() {
                    emit_token_with_trivia(b, src, last_end, &tokens[commas[i]]);
                }
            }
            if let Some(c) = close {
                emit_token_with_trivia(b, src, last_end, &tokens[*c]);
            }
            b.finish_node();
        }
        Expr::ObjectLiteral {
            open,
            properties,
            commas,
            close,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ObjectLiteralExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[*open]);
            for (i, p) in properties.iter().enumerate() {
                emit_leaf(b, src, tokens, last_end, p.key_tok);
                emit_token_with_trivia(b, src, last_end, &tokens[p.colon]);
                emit_expr(b, src, tokens, last_end, &p.value);
                if i < commas.len() {
                    emit_token_with_trivia(b, src, last_end, &tokens[commas[i]]);
                }
            }
            emit_token_with_trivia(b, src, last_end, &tokens[*close]);
            b.finish_node();
        }
        Expr::New {
            new_kw,
            type_name,
            open,
            args,
            arg_commas,
            close,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::NewExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[*new_kw]);
            emit_leaf(b, src, tokens, last_end, *type_name);
            if let (Some(open), Some(close)) = (open, close) {
                emit_token_with_trivia(b, src, last_end, &tokens[*open]);
                for (i, arg) in args.iter().enumerate() {
                    emit_expr(b, src, tokens, last_end, arg);
                    if i + 1 < args.len() {
                        if let Some(ci) = arg_commas[i] {
                            emit_token_with_trivia(b, src, last_end, &tokens[ci]);
                        }
                    }
                }
                emit_token_with_trivia(b, src, last_end, &tokens[*close]);
            }
            b.finish_node();
        }
        Expr::Index {
            base,
            open,
            index,
            close,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::IndexExpr as u16));
            emit_expr(b, src, tokens, last_end, base);
            emit_token_with_trivia(b, src, last_end, &tokens[*open]);
            emit_expr(b, src, tokens, last_end, index.as_ref());
            emit_token_with_trivia(b, src, last_end, &tokens[*close]);
            b.finish_node();
        }
        Expr::ArraySlice {
            base,
            open,
            start,
            colon,
            end,
            colon_step,
            step,
            close,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ArraySliceExpr as u16));
            emit_expr(b, src, tokens, last_end, base);
            emit_token_with_trivia(b, src, last_end, &tokens[*open]);
            if let Some(ref s) = start {
                emit_expr(b, src, tokens, last_end, s.as_ref());
            }
            emit_token_with_trivia(b, src, last_end, &tokens[*colon]);
            if let Some(ref e) = end {
                emit_expr(b, src, tokens, last_end, e.as_ref());
            }
            if let Some(cs) = colon_step {
                emit_token_with_trivia(b, src, last_end, &tokens[*cs]);
                if let Some(ref st) = step {
                    emit_expr(b, src, tokens, last_end, st.as_ref());
                }
            }
            emit_token_with_trivia(b, src, last_end, &tokens[*close]);
            b.finish_node();
        }
        Expr::Member { base, dot, field } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::MemberExpr as u16));
            emit_expr(b, src, tokens, last_end, base);
            emit_token_with_trivia(b, src, last_end, &tokens[*dot]);
            emit_leaf(b, src, tokens, last_end, *field);
            b.finish_node();
        }
        Expr::Ternary {
            cond,
            question,
            then_expr,
            colon,
            else_expr,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::TernaryExpr as u16));
            emit_expr(b, src, tokens, last_end, cond);
            emit_token_with_trivia(b, src, last_end, &tokens[*question]);
            emit_expr(b, src, tokens, last_end, then_expr);
            emit_token_with_trivia(b, src, last_end, &tokens[*colon]);
            emit_expr(b, src, tokens, last_end, else_expr);
            b.finish_node();
        }
        Expr::NotIn {
            elem,
            not_kw,
            in_kw,
            container,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::NotInExpr as u16));
            emit_expr(b, src, tokens, last_end, elem);
            emit_token_with_trivia(b, src, last_end, &tokens[*not_kw]);
            emit_token_with_trivia(b, src, last_end, &tokens[*in_kw]);
            emit_expr(b, src, tokens, last_end, container);
            b.finish_node();
        }
        Expr::AsCast {
            expr,
            as_kw,
            ty: _,
            type_tokens,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::AsCastExpr as u16));
            emit_expr(b, src, tokens, last_end, expr);
            emit_token_with_trivia(b, src, last_end, &tokens[*as_kw]);
            for i in type_tokens {
                emit_token_with_trivia(b, src, last_end, &tokens[*i]);
            }
            b.finish_node();
        }
        Expr::PrefixCast { ty, expr } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::PrefixCastExpr as u16));
            emit_leaf(b, src, tokens, last_end, *ty);
            emit_expr(b, src, tokens, last_end, expr);
            b.finish_node();
        }
        Expr::FunctionValue(fv) => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::FunctionValueExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[fv.function_kw]);
            emit_token_with_trivia(b, src, last_end, &tokens[fv.open_paren]);
            for (i, &param) in fv.params.iter().enumerate() {
                b.start_node(rowan::SyntaxKind(LeekSyntaxKind::FnParam as u16));
                for tt in &fv.param_type_tokens[i] {
                    emit_token_with_trivia(b, src, last_end, &tokens[*tt]);
                }
                if let Some(at) = fv.param_at.get(i).copied().flatten() {
                    emit_token_with_trivia(b, src, last_end, &tokens[at]);
                }
                emit_token_with_trivia(b, src, last_end, &tokens[param]);
                if let Some(d) = &fv.param_defaults[i] {
                    emit_token_with_trivia(b, src, last_end, &tokens[d.eq]);
                    b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
                    emit_expr(b, src, tokens, last_end, &d.value);
                    b.finish_node();
                }
                b.finish_node();
                if i + 1 < fv.params.len() {
                    emit_token_with_trivia(b, src, last_end, &tokens[fv.param_commas[i]]);
                }
            }
            emit_token_with_trivia(b, src, last_end, &tokens[fv.close_paren]);
            if let Some(arrow) = fv.arrow {
                emit_token_with_trivia(b, src, last_end, &tokens[arrow]);
                for i in &fv.return_type_tokens {
                    emit_token_with_trivia(b, src, last_end, &tokens[*i]);
                }
            }
            emit_block(b, src, tokens, last_end, &fv.body);
            b.finish_node();
        }
        Expr::ArrowFn {
            open_paren,
            params,
            param_commas,
            close_paren,
            arrow,
            body,
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::ArrowFnExpr as u16));
            if let Some(o) = open_paren {
                emit_token_with_trivia(b, src, last_end, &tokens[*o]);
            }
            for (i, p) in params.iter().enumerate() {
                emit_leaf(b, src, tokens, last_end, *p);
                if i + 1 < params.len() {
                    emit_token_with_trivia(b, src, last_end, &tokens[param_commas[i]]);
                }
            }
            if let Some(c) = close_paren {
                emit_token_with_trivia(b, src, last_end, &tokens[*c]);
            }
            emit_token_with_trivia(b, src, last_end, &tokens[*arrow]);
            match body {
                ArrowFnBody::Expr(e) => emit_expr(b, src, tokens, last_end, e),
                ArrowFnBody::Block(bl) => emit_block(b, src, tokens, last_end, bl),
            }
            b.finish_node();
        }
        Expr::PreUpdate {
            expr,
            op1,
            op2,
            ..
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::PreUpdateExpr as u16));
            emit_token_with_trivia(b, src, last_end, &tokens[*op1]);
            if op2 != op1 {
                emit_token_with_trivia(b, src, last_end, &tokens[*op2]);
            }
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
            emit_expr(b, src, tokens, last_end, expr);
            b.finish_node();
            b.finish_node();
        }
        Expr::PostUpdate {
            expr,
            op1,
            op2,
            ..
        } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::PostUpdateExpr as u16));
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
            emit_expr(b, src, tokens, last_end, expr);
            b.finish_node();
            emit_token_with_trivia(b, src, last_end, &tokens[*op1]);
            if op2 != op1 {
                emit_token_with_trivia(b, src, last_end, &tokens[*op2]);
            }
            b.finish_node();
        }
        Expr::AssignExpr { target, op, value } => {
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::AssignExpr as u16));
            emit_expr(b, src, tokens, last_end, target);
            emit_token_with_trivia(b, src, last_end, &tokens[*op]);
            b.start_node(rowan::SyntaxKind(LeekSyntaxKind::Expr as u16));
            emit_expr(b, src, tokens, last_end, value);
            b.finish_node();
            b.finish_node();
        }
    }
}

fn emit_leaf(
    b: &mut GreenNodeBuilder<'_>,
    src: &str,
    tokens: &[Token],
    last_end: &mut usize,
    i: usize,
) {
    let kind = match tokens[i].kind {
        TokenKind::Ident => LeekSyntaxKind::IdentExpr,
        _ => LeekSyntaxKind::LiteralExpr,
    };
    b.start_node(rowan::SyntaxKind(kind as u16));
    emit_token_with_trivia(b, src, last_end, &tokens[i]);
    b.finish_node();
}
