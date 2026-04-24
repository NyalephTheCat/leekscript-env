//! Lower syntax nodes into [`HirStmt`](crate::nodes::HirStmt).

use super::expr::{is_expr_shape, lower_expr, lower_expr_wrapped, lower_ident_expr};
use super::util::{diag, non_trivia, span_of_node, span_of_range, token_text, LowerCtx};
use super::HirLoweringDiagnostic;
use crate::nodes::{
    HirAssignOp, HirClassMember, HirExpr, HirForStep, HirForUpdate, HirParam, HirStmt,
    HirSwitchClause, NameDef,
};

pub(super) fn hir_assign_op_from_token(op: &str) -> Option<HirAssignOp> {
    Some(match op {
        "=" => HirAssignOp::Assign,
        "+=" => HirAssignOp::AddAssign,
        "-=" => HirAssignOp::SubAssign,
        "*=" => HirAssignOp::MulAssign,
        "/=" => HirAssignOp::DivAssign,
        "%=" => HirAssignOp::RemAssign,
        "**=" => HirAssignOp::PowAssign,
        "\\=" => HirAssignOp::IntDivAssign,
        "^=" => HirAssignOp::BitXorAssign,
        "&=" => HirAssignOp::BitAndAssign,
        "|=" => HirAssignOp::BitOrAssign,
        "<<=" => HirAssignOp::ShlAssign,
        ">>=" => HirAssignOp::ShrAssign,
        ">>>=" => HirAssignOp::UShrAssign,
        _ => return None,
    })
}
use leekscript_syntax::{LeekLanguage, LeekSyntaxKind};
use rowan::{NodeOrToken, SyntaxElement, SyntaxNode, SyntaxToken, TextRange};

fn decl_ty_src_before_name(
    parts: &[SyntaxElement<LeekLanguage>],
    name_part_idx: usize,
    ctx: &LowerCtx,
) -> Option<String> {
    if name_part_idx == 0 {
        return None;
    }
    let range = TextRange::new(parts[0].text_range().start(), parts[name_part_idx - 1].text_range().end());
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    let raw = ctx.src.get(start..end)?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(
        raw.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase(),
    )
}

fn type_src_between_parts(
    parts: &[SyntaxElement<LeekLanguage>],
    start_idx: usize,
    end_idx_inclusive: usize,
    ctx: &LowerCtx,
) -> Option<String> {
    if start_idx > end_idx_inclusive || end_idx_inclusive >= parts.len() {
        return None;
    }
    let range = TextRange::new(
        parts[start_idx].text_range().start(),
        parts[end_idx_inclusive].text_range().end(),
    );
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    let raw = ctx.src.get(start..end)?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(
        raw.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase(),
    )
}

fn lower_for_loop_body(
    el: NodeOrToken<SyntaxNode<LeekLanguage>, SyntaxToken<LeekLanguage>>,
    ctx: &LowerCtx,
) -> Result<Vec<HirStmt>, HirLoweringDiagnostic> {
    match el {
        NodeOrToken::Node(node) if node.kind() == LeekSyntaxKind::Block => {
            lower_block_stmts(&node, ctx)
        }
        NodeOrToken::Node(node) => lower_stmt(&node, ctx),
        NodeOrToken::Token(t) => Err(diag(
            "UNEXPECTED_TOKEN",
            span_of_range(t.text_range()),
            "expected `{` or statement after `for` `(` … `)`",
        )),
    }
}

pub(super) fn lower_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<Vec<HirStmt>, HirLoweringDiagnostic> {
    match n.kind() {
        LeekSyntaxKind::VarDecl => lower_var_decl(n, ctx),
        LeekSyntaxKind::TypedVarDecl => Ok(vec![lower_typed_var_decl(n, ctx)?]),
        LeekSyntaxKind::ExprStmt => Ok(vec![lower_expr_stmt(n, ctx)?]),
        LeekSyntaxKind::ReturnStmt => Ok(vec![lower_return_stmt(n, ctx)?]),
        LeekSyntaxKind::Block => Ok(vec![lower_block_stmt(n, ctx)?]),
        LeekSyntaxKind::FunctionDecl => Ok(vec![lower_function_decl(n, ctx)?]),
        LeekSyntaxKind::IfStmt => Ok(vec![lower_if_stmt(n, ctx)?]),
        LeekSyntaxKind::WhileStmt => Ok(vec![lower_while_stmt(n, ctx)?]),
        LeekSyntaxKind::DoWhileStmt => Ok(vec![lower_do_while_stmt(n, ctx)?]),
        LeekSyntaxKind::SwitchStmt => Ok(vec![lower_switch_stmt(n, ctx)?]),
        LeekSyntaxKind::ForStmt => Ok(vec![lower_for_stmt(n, ctx)?]),
        LeekSyntaxKind::ForInStmt => Ok(vec![lower_for_in_stmt(n, ctx)?]),
        LeekSyntaxKind::ForInKeyValueStmt => Ok(vec![lower_for_in_key_value_stmt(n, ctx)?]),
        LeekSyntaxKind::EmptyStmt => Ok(vec![lower_empty_stmt(n, ctx)?]),
        LeekSyntaxKind::AssignStmt => Ok(vec![lower_assign_stmt(n, ctx)?]),
        LeekSyntaxKind::TryStmt => Ok(vec![lower_try_stmt(n, ctx)?]),
        LeekSyntaxKind::ThrowStmt => Ok(vec![lower_throw_stmt(n, ctx)?]),
        LeekSyntaxKind::ClassDecl => Ok(vec![lower_class_decl(n, ctx)?]),
        LeekSyntaxKind::BreakStmt => Ok(vec![lower_break_stmt(n, ctx)?]),
        LeekSyntaxKind::ContinueStmt => Ok(vec![lower_continue_stmt(n, ctx)?]),
        LeekSyntaxKind::GlobalStmt => Ok(vec![lower_global_stmt(n, ctx)?]),
        LeekSyntaxKind::IncludeStmt => Ok(vec![lower_include_stmt(n, ctx)?]),
        _ => Err(diag(
            "UNCOMPLETE_EXPRESSION",
            span_of_node(n),
            format!("unexpected statement node {:?}", n.kind()),
        )),
    }
}

pub(super) fn lower_var_decl(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<Vec<HirStmt>, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 2 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed VarDecl: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "var" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `var`",
            ));
        }
    }
    let mut out = Vec::new();
    let mut i = 1usize;
    while i < parts.len() {
        let name_tok = match &parts[i] {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident => t,
            _ => {
                return Err(diag(
                    "VAR_NAME_EXPECTED",
                    span_of_node(n),
                    "expected identifier in var declaration",
                ));
            }
        };
        let name = NameDef {
            name: token_text(name_tok, ctx.src).to_string(),
            span: span_of_range(name_tok.text_range()),
        };
        i += 1;
        let init = if i < parts.len()
            && matches!(
                &parts[i],
                NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == "="
            )
        {
            i += 1;
            let init_node = match parts.get(i) {
                Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::Expr => x,
                _ => {
                    return Err(diag(
                        "VALUE_EXPECTED",
                        span_of_node(n),
                        "expected initializer expression",
                    ));
                }
            };
            let init = lower_expr_wrapped(init_node, ctx)?;
            i += 1;
            Some(init)
        } else {
            None
        };
        out.push(HirStmt::Var {
            name,
            init,
            decl_ty: None,
        });
        if i < parts.len()
            && matches!(&parts[i], NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Comma)
        {
            i += 1;
            continue;
        }
        if i < parts.len()
            && matches!(&parts[i], NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Semicolon)
        {
            break;
        }
        if i < parts.len() {
            return Err(diag(
                "END_OF_INSTRUCTION_EXPECTED",
                span_of_node(n),
                "expected `,` or end of var declaration",
            ));
        }
        break;
    }
    Ok(out)
}

pub(super) fn lower_typed_var_decl(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    // Grammar: `TypeTokens...` Ident [`=` Expr] [`;`]
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 2 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed TypedVarDecl: {} elements", parts.len()),
        ));
    }
    let eq_idx = parts.iter().position(|p| {
        matches!(p, NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == "=")
    });
    let (name_tok, init, name_part_idx) = if let Some(eq_i) = eq_idx {
        if eq_i < 2 {
            return Err(diag(
                "INTERNAL_ERROR",
                span_of_node(n),
                "typed var decl too short before `=`",
            ));
        }
        let name_tok = match &parts[eq_i - 1] {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident => t,
            _ => {
                return Err(diag(
                    "VAR_NAME_EXPECTED",
                    span_of_node(n),
                    "expected identifier name in typed declaration",
                ));
            }
        };
        let init_node = match parts.get(eq_i + 1) {
            Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::Expr => x,
            _ => {
                return Err(diag(
                    "VALUE_EXPECTED",
                    span_of_node(n),
                    "expected initializer expression after `=`",
                ));
            }
        };
        (
            name_tok,
            Some(lower_expr_wrapped(init_node, ctx)?),
            eq_i - 1,
        )
    } else {
        let mut name_idx = parts.len();
        if matches!(
            parts.last(),
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Semicolon
        ) {
            name_idx -= 1;
        }
        if name_idx == 0 {
            return Err(diag(
                "VAR_NAME_EXPECTED",
                span_of_node(n),
                "expected identifier in typed declaration",
            ));
        }
        let name_tok = match &parts[name_idx - 1] {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident => t,
            _ => {
                return Err(diag(
                    "VAR_NAME_EXPECTED",
                    span_of_node(n),
                    "expected identifier name in typed declaration",
                ));
            }
        };
        (name_tok, None, name_idx - 1)
    };
    let decl_ty = decl_ty_src_before_name(&parts, name_part_idx, ctx);
    let name = NameDef {
        name: token_text(name_tok, ctx.src).to_string(),
        span: span_of_range(name_tok.text_range()),
    };
    Ok(HirStmt::Var {
        name,
        init,
        decl_ty,
    })
}

pub(super) fn lower_expr_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 1 || parts.len() > 2 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed ExprStmt: {} elements", parts.len()),
        ));
    }
    let expr_node = match &parts[0] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected Expr in expression statement",
            ));
        }
    };
    let e = lower_expr_wrapped(expr_node, ctx)?;
    if parts.len() == 2 {
        if !matches!(
            &parts[1],
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Semicolon
        ) {
            return Err(diag(
                "END_OF_INSTRUCTION_EXPECTED",
                span_of_node(n),
                "expected `;`",
            ));
        }
    }
    Ok(HirStmt::Expr(e))
}

pub(super) fn lower_return_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.is_empty() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "empty ReturnStmt",
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "return" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `return`",
            ));
        }
    }
    let mut i = 1usize;
    let mut if_truthy = false;
    if i < parts.len() {
        if let NodeOrToken::Token(t) = &parts[i] {
            if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == "?" {
                if_truthy = true;
                i += 1;
            }
        }
    }
    let mut by_ref = false;
    if i < parts.len() {
        if let NodeOrToken::Token(t) = &parts[i] {
            if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == "@" {
                by_ref = true;
                i += 1;
            }
        }
    }
    let value = if i < parts.len() {
        match &parts[i] {
            NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Expr => {
                let e = lower_expr_wrapped(x, ctx)?;
                i += 1;
                Some(e)
            }
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Semicolon => {
                i += 1;
                None
            }
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span_of_node(n),
                    "expected expression or `;` after `return`",
                ));
            }
        }
    } else {
        None
    };
    if i < parts.len() {
        if !matches!(
            &parts[i],
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Semicolon
        ) {
            return Err(diag(
                "END_OF_INSTRUCTION_EXPECTED",
                span_of_node(n),
                "expected `;`",
            ));
        }
        i += 1;
    }
    if i != parts.len() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("unexpected tokens in ReturnStmt ({} parts)", parts.len()),
        ));
    }
    Ok(HirStmt::Return {
        value,
        if_truthy,
        by_ref,
    })
}

pub(super) fn lower_block_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    Ok(HirStmt::Block(lower_block_stmts(n, ctx)?))
}

pub(super) fn lower_block_stmts(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<Vec<HirStmt>, HirLoweringDiagnostic> {
    let mut stmts = Vec::new();
    for el in non_trivia(n) {
        if let NodeOrToken::Node(ch) = el {
            if ch.kind() == LeekSyntaxKind::BraceOpen || ch.kind() == LeekSyntaxKind::BraceClose {
                continue;
            }
            stmts.extend(lower_stmt(&ch, ctx)?);
        }
    }
    Ok(stmts)
}

pub(super) fn lower_if_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 5 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed IfStmt: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "if" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `if`",
            ));
        }
    }
    if !matches!(
        &parts[1],
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenOpen
    ) {
        return Err(diag(
            "OPENING_PARENTHESIS_EXPECTED",
            span_of_node(n),
            "expected `(`",
        ));
    }
    let cond_node = match &parts[2] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected condition expression",
            ));
        }
    };
    let cond = lower_expr_wrapped(cond_node, ctx)?;
    if !matches!(
        &parts[3],
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenClose
    ) {
        return Err(diag(
            "CLOSING_PARENTHESIS_EXPECTED",
            span_of_node(n),
            "expected `)`",
        ));
    }
    let then_body = lower_stmt_body(&parts[4], ctx)?;
    let else_body = if parts.len() > 5 {
        match &parts[5] {
            NodeOrToken::Token(t)
                if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "else" => {}
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span_of_node(n),
                    "expected `else`",
                ));
            }
        }
        match &parts[6] {
            NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::IfStmt => {
                let inner = lower_if_stmt(x, ctx)?;
                Some(vec![inner])
            }
            other => Some(lower_stmt_body(other, ctx)?),
        }
    } else {
        None
    };
    Ok(HirStmt::If {
        cond,
        then_body,
        else_body,
    })
}

fn lower_stmt_body(
    el: &NodeOrToken<SyntaxNode<LeekLanguage>, rowan::SyntaxToken<LeekLanguage>>,
    ctx: &LowerCtx,
) -> Result<Vec<HirStmt>, HirLoweringDiagnostic> {
    match el {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Block => lower_block_stmts(x, ctx),
        NodeOrToken::Node(x) => lower_stmt(x, ctx),
        _ => Err(diag(
            "INTERNAL_ERROR",
            leekscript_span::Span::point(0),
            "expected statement node for body",
        )),
    }
}

pub(super) fn lower_while_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 5 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed WhileStmt: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "while" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `while`",
            ));
        }
    }
    if !matches!(
        &parts[1],
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenOpen
    ) {
        return Err(diag(
            "OPENING_PARENTHESIS_EXPECTED",
            span_of_node(n),
            "expected `(`",
        ));
    }
    let cond_node = match &parts[2] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected condition expression",
            ));
        }
    };
    let cond = lower_expr_wrapped(cond_node, ctx)?;
    if !matches!(
        &parts[3],
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenClose
    ) {
        return Err(diag(
            "CLOSING_PARENTHESIS_EXPECTED",
            span_of_node(n),
            "expected `)`",
        ));
    }
    let body = lower_stmt_body(&parts[4], ctx)?;
    Ok(HirStmt::While { cond, body })
}

pub(super) fn lower_do_while_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 6 && parts.len() != 7 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed DoWhileStmt: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "do" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `do`",
            ));
        }
    }
    let body_block = match &parts[1] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Block => x,
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span_of_node(n),
                "expected `do` body block",
            ));
        }
    };
    let body = lower_block_stmts(body_block, ctx)?;
    match &parts[2] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "while" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `while`",
            ));
        }
    }
    if !matches!(
        &parts[3],
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenOpen
    ) {
        return Err(diag(
            "OPENING_PARENTHESIS_EXPECTED",
            span_of_node(n),
            "expected `(`",
        ));
    }
    let cond_node = match &parts[4] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected condition expression",
            ));
        }
    };
    let cond = lower_expr_wrapped(cond_node, ctx)?;
    if !matches!(
        &parts[5],
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenClose
    ) {
        return Err(diag(
            "CLOSING_PARENTHESIS_EXPECTED",
            span_of_node(n),
            "expected `)`",
        ));
    }
    if parts.len() == 7 {
        if !matches!(
            &parts[6],
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Semicolon
        ) {
            return Err(diag(
                "UNEXPECTED_TOKEN",
                span_of_node(n),
                "expected `;` after `do` … `while`",
            ));
        }
    }
    Ok(HirStmt::DoWhile { body, cond })
}

pub(super) fn lower_switch_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let mut it = non_trivia(n).peekable();
    let el = it
        .next()
        .ok_or_else(|| diag("INTERNAL_ERROR", span_of_node(n), "empty SwitchStmt"))?;
    match &el {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "switch" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `switch`",
            ));
        }
    }
    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenOpen => {}
        _ => {
            return Err(diag(
                "OPENING_PARENTHESIS_EXPECTED",
                span_of_node(n),
                "expected `(`",
            ));
        }
    }
    let discr_node = match it.next() {
        Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected switch discriminant expression",
            ));
        }
    };
    let discr = lower_expr_wrapped(&discr_node, ctx)?;
    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenClose => {}
        _ => {
            return Err(diag(
                "CLOSING_PARENTHESIS_EXPECTED",
                span_of_node(n),
                "expected `)`",
            ));
        }
    }
    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BraceOpen => {}
        _ => {
            return Err(diag("UNEXPECTED_TOKEN", span_of_node(n), "expected `{`"));
        }
    }

    let mut clauses: Vec<HirSwitchClause> = Vec::new();
    while let Some(el) = it.peek() {
        match el {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::BraceClose => break,
            NodeOrToken::Node(node) => match node.kind() {
                LeekSyntaxKind::SwitchCaseClause => {
                    let node = node.clone();
                    it.next();
                    clauses.push(lower_switch_case_clause(&node, ctx)?);
                }
                LeekSyntaxKind::SwitchDefaultClause => {
                    let node = node.clone();
                    it.next();
                    clauses.push(lower_switch_default_clause(&node, ctx)?);
                }
                _ => {
                    return Err(diag(
                        "INTERNAL_ERROR",
                        span_of_node(n),
                        format!("unexpected node in switch: {:?}", node.kind()),
                    ));
                }
            },
            NodeOrToken::Token(t) => {
                return Err(diag(
                    "UNEXPECTED_TOKEN",
                    span_of_range(t.text_range()),
                    "unexpected token in `switch` body",
                ));
            }
        }
    }

    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BraceClose => {}
        _ => {
            return Err(diag(
                "UNEXPECTED_TOKEN",
                span_of_node(n),
                "expected `}` to close `switch`",
            ));
        }
    }
    if it.next().is_some() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "unexpected token after `switch`",
        ));
    }

    Ok(HirStmt::Switch { discr, clauses })
}

pub(super) fn lower_switch_case_clause(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirSwitchClause, HirLoweringDiagnostic> {
    let mut labels: Vec<HirExpr> = Vec::new();
    let mut body: Vec<HirStmt> = Vec::new();
    for el in non_trivia(n) {
        match el {
            NodeOrToken::Node(ch) if ch.kind() == LeekSyntaxKind::CaseLabel => {
                labels.push(lower_case_label_value(&ch, ctx)?);
            }
            NodeOrToken::Node(ch) => {
                body.extend(lower_stmt(&ch, ctx)?);
            }
            NodeOrToken::Token(_) => {}
        }
    }
    if labels.is_empty() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "`switch` case arm has no labels",
        ));
    }
    Ok(HirSwitchClause::Case { labels, body })
}

pub(super) fn lower_case_label_value(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 3 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed CaseLabel: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "case" => {
        }
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `case`",
            ));
        }
    }
    let expr_node = match &parts[1] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `case` value expression",
            ));
        }
    };
    let v = lower_expr_wrapped(expr_node, ctx)?;
    if !matches!(
        &parts[2],
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == ":"
    ) {
        return Err(diag(
            "UNEXPECTED_TOKEN",
            span_of_node(n),
            "expected `:` after `case` expression",
        ));
    }
    Ok(v)
}

pub(super) fn lower_switch_default_clause(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirSwitchClause, HirLoweringDiagnostic> {
    let mut it = non_trivia(n).peekable();
    match it.next() {
        Some(NodeOrToken::Token(ref t))
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "default" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `default`",
            ));
        }
    }
    match it.next() {
        Some(NodeOrToken::Token(ref t))
            if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == ":" => {}
        _ => {
            return Err(diag(
                "UNEXPECTED_TOKEN",
                span_of_node(n),
                "expected `:` after `default`",
            ));
        }
    }
    let mut body: Vec<HirStmt> = Vec::new();
    while let Some(NodeOrToken::Node(ch)) = it.peek() {
        let ch = ch.clone();
        it.next();
        body.extend(lower_stmt(&ch, ctx)?);
    }
    if it.next().is_some() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "unexpected token in `default` clause",
        ));
    }
    Ok(HirSwitchClause::Default { body })
}

pub(super) fn lower_for_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let mut it = non_trivia(n).peekable();
    let el = it
        .next()
        .ok_or_else(|| diag("INTERNAL_ERROR", span_of_node(n), "empty ForStmt"))?;
    match &el {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "for" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `for`",
            ));
        }
    }
    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenOpen => {}
        _ => {
            return Err(diag(
                "OPENING_PARENTHESIS_EXPECTED",
                span_of_node(n),
                "expected `(`",
            ));
        }
    }

    let init = match it.peek() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Semicolon => {
            it.next();
            None
        }
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::ForInitVar => {
            let node = node.clone();
            it.next();
            Some(lower_for_init_var_as_stmt(&node, ctx)?)
        }
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::ForAssign => {
            let node = node.clone();
            it.next();
            Some(lower_for_assign_node_as_stmt(&node, ctx)?)
        }
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `;`, `var`, or assignment in for init",
            ));
        }
    };

    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Semicolon => {}
        _ => {
            return Err(diag(
                "END_OF_INSTRUCTION_EXPECTED",
                span_of_node(n),
                "expected `;` after for init",
            ));
        }
    }

    let cond = match it.peek() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Semicolon => {
            it.next();
            None
        }
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::Expr => {
            let node = node.clone();
            it.next();
            Some(lower_expr_wrapped(&node, ctx)?)
        }
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected condition expression or `;`",
            ));
        }
    };

    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Semicolon => {}
        _ => {
            return Err(diag(
                "END_OF_INSTRUCTION_EXPECTED",
                span_of_node(n),
                "expected `;` after for condition",
            ));
        }
    }

    let update = match it.peek() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenClose => {
            it.next();
            None
        }
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::ForAssign => {
            let node = node.clone();
            it.next();
            Some(HirForStep::Assign(lower_for_assign_pair(&node, ctx)?))
        }
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::Expr => {
            let node = node.clone();
            it.next();
            Some(HirForStep::Expr(lower_expr_wrapped(&node, ctx)?))
        }
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `)`, `for` update assignment, or expression",
            ));
        }
    };

    if update.is_some() {
        match it.next() {
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenClose => {}
            _ => {
                return Err(diag(
                    "CLOSING_PARENTHESIS_EXPECTED",
                    span_of_node(n),
                    "expected `)` after for update",
                ));
            }
        }
    }

    let body_el = match it.next() {
        Some(el) => el,
        None => {
            return Err(diag(
                "INTERNAL_ERROR",
                span_of_node(n),
                "expected body after `for (...)`",
            ));
        }
    };
    let body = lower_for_loop_body(body_el, ctx)?;

    if it.next().is_some() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "unexpected token after for body",
        ));
    }

    Ok(HirStmt::For {
        init: init.map(Box::new),
        cond,
        update,
        body,
    })
}

pub(super) fn lower_for_in_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let mut it = non_trivia(n).peekable();
    let el = it
        .next()
        .ok_or_else(|| diag("INTERNAL_ERROR", span_of_node(n), "empty ForInStmt"))?;
    match &el {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "for" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `for`",
            ));
        }
    }
    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenOpen => {}
        _ => {
            return Err(diag(
                "OPENING_PARENTHESIS_EXPECTED",
                span_of_node(n),
                "expected `(`",
            ));
        }
    }
    let bind_node = match it.next() {
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::ForInBinding => node,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `for`-`in` binding",
            ));
        }
    };
    let (name, is_declaration, name_by_ref) = lower_for_in_binding_detail(&bind_node, ctx)?;
    match it.next() {
        Some(NodeOrToken::Token(ref t))
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "in" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `in`",
            ));
        }
    }
    let cont_node = match it.next() {
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::Expr => node,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected iterable expression",
            ));
        }
    };
    let container = lower_expr_wrapped(&cont_node, ctx)?;
    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenClose => {}
        _ => {
            return Err(diag(
                "CLOSING_PARENTHESIS_EXPECTED",
                span_of_node(n),
                "expected `)`",
            ));
        }
    }
    let body_el = match it.next() {
        Some(el) => el,
        None => {
            return Err(diag(
                "INTERNAL_ERROR",
                span_of_node(n),
                "expected body after `for` `(` … `)`",
            ));
        }
    };
    let body = lower_for_loop_body(body_el, ctx)?;
    if it.next().is_some() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "unexpected token after `for`-`in` body",
        ));
    }
    Ok(HirStmt::ForIn {
        name,
        is_declaration,
        name_by_ref,
        container,
        body,
    })
}

pub(super) fn lower_for_in_key_value_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let mut it = non_trivia(n).peekable();
    let el = it
        .next()
        .ok_or_else(|| diag("INTERNAL_ERROR", span_of_node(n), "empty ForInKeyValueStmt"))?;
    match &el {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "for" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `for`",
            ));
        }
    }
    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenOpen => {}
        _ => {
            return Err(diag(
                "OPENING_PARENTHESIS_EXPECTED",
                span_of_node(n),
                "expected `(`",
            ));
        }
    }
    let key_node = match it.next() {
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::ForInBinding => node,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `for` key binding",
            ));
        }
    };
    let (key, key_is_declaration, key_by_ref) = lower_for_in_binding_detail(&key_node, ctx)?;
    match it.next() {
        Some(NodeOrToken::Token(ref t))
            if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == ":" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `:` between key and value in `for`-`in` header",
            ));
        }
    }
    let val_node = match it.next() {
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::ForInBinding => node,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `for` value binding",
            ));
        }
    };
    let (value, value_is_declaration, value_by_ref) =
        lower_for_in_binding_detail(&val_node, ctx)?;
    match it.next() {
        Some(NodeOrToken::Token(ref t))
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "in" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `in`",
            ));
        }
    }
    let cont_node = match it.next() {
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::Expr => node,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected iterable expression",
            ));
        }
    };
    let container = lower_expr_wrapped(&cont_node, ctx)?;
    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenClose => {}
        _ => {
            return Err(diag(
                "CLOSING_PARENTHESIS_EXPECTED",
                span_of_node(n),
                "expected `)`",
            ));
        }
    }
    let body_el = match it.next() {
        Some(el) => el,
        None => {
            return Err(diag(
                "INTERNAL_ERROR",
                span_of_node(n),
                "expected body after `for` `(` … `)`",
            ));
        }
    };
    let body = lower_for_loop_body(body_el, ctx)?;
    if it.next().is_some() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "unexpected token after `for` key-value body",
        ));
    }
    Ok(HirStmt::ForInKeyValue {
        key,
        key_is_declaration,
        key_by_ref,
        value,
        value_is_declaration,
        value_by_ref,
        container,
        body,
    })
}

pub(super) fn lower_for_in_binding_detail(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<(NameDef, bool, bool), HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    let mut i = 0usize;
    let mut has_type = false;
    if let Some(NodeOrToken::Node(node)) = parts.get(i) {
        if node.kind() == LeekSyntaxKind::ForInTypeAnn {
            has_type = true;
            i += 1;
        }
    }
    let mut has_var = false;
    if let Some(NodeOrToken::Token(t)) = parts.get(i) {
        if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "var" {
            has_var = true;
            i += 1;
        }
    }
    let mut by_ref = false;
    if let Some(NodeOrToken::Token(t)) = parts.get(i) {
        if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == "@" {
            by_ref = true;
            i += 1;
        }
    }
    let id_node = match parts.get(i) {
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::IdentExpr => node,
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span_of_node(n),
                "malformed `for`-`in` binding",
            ));
        }
    };
    i += 1;
    if i != parts.len() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "malformed `for`-`in` binding",
        ));
    }
    let name = match lower_ident_expr(id_node, ctx)? {
        HirExpr::Ident { name, span } => NameDef { name, span },
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span_of_node(n),
                "expected identifier in `for`-`in` binding",
            ));
        }
    };
    let is_declaration = has_type || has_var;
    Ok((name, is_declaration, by_ref))
}

pub(super) fn lower_for_init_var_as_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    let eq_idx = parts
        .iter()
        .position(|p| {
            matches!(
                p,
                NodeOrToken::Token(t)
                    if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == "="
            )
        })
        .ok_or_else(|| {
            diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `=` in `for` init",
            )
        })?;
    if eq_idx < 1 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "malformed ForInitVar",
        ));
    }
    let name_tok = match &parts[eq_idx - 1] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident => t,
        _ => {
            return Err(diag(
                "VAR_NAME_EXPECTED",
                span_of_node(n),
                "expected variable name",
            ));
        }
    };
    let name = NameDef {
        name: token_text(name_tok, ctx.src).to_string(),
        span: span_of_range(name_tok.text_range()),
    };
    let init_node = match parts.get(eq_idx + 1) {
        Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "VALUE_EXPECTED",
                span_of_node(n),
                "expected initializer",
            ));
        }
    };
    let init = lower_expr_wrapped(init_node, ctx)?;
    let decl_ty = if eq_idx == 2
        && matches!(
            &parts[0],
            NodeOrToken::Token(t)
                if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "var"
        )
    {
        None
    } else {
        decl_ty_src_before_name(&parts, eq_idx - 1, ctx)
    };
    Ok(HirStmt::Var {
        name,
        init: Some(init),
        decl_ty,
    })
}

pub(super) fn lower_for_assign_pair(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirForUpdate, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 3 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed ForAssign: {} elements", parts.len()),
        ));
    }
    let name_tok = match &parts[0] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident => t,
        _ => {
            return Err(diag(
                "VAR_NAME_EXPECTED",
                span_of_node(n),
                "expected identifier",
            ));
        }
    };
    let name = NameDef {
        name: token_text(name_tok, ctx.src).to_string(),
        span: span_of_range(name_tok.text_range()),
    };
    let op_tok = match &parts[1] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Operator => t,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected assignment operator",
            ));
        }
    };
    let op_txt = token_text(op_tok, ctx.src);
    let Some(op) = hir_assign_op_from_token(op_txt) else {
        return Err(diag(
            "INVALID_OPERATOR",
            span_of_range(op_tok.text_range()),
            "unsupported assignment operator",
        ));
    };
    let val_node = match &parts[2] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "VALUE_EXPECTED",
                span_of_node(n),
                "expected expression",
            ));
        }
    };
    let value = lower_expr_wrapped(val_node, ctx)?;
    Ok(HirForUpdate { name, op, value })
}

pub(super) fn lower_empty_stmt(
    n: &SyntaxNode<LeekLanguage>,
    _ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    match parts.as_slice() {
        [NodeOrToken::Token(t)] if t.kind() == LeekSyntaxKind::Semicolon => Ok(HirStmt::Empty),
        _ => Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed EmptyStmt: {} elements", parts.len()),
        )),
    }
}

pub(super) fn lower_for_assign_node_as_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let HirForUpdate { name, op, value } = lower_for_assign_pair(n, ctx)?;
    let place = HirExpr::Ident {
        name: name.name.clone(),
        span: name.span,
    };
    Ok(HirStmt::Assign {
        place: Box::new(place),
        op,
        value,
    })
}

pub(super) fn is_valid_assign_place(e: &HirExpr) -> bool {
    match e {
        HirExpr::Ident { .. } | HirExpr::This => true,
        HirExpr::Index { base, .. } | HirExpr::Member { base, .. } => match base.as_ref() {
            HirExpr::New { .. } | HirExpr::ClassSelf { .. } => true,
            b => is_valid_assign_place(b),
        },
        _ => false,
    }
}

pub(super) fn lower_assign_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 3 || parts.len() > 4 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed AssignStmt: {} elements", parts.len()),
        ));
    }
    let target_node = match &parts[0] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected assignment target expression",
            ));
        }
    };
    let place = lower_expr(target_node, ctx)?;
    if !is_valid_assign_place(&place) {
        return Err(diag(
            "INVALID_ASSIGN_TARGET",
            span_of_node(n),
            "invalid left-hand side of assignment",
        ));
    }
    let op_tok = match &parts[1] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Operator => t,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected assignment operator",
            ));
        }
    };
    let op_txt = token_text(op_tok, ctx.src);
    let Some(op) = hir_assign_op_from_token(op_txt) else {
        return Err(diag(
            "INVALID_OPERATOR",
            span_of_range(op_tok.text_range()),
            "unsupported assignment operator",
        ));
    };
    let val_node = match &parts[2] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "VALUE_EXPECTED",
                span_of_node(n),
                "expected right-hand expression",
            ));
        }
    };
    let value = lower_expr_wrapped(val_node, ctx)?;
    if parts.len() == 4 {
        if !matches!(
            &parts[3],
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Semicolon
        ) {
            return Err(diag(
                "END_OF_INSTRUCTION_EXPECTED",
                span_of_node(n),
                "expected `;`",
            ));
        }
    }
    Ok(HirStmt::Assign {
        place: Box::new(place),
        op,
        value,
    })
}

pub(super) fn lower_break_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 1 || parts.len() > 2 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed BreakStmt: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "break" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `break`",
            ));
        }
    }
    if parts.len() == 2 {
        if !matches!(
            &parts[1],
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Semicolon
        ) {
            return Err(diag(
                "END_OF_INSTRUCTION_EXPECTED",
                span_of_node(n),
                "expected `;`",
            ));
        }
    }
    Ok(HirStmt::Break)
}

pub(super) fn lower_continue_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 1 || parts.len() > 2 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed ContinueStmt: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "continue" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `continue`",
            ));
        }
    }
    if parts.len() == 2 {
        if !matches!(
            &parts[1],
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Semicolon
        ) {
            return Err(diag(
                "END_OF_INSTRUCTION_EXPECTED",
                span_of_node(n),
                "expected `;`",
            ));
        }
    }
    Ok(HirStmt::Continue)
}
pub(super) fn lower_try_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    let mut i = 0usize;
    match parts.get(i) {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "try" => {
            i += 1;
        }
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected `try`",
            ));
        }
    }
    let try_block = match parts.get(i) {
        Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::Block => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected `try` body",
            ));
        }
    };
    i += 1;
    let mut catch = None;
    if let Some(NodeOrToken::Token(t)) = parts.get(i) {
        if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "catch" {
            i += 1;
            match parts.get(i) {
                Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenOpen => i += 1,
                _ => {
                    return Err(diag(
                        "OPENING_PARENTHESIS_EXPECTED",
                        span,
                        "expected `(` after `catch`",
                    ));
                }
            }
            let param_tok = match parts.get(i) {
                Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Ident => t,
                _ => {
                    return Err(diag(
                        "UNCOMPLETE_EXPRESSION",
                        span,
                        "expected catch parameter",
                    ));
                }
            };
            let catch_param = NameDef {
                name: token_text(param_tok, ctx.src).to_string(),
                span: span_of_range(param_tok.text_range()),
            };
            i += 1;
            match parts.get(i) {
                Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenClose => i += 1,
                _ => {
                    return Err(diag(
                        "CLOSING_PARENTHESIS_EXPECTED",
                        span,
                        "expected `)` after catch parameter",
                    ));
                }
            }
            let catch_block = match parts.get(i) {
                Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::Block => x,
                _ => {
                    return Err(diag(
                        "UNCOMPLETE_EXPRESSION",
                        span,
                        "expected `catch` body",
                    ));
                }
            };
            i += 1;
            let catch_body = lower_block_stmts(catch_block, ctx)?;
            catch = Some((catch_param, catch_body));
        }
    }
    let mut finally_body = None;
    if let Some(NodeOrToken::Token(t)) = parts.get(i) {
        if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "finally" {
            i += 1;
            let fb = match parts.get(i) {
                Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::Block => x,
                _ => {
                    return Err(diag(
                        "UNCOMPLETE_EXPRESSION",
                        span,
                        "expected `finally` body",
                    ));
                }
            };
            i += 1;
            finally_body = Some(lower_block_stmts(fb, ctx)?);
        }
    }
    if i != parts.len() {
        return Err(diag(
            "INTERNAL_ERROR",
            span,
            "unexpected tokens after `try` statement",
        ));
    }
    if catch.is_none() && finally_body.is_none() {
        return Err(diag(
            "UNCOMPLETE_EXPRESSION",
            span,
            "expected `catch` or `finally` after `try`",
        ));
    }
    let try_body = lower_block_stmts(try_block, ctx)?;
    Ok(HirStmt::Try {
        try_body,
        catch,
        finally_body,
    })
}

pub(super) fn lower_global_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 2 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed GlobalStmt: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "global" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `global`",
            ));
        }
    }
    // Java suite allows `global x` without trailing `;` (e.g. `global x global y ...`).
    if parts.len() == 2 {
        let name_tok = match &parts[1] {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident => t,
            _ => {
                return Err(diag(
                    "VAR_NAME_EXPECTED",
                    span_of_node(n),
                    "expected identifier in `global` declaration",
                ));
            }
        };
        let name = NameDef {
            name: token_text(name_tok, ctx.src).to_string(),
            span: span_of_range(name_tok.text_range()),
        };
        return Ok(HirStmt::Global {
            decl_ty: None,
            entries: vec![(name, None)],
        });
    }

    let mut entries = Vec::new();
    let mut decl_ty: Option<String> = None;
    let mut i = 1usize;
    if let Some(NodeOrToken::Node(ch)) = parts.get(i) {
        if ch.kind() == LeekSyntaxKind::GlobalLeadingType {
            let range = ch.text_range();
            let raw = &ctx.src[range.start().into()..range.end().into()];
            decl_ty = Some(raw.trim().to_string());
            i += 1;
        }
    }
    loop {
        let name_tok = match parts.get(i) {
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Ident => t,
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Semicolon => {
                if entries.is_empty() {
                    return Err(diag(
                        "VAR_NAME_EXPECTED",
                        span_of_node(n),
                        "`global` needs at least one identifier",
                    ));
                }
                return Ok(HirStmt::Global { decl_ty, entries });
            }
            _ => {
                return Err(diag(
                    "VAR_NAME_EXPECTED",
                    span_of_node(n),
                    "expected identifier in `global` declaration",
                ));
            }
        };
        let name = NameDef {
            name: token_text(name_tok, ctx.src).to_string(),
            span: span_of_range(name_tok.text_range()),
        };
        i += 1;
        let init = if i < parts.len() {
            if let NodeOrToken::Token(t) = &parts[i] {
                if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == "=" {
                    i += 1;
                    let expr_n = match parts.get(i) {
                        Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::Expr => x,
                        _ => {
                            return Err(diag(
                                "VALUE_EXPECTED",
                                span_of_node(n),
                                "expected initializer after `=`",
                            ));
                        }
                    };
                    let e = lower_expr_wrapped(expr_n, ctx)?;
                    i += 1;
                    Some(e)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        entries.push((name, init));
        match parts.get(i) {
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Comma => {
                i += 1;
            }
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Semicolon => {
                return Ok(HirStmt::Global { decl_ty, entries });
            }
            None => {
                return Ok(HirStmt::Global { decl_ty, entries });
            }
            _ => {
                return Err(diag(
                    "END_OF_INSTRUCTION_EXPECTED",
                    span_of_node(n),
                    "expected `,` or `;` after `global` binding",
                ));
            }
        }
    }
}

pub(super) fn lower_include_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 4 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed IncludeStmt: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "include" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `include`",
            ));
        }
    }
    if !matches!(
        &parts[1],
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenOpen
    ) {
        return Err(diag(
            "OPENING_PARENTHESIS_EXPECTED",
            span_of_node(n),
            "expected `(`",
        ));
    }
    // Match Java `include` parsing: string literal must follow the opening `(` immediately.
    let lit_idx = 2usize;
    let lit_n = match parts.get(lit_idx) {
        Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::LiteralExpr => x,
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenOpen => {
            return Err(diag(
                "AI_NAME_EXPECTED",
                span_of_node(n),
                "expected string literal in `include`",
            ));
        }
        _ => {
            return Err(diag(
                "AI_NAME_EXPECTED",
                span_of_node(n),
                "expected string literal in `include`",
            ));
        }
    };
    let lit = super::expr::lower_literal(lit_n, ctx)?;
    let path = match lit {
        HirExpr::String(s) => s,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "include path must be a string literal",
            ));
        }
    };
    let mut i = lit_idx + 1;
    match parts.get(i) {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenClose => {
            i += 1;
        }
        _ => {
            return Err(diag(
                "CLOSING_PARENTHESIS_EXPECTED",
                span_of_node(n),
                "expected `)`",
            ));
        }
    }
    match parts.get(i) {
        None => {}
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Semicolon => {
            i += 1;
        }
        _ => {
            return Err(diag(
                "END_OF_INSTRUCTION_EXPECTED",
                span_of_node(n),
                "expected `;`",
            ));
        }
    }
    if i != parts.len() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed IncludeStmt: {} trailing elements", parts.len() - i),
        ));
    }
    Ok(HirStmt::Include {
        path,
        span: span_of_node(n),
    })
}

pub(super) fn lower_throw_stmt(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    match parts.as_slice() {
        [NodeOrToken::Token(t1), NodeOrToken::Token(semi)]
            if t1.kind() == LeekSyntaxKind::Kw
                && token_text(t1, ctx.src) == "throw"
                && semi.kind() == LeekSyntaxKind::Semicolon =>
        {
            Ok(HirStmt::Throw(None))
        }
        [NodeOrToken::Token(t1), NodeOrToken::Node(expr)]
            if t1.kind() == LeekSyntaxKind::Kw
                && token_text(t1, ctx.src) == "throw"
                && expr.kind() == LeekSyntaxKind::Expr =>
        {
            Ok(HirStmt::Throw(Some(lower_expr_wrapped(expr, ctx)?)))
        }
        [NodeOrToken::Token(t1), NodeOrToken::Node(expr), NodeOrToken::Token(semi)]
            if t1.kind() == LeekSyntaxKind::Kw
                && token_text(t1, ctx.src) == "throw"
                && expr.kind() == LeekSyntaxKind::Expr
                && semi.kind() == LeekSyntaxKind::Semicolon =>
        {
            Ok(HirStmt::Throw(Some(lower_expr_wrapped(expr, ctx)?)))
        }
        _ => Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed ThrowStmt: {} elements", parts.len()),
        )),
    }
}

pub(super) fn lower_class_decl(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 4 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed ClassDecl: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "class" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `class`",
            ));
        }
    }
    let name_tok = match &parts[1] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident => t,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected class name",
            ));
        }
    };
    let name = NameDef {
        name: token_text(name_tok, ctx.src).to_string(),
        span: span_of_range(name_tok.text_range()),
    };
    let mut extends: Option<NameDef> = None;
    let mut brace_idx = 2usize;
    if let Some(NodeOrToken::Token(t)) = parts.get(brace_idx) {
        if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "extends" {
            let super_tok = match parts.get(brace_idx + 1) {
                Some(NodeOrToken::Token(t))
                    if t.kind() == LeekSyntaxKind::Ident =>
                {
                    t
                }
                _ => {
                    return Err(diag(
                        "UNCOMPLETE_EXPRESSION",
                        span_of_node(n),
                        "expected superclass name after `extends`",
                    ));
                }
            };
            extends = Some(NameDef {
                name: token_text(super_tok, ctx.src).to_string(),
                span: span_of_range(super_tok.text_range()),
            });
            brace_idx += 2;
        }
    }
    match parts.get(brace_idx) {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BraceOpen => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `{` after class header",
            ));
        }
    }
    match parts.last() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BraceClose => {}
        _ => {
            return Err(diag(
                "END_OF_SCRIPT_UNEXPECTED",
                span_of_node(n),
                "expected `}` at end of class body",
            ));
        }
    }

    let mut members = Vec::new();
    for p in &parts[brace_idx + 1..parts.len() - 1] {
        match p {
            NodeOrToken::Node(ch) if ch.kind() == LeekSyntaxKind::FunctionDecl => {
                let parts: Vec<_> = non_trivia(ch).collect();
                let mut i0 = 0usize;
                if let Some(NodeOrToken::Token(t)) = parts.first() {
                    if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "function" {
                        i0 = 1;
                    }
                }
                let mut is_static = false;
                let mut visibility = crate::HirFieldVisibility::Public;
                while i0 < parts.len() {
                    match &parts[i0] {
                        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Kw => {
                            let tx = token_text(t, ctx.src);
                            if tx == "static" {
                                is_static = true;
                                i0 += 1;
                                continue;
                            }
                            if tx == "private" {
                                visibility = crate::HirFieldVisibility::Private;
                                i0 += 1;
                                continue;
                            }
                            if tx == "protected" {
                                visibility = crate::HirFieldVisibility::Protected;
                                i0 += 1;
                                continue;
                            }
                            if tx == "public" {
                                visibility = crate::HirFieldVisibility::Public;
                                i0 += 1;
                                continue;
                            }
                            if tx == "final" {
                                i0 += 1;
                                continue;
                            }
                        }
                        _ => {}
                    }
                    break;
                }
                let st = lower_function_decl(ch, ctx)?;
                let HirStmt::FnDecl {
                    name,
                    params,
                    return_ty: _,
                    body,
                } = st
                else {
                    return Err(diag(
                        "INTERNAL_ERROR",
                        span_of_node(ch),
                        "FunctionDecl must lower to FnDecl",
                    ));
                };
                members.push(HirClassMember::Method {
                    name,
                    is_static,
                    visibility,
                    params,
                    body,
                });
            }
            NodeOrToken::Node(ch) if ch.kind() == LeekSyntaxKind::ConstructorDecl => {
                members.push(lower_constructor_decl(ch, ctx)?);
            }
            NodeOrToken::Node(ch) if ch.kind() == LeekSyntaxKind::ClassFieldDecl => {
                members.push(lower_class_field_decl(ch, ctx)?);
            }
            _ => {
                return Err(diag(
                    "UNEXPECTED_TOKEN",
                    span_of_node(n),
                    "unexpected node in class body",
                ));
            }
        }
    }

    Ok(HirStmt::ClassDecl {
        name,
        extends,
        members,
    })
}

fn lower_class_field_decl(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirClassMember, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.is_empty() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "empty ClassFieldDecl",
        ));
    }
    let mut i = 0usize;
    let mut is_static = false;
    let mut is_final = false;
    let mut visibility = crate::HirFieldVisibility::Public;
    while let Some(NodeOrToken::Token(t)) = parts.get(i) {
        if t.kind() != LeekSyntaxKind::Kw {
            break;
        }
        let tx = token_text(t, ctx.src);
        match tx {
            "private" => {
                visibility = crate::HirFieldVisibility::Private;
                i += 1;
            }
            "protected" => {
                visibility = crate::HirFieldVisibility::Protected;
                i += 1;
            }
            "public" => {
                visibility = crate::HirFieldVisibility::Public;
                i += 1;
            }
            "final" => {
                is_final = true;
                i += 1;
            }
            "static" => {
                is_static = true;
                i += 1;
            }
            _ => break,
        }
    }
    let eq_pos = parts.iter().position(|p| {
        matches!(
            p,
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == "="
        )
    });
    let decl_ty = |name_end: usize| -> Option<String> {
        if i >= name_end {
            return None;
        }
        let mut s = String::new();
        for p in &parts[i..name_end] {
            if let NodeOrToken::Token(t) = p {
                s.push_str(token_text(t, ctx.src));
            }
        }
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    };
    let (name, init, decl_ty) = if let Some(ep) = eq_pos {
        if ep <= i {
            return Err(diag(
                "UNEXPECTED_TOKEN",
                span_of_node(n),
                "malformed class field",
            ));
        }
        let name_tok = match &parts[ep - 1] {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident => t,
            _ => {
                return Err(diag(
                    "UNEXPECTED_TOKEN",
                    span_of_node(n),
                    "expected field name before `=`",
                ));
            }
        };
        let name = NameDef {
            name: token_text(name_tok, ctx.src).to_string(),
            span: span_of_range(name_tok.text_range()),
        };
        let init_n = match parts.get(ep + 1) {
            Some(NodeOrToken::Node(ch)) if ch.kind() == LeekSyntaxKind::Expr => {
                super::expr::lower_expr_wrapped(ch, ctx)?
            }
            _ => {
                return Err(diag(
                    "UNEXPECTED_TOKEN",
                    span_of_node(n),
                    "expected initializer expression",
                ));
            }
        };
        let dt = decl_ty(ep - 1);
        (name, Some(init_n), dt)
    } else {
        let name_idx = match parts.last() {
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Semicolon => {
                if parts.len() < 2 {
                    return Err(diag(
                        "UNEXPECTED_TOKEN",
                        span_of_node(n),
                        "malformed class field",
                    ));
                }
                parts.len() - 2
            }
            _ => parts.len() - 1,
        };
        if name_idx < i {
            return Err(diag(
                "UNEXPECTED_TOKEN",
                span_of_node(n),
                "malformed class field",
            ));
        }
        let name_tok = match &parts[name_idx] {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident => t,
            _ => {
                return Err(diag(
                    "UNEXPECTED_TOKEN",
                    span_of_node(n),
                    "expected field name",
                ));
            }
        };
        let name = NameDef {
            name: token_text(name_tok, ctx.src).to_string(),
            span: span_of_range(name_tok.text_range()),
        };
        let dt = decl_ty(name_idx);
        (name, None, dt)
    };
    Ok(HirClassMember::Field {
        name,
        decl_ty,
        init,
        is_static,
        is_final,
        visibility,
    })
}

pub(super) fn lower_paren_param_list(
    parts: &[NodeOrToken<SyntaxNode<LeekLanguage>, SyntaxToken<LeekLanguage>>],
    mut i: usize,
    ctx: &LowerCtx,
    err_ctx: &SyntaxNode<LeekLanguage>,
) -> Result<(Vec<HirParam>, usize), HirLoweringDiagnostic> {
    let mut params = Vec::new();
    loop {
        match parts.get(i) {
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenClose => {
                return Ok((params, i + 1));
            }
            Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::FnParam => {
                params.push(lower_fn_param(node, ctx)?);
                i += 1;
            }
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Comma => {
                i += 1;
            }
            Some(p) => {
                let span = match p {
                    NodeOrToken::Node(n) => span_of_node(n),
                    NodeOrToken::Token(t) => span_of_range(t.text_range()),
                };
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span,
                    "expected parameter, `,`, or `)`",
                ));
            }
            None => {
                return Err(diag(
                    "CLOSING_PARENTHESIS_EXPECTED",
                    span_of_node(err_ctx),
                    "expected `)`",
                ));
            }
        }
    }
}

fn lower_constructor_decl(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirClassMember, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 4 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed ConstructorDecl: {} elements", parts.len()),
        ));
    }
    let mut i0 = 0usize;
    let mut visibility = crate::HirFieldVisibility::Public;
    while i0 < parts.len() {
        match &parts[i0] {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Kw => {
                let tx = token_text(t, ctx.src);
                if tx == "private" {
                    visibility = crate::HirFieldVisibility::Private;
                    i0 += 1;
                    continue;
                }
                if tx == "protected" {
                    visibility = crate::HirFieldVisibility::Protected;
                    i0 += 1;
                    continue;
                }
                if tx == "public" {
                    visibility = crate::HirFieldVisibility::Public;
                    i0 += 1;
                    continue;
                }
                if matches!(tx, "static" | "final") {
                    i0 += 1;
                    continue;
                }
            }
            _ => {}
        }
        break;
    }
    match parts.get(i0) {
        Some(NodeOrToken::Token(t))
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "constructor" => {}
        _ => {
            return Err(diag(
                "UNEXPECTED_TOKEN",
                span_of_node(n),
                "expected `constructor`",
            ));
        }
    }
    let paren_idx = i0 + 1;
    if !matches!(&parts.get(paren_idx), Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenOpen)
    {
        return Err(diag(
            "OPENING_PARENTHESIS_EXPECTED",
            span_of_node(n),
            "expected `(` after `constructor`",
        ));
    }
    let (params, i) = lower_paren_param_list(&parts, paren_idx + 1, ctx, n)?;
    let body_n = match parts.get(i) {
        Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::Block => x,
        _ => {
            return Err(diag(
                "UNEXPECTED_TOKEN",
                span_of_node(n),
                "expected constructor body block",
            ));
        }
    };
    let body = lower_block_stmts(body_n, ctx)?;
    Ok(HirClassMember::Constructor {
        params,
        body,
        visibility,
    })
}

pub(super) fn lower_function_decl(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirStmt, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 4 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed FunctionDecl: {} elements", parts.len()),
        ));
    }
    let mut i0 = 0usize;
    if let Some(NodeOrToken::Token(t)) = parts.first() {
        if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "function" {
            i0 = 1;
        }
    }
    while i0 < parts.len() {
        match &parts[i0] {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Kw => {
                let tx = token_text(t, ctx.src);
                if matches!(tx, "private" | "public" | "protected" | "static" | "final") {
                    i0 += 1;
                    continue;
                }
            }
            _ => {}
        }
        break;
    }
    let paren_idx = parts[i0..]
        .iter()
        .position(|p| matches!(p, NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenOpen))
        .map(|j| j + i0)
        .ok_or_else(|| {
            diag(
                "OPENING_PARENTHESIS_EXPECTED",
                span_of_node(n),
                "expected `(` in function header",
            )
        })?;
    if paren_idx <= i0 {
        return Err(diag(
            "FUNCTION_NAME_EXPECTED",
            span_of_node(n),
            "expected function name",
        ));
    }
    let name_tok = match &parts[paren_idx - 1] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident => t,
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "include" =>
        {
            t
        }
        _ => {
            return Err(diag(
                "FUNCTION_NAME_EXPECTED",
                span_of_node(n),
                "expected function name",
            ));
        }
    };
    let name = NameDef {
        name: token_text(name_tok, ctx.src).to_string(),
        span: span_of_range(name_tok.text_range()),
    };
    let (params, mut i) = lower_paren_param_list(&parts, paren_idx + 1, ctx, n)?;
    let mut return_ty: Option<String> = None;
    if let Some(NodeOrToken::Token(t)) = parts.get(i) {
        if t.kind() == LeekSyntaxKind::Arrow {
            let arrow_i = i;
            i += 1;
            while i < parts.len() {
                match &parts[i] {
                    NodeOrToken::Node(node) if node.kind() == LeekSyntaxKind::Block => break,
                    NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Semicolon => break,
                    _ => i += 1,
                }
            }
            if i > arrow_i + 1 {
                return_ty = type_src_between_parts(&parts, arrow_i + 1, i - 1, ctx);
            }
        }
    }
    match parts.get(i) {
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::Block => {
            let body = lower_block_stmts(node, ctx)?;
            Ok(HirStmt::FnDecl {
                name,
                params,
                return_ty,
                body,
            })
        }
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Semicolon => {
            Ok(HirStmt::FnDecl {
                name,
                params,
                return_ty,
                body: vec![],
            })
        }
        _ => Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "expected function body `{ ... }` or signature `;`",
        )),
    }
}

pub(super) fn lower_fn_param(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirParam, HirLoweringDiagnostic> {
    let mut by_ref = false;
    let mut name_tok = None;
    let mut default = None;
    let mut after_eq = false;
    let parts: Vec<_> = non_trivia(n).collect();
    for ch in &parts {
        match ch {
            NodeOrToken::Token(t)
                if t.kind() == LeekSyntaxKind::Operator && token_text(&t, ctx.src) == "@" =>
            {
                by_ref = true;
            }
            NodeOrToken::Token(t)
                if t.kind() == LeekSyntaxKind::Operator && token_text(&t, ctx.src) == "=" =>
            {
                after_eq = true;
            }
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Ident && !after_eq => {
                name_tok = Some(t.clone());
            }
            NodeOrToken::Node(ref ch) if ch.kind() == LeekSyntaxKind::Expr && after_eq => {
                default = Some(super::expr::lower_expr_wrapped(ch, ctx)?);
            }
            _ => {}
        }
    }
    let Some(t) = name_tok else {
        return Err(diag(
            "PARAMETER_NAME_EXPECTED",
            span_of_node(n),
            "expected parameter name",
        ));
    };
    let name_idx = parts.iter().position(|p| match p {
        NodeOrToken::Token(tok) => tok.text_range() == t.text_range(),
        _ => false,
    });
    let decl_ty = name_idx.and_then(|i| decl_ty_src_before_name(&parts, i, ctx));
    Ok(HirParam {
        name: NameDef {
            name: token_text(&t, ctx.src).to_string(),
            span: span_of_range(t.text_range()),
        },
        by_ref,
        decl_ty,
        default,
    })
}
