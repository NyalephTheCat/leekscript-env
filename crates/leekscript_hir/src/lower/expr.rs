//! Lower syntax nodes into [`HirExpr`](crate::nodes::HirExpr).

use super::stmt::{
    hir_assign_op_from_token, is_valid_assign_place, lower_block_stmts, lower_paren_param_list,
};
use super::util::{
    diag, non_trivia, span_of_node, span_of_range, span_start_of_first_non_trivia_token,
    token_text, unquote_string, LowerCtx,
};
use super::HirLoweringDiagnostic;
use crate::nodes::{HirAssignOp, HirBinOp, HirExpr, HirParam, HirTypeExpr, HirUnaryOp, NameDef};
use leekscript_syntax::{LeekLanguage, LeekSyntaxKind};
use rowan::{NodeOrToken, SyntaxNode, SyntaxToken};

pub(super) fn lower_expr_wrapped(
    expr: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    debug_assert_eq!(expr.kind(), LeekSyntaxKind::Expr);
    let mut inner = non_trivia(expr);
    let first = inner
        .next()
        .ok_or_else(|| diag("INTERNAL_ERROR", span_of_node(expr), "empty Expr wrapper"))?;
    let inner_node = match first {
        NodeOrToken::Node(n) => n,
        NodeOrToken::Token(t) => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_range(t.text_range()),
                "expected expression node inside Expr wrapper",
            ));
        }
    };
    if inner.next().is_some() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(expr),
            "Expr wrapper must contain a single subtree",
        ));
    }
    lower_expr(&inner_node, ctx)
}

pub(super) fn lower_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    match n.kind() {
        LeekSyntaxKind::Expr => lower_expr_wrapped(n, ctx),
        LeekSyntaxKind::BinaryExpr => lower_binary(n, ctx),
        LeekSyntaxKind::LiteralExpr => lower_literal(n, ctx),
        LeekSyntaxKind::IdentExpr => lower_ident_expr(n, ctx),
        LeekSyntaxKind::ParenExpr => lower_paren(n, ctx),
        LeekSyntaxKind::CallExpr => lower_call_expr(n, ctx),
        LeekSyntaxKind::UnaryExpr => lower_unary_expr(n, ctx),
        LeekSyntaxKind::ArrayLiteralExpr => lower_array_literal(n, ctx),
        LeekSyntaxKind::MapLiteralExpr => lower_map_literal(n, ctx),
        LeekSyntaxKind::ObjectLiteralExpr => lower_object_literal(n, ctx),
        LeekSyntaxKind::IntervalLiteralExpr => lower_interval_literal(n, ctx),
        LeekSyntaxKind::SetLiteralExpr => lower_set_literal(n, ctx),
        LeekSyntaxKind::NewExpr => lower_new_expr(n, ctx),
        LeekSyntaxKind::IndexExpr => lower_index_expr(n, ctx),
        LeekSyntaxKind::ArraySliceExpr => lower_array_slice_expr(n, ctx),
        LeekSyntaxKind::MemberExpr => lower_member_expr(n, ctx),
        LeekSyntaxKind::TernaryExpr => lower_ternary_expr(n, ctx),
        LeekSyntaxKind::NotInExpr => lower_not_in_expr(n, ctx),
        LeekSyntaxKind::AsCastExpr => lower_as_cast_expr(n, ctx),
        LeekSyntaxKind::PrefixCastExpr => lower_prefix_cast_expr(n, ctx),
        LeekSyntaxKind::ArrowFnExpr => lower_arrow_fn_expr(n, ctx),
        LeekSyntaxKind::FunctionValueExpr => lower_function_value_expr(n, ctx),
        LeekSyntaxKind::PostUpdateExpr => lower_post_update_expr(n, ctx),
        LeekSyntaxKind::PreUpdateExpr => lower_pre_update_expr(n, ctx),
        LeekSyntaxKind::AssignExpr => lower_assign_expr(n, ctx),
        _ => Err(diag(
            "UNCOMPLETE_EXPRESSION",
            span_of_node(n),
            format!("unexpected expression node {:?}", n.kind()),
        )),
    }
}

fn lower_assign_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 3 {
        return Err(diag(
            "INTERNAL_ERROR",
            span,
            format!("malformed AssignExpr: {} elements", parts.len()),
        ));
    }
    let target_node = match &parts[0] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected assignment target expression",
            ));
        }
    };
    let place = lower_expr(target_node, ctx)?;
    if !is_valid_assign_place(&place) {
        return Err(diag(
            "INVALID_ASSIGN_TARGET",
            span,
            "invalid left-hand side of assignment",
        ));
    }
    let op_tok = match &parts[1] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Operator => t,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected assignment operator",
            ));
        }
    };
    let op_txt = token_text(op_tok, ctx.src);
    let val_node = match &parts[2] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "VALUE_EXPECTED",
                span,
                "expected right-hand expression",
            ));
        }
    };
    let value = lower_expr_wrapped(val_node, ctx)?;
    if op_txt == "??=" {
        // Desugar `x ??= y` as `x = (x ?? y)` (sufficient for suite: simple identifier lvalues).
        let combined = HirExpr::Binary {
            op: HirBinOp::NullishCoalesce,
            left: Box::new(place.clone()),
            right: Box::new(value),
        };
        return Ok(HirExpr::AssignExpr {
            place: Box::new(place),
            op: HirAssignOp::Assign,
            value: Box::new(combined),
            span,
        });
    }
    let Some(op) = hir_assign_op_from_token(op_txt) else {
        return Err(diag(
            "INVALID_OPERATOR",
            span_of_range(op_tok.text_range()),
            "unsupported assignment operator",
        ));
    };
    Ok(HirExpr::AssignExpr {
        place: Box::new(place),
        op,
        value: Box::new(value),
        span,
    })
}

fn lower_post_update_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    let inner_n = match parts.first() {
        Some(NodeOrToken::Node(x)) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "PostUpdateExpr must start with Expr wrapper",
            ));
        }
    };
    let increment = match parts.get(1..) {
        Some([NodeOrToken::Token(t)]) if t.kind() == LeekSyntaxKind::Operator => {
            match token_text(t, ctx.src) {
                "++" => true,
                "--" => false,
                _ => {
                    return Err(diag("UNEXPECTED_TOKEN", span, "expected `++` or `--`"));
                }
            }
        }
        Some([NodeOrToken::Token(t1), NodeOrToken::Token(t2)])
            if t1.kind() == LeekSyntaxKind::Operator && t2.kind() == LeekSyntaxKind::Operator =>
        {
            let a = token_text(t1, ctx.src);
            let b = token_text(t2, ctx.src);
            if a != b || (a != "+" && a != "-") {
                return Err(diag("UNEXPECTED_TOKEN", span, "expected `++` or `--`"));
            }
            a == "+"
        }
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                format!("malformed PostUpdateExpr: {} parts", parts.len()),
            ));
        }
    };
    let target = lower_expr_wrapped(inner_n, ctx)?;
    if !is_valid_assign_place(&target) {
        return Err(diag(
            "INVALID_ASSIGN_TARGET",
            span,
            "invalid target for `++` / `--`",
        ));
    }
    Ok(HirExpr::PostUpdate {
        target: Box::new(target),
        increment,
        span,
    })
}

fn lower_pre_update_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    let (increment, inner_n) = match parts.as_slice() {
        [NodeOrToken::Token(t), NodeOrToken::Node(x)] if x.kind() == LeekSyntaxKind::Expr => {
            let inc = match token_text(t, ctx.src) {
                "++" => true,
                "--" => false,
                _ => {
                    return Err(diag("UNEXPECTED_TOKEN", span, "expected `++` or `--`"));
                }
            };
            (inc, x)
        }
        [NodeOrToken::Token(t1), NodeOrToken::Token(t2), NodeOrToken::Node(x)]
            if x.kind() == LeekSyntaxKind::Expr
                && t1.kind() == LeekSyntaxKind::Operator
                && t2.kind() == LeekSyntaxKind::Operator =>
        {
            let a = token_text(t1, ctx.src);
            let b = token_text(t2, ctx.src);
            if a != b || (a != "+" && a != "-") {
                return Err(diag("UNEXPECTED_TOKEN", span, "expected `++` or `--`"));
            }
            (a == "+", x)
        }
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                format!("malformed PreUpdateExpr: {} parts", parts.len()),
            ));
        }
    };
    let target = lower_expr_wrapped(inner_n, ctx)?;
    if !is_valid_assign_place(&target) {
        return Err(diag(
            "INVALID_ASSIGN_TARGET",
            span,
            "invalid target for prefix `++` / `--`",
        ));
    }
    Ok(HirExpr::PreUpdate {
        target: Box::new(target),
        increment,
        span,
    })
}

fn lower_prefix_cast_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let mut it = non_trivia(n);
    let ty_n = match it.next() {
        Some(NodeOrToken::Node(ch)) if ch.kind() == LeekSyntaxKind::IdentExpr => ch,
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "malformed PrefixCastExpr: expected type",
            ));
        }
    };
    let ty_h = lower_ident_expr(&ty_n, ctx)?;
    let ty_name = match ty_h {
        HirExpr::Ident { name, .. } => HirTypeExpr::Named(name),
        _ => unreachable!(),
    };
    let expr_n = match it.next() {
        Some(NodeOrToken::Node(ch)) if is_expr_shape(ch.kind()) => ch,
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "malformed PrefixCastExpr: missing expr",
            ));
        }
    };
    let expr = lower_expr(&expr_n, ctx)?;
    if it.next().is_some() {
        return Err(diag(
            "INTERNAL_ERROR",
            span,
            "trailing tokens in PrefixCastExpr",
        ));
    }
    Ok(HirExpr::Cast {
        expr: Box::new(expr),
        ty: ty_name,
        span,
    })
}

fn lower_function_value_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    let mut i = 0usize;
    let legacy_function_ctor;
    match parts.get(i) {
        Some(NodeOrToken::Token(t))
            if t.kind() == LeekSyntaxKind::Kw
                && matches!(token_text(t, ctx.src), "function" | "Function" | "FUNCTION") =>
        {
            legacy_function_ctor = matches!(token_text(t, ctx.src), "Function" | "FUNCTION");
            i += 1;
        }
        Some(NodeOrToken::Token(t))
            if t.kind() == LeekSyntaxKind::Ident
                && matches!(token_text(t, ctx.src), "Function" | "FUNCTION") =>
        {
            legacy_function_ctor = true;
            i += 1;
        }
        // Java v1–v2 also accepts `Function(...) { ... }` as an anonymous function literal.
        Some(NodeOrToken::Node(ch)) if ch.kind() == LeekSyntaxKind::IdentExpr => {
            let h = lower_ident_expr(ch, ctx)?;
            match h {
                HirExpr::Ident { name, .. } if name == "Function" => {
                    legacy_function_ctor = true;
                    i += 1;
                }
                _ => {
                    return Err(diag(
                        "INTERNAL_ERROR",
                        span,
                        "FunctionValueExpr must start with `function`",
                    ));
                }
            }
        }
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "FunctionValueExpr must start with `function`",
            ));
        }
    }
    if legacy_function_ctor && ctx.language_version >= 3 {
        return Err(diag(
            "CANT_ADD_INSTRUCTION_AFTER_BREAK",
            span,
            "legacy `Function(...) {}` anonymous function literal is not allowed in v3+",
        ));
    }
    let paren_idx = match parts.get(i) {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenOpen => i,
        _ => {
            return Err(diag(
                "OPENING_PARENTHESIS_EXPECTED",
                span,
                "expected `(` after `function`",
            ));
        }
    };
    let (params, mut i) = lower_paren_param_list(&parts, paren_idx + 1, ctx, n)?;
    match parts.get(i) {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Arrow => {
            i += 1;
            while i < parts.len() {
                match &parts[i] {
                    NodeOrToken::Node(node) if node.kind() == LeekSyntaxKind::Block => break,
                    _ => i += 1,
                }
            }
        }
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::Block => {}
        _ => {
            return Err(diag(
                "UNEXPECTED_TOKEN",
                span,
                "expected `=>` or `{` after parameters",
            ));
        }
    }
    let body_n = match parts.get(i) {
        Some(NodeOrToken::Node(node)) if node.kind() == LeekSyntaxKind::Block => node,
        _ => {
            return Err(diag(
                "UNEXPECTED_TOKEN",
                span,
                "expected `{` body after `function` header",
            ));
        }
    };
    let body = lower_block_stmts(body_n, ctx)?;
    Ok(HirExpr::FunctionLiteral { params, body, span })
}

fn lower_arrow_fn_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    let mut i = 0usize;
    let mut param_nodes: Vec<SyntaxNode<LeekLanguage>> = Vec::new();

    let is_arrow_token = |t: &rowan::SyntaxToken<LeekLanguage>| {
        t.kind() == LeekSyntaxKind::Arrow || matches!(token_text(t, ctx.src), "->" | "=>")
    };

    match parts.get(i) {
        Some(NodeOrToken::Token(t)) if is_arrow_token(t) => {
            i += 1;
            let body_n = match parts.get(i) {
                Some(NodeOrToken::Node(ch))
                    if ch.kind() == LeekSyntaxKind::Block || is_expr_shape(ch.kind()) =>
                {
                    ch.clone()
                }
                _ => {
                    return Err(diag(
                        "INTERNAL_ERROR",
                        span,
                        "malformed ArrowFnExpr: missing body after leading `->`",
                    ));
                }
            };
            i += 1;
            if i < parts.len() {
                return Err(diag(
                    "INTERNAL_ERROR",
                    span,
                    "trailing tokens in ArrowFnExpr",
                ));
            }
            let params: Vec<HirParam> = Vec::new();
            return Ok(if body_n.kind() == LeekSyntaxKind::Block {
                HirExpr::FunctionLiteral {
                    params,
                    body: lower_block_stmts(&body_n, ctx)?,
                    span,
                }
            } else {
                let body = lower_expr(&body_n, ctx)?;
                HirExpr::ArrowClosure {
                    params,
                    body: Box::new(body),
                    span,
                }
            });
        }
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::ParenOpen => {
            i += 1;
            while i < parts.len() {
                match &parts[i] {
                    NodeOrToken::Node(ch) if ch.kind() == LeekSyntaxKind::IdentExpr => {
                        param_nodes.push(ch.clone());
                        i += 1;
                    }
                    NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Comma => i += 1,
                    NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenClose => {
                        i += 1;
                        break;
                    }
                    _ => {
                        return Err(diag(
                            "INTERNAL_ERROR",
                            span,
                            "malformed ArrowFnExpr: expected parameters or `)`",
                        ));
                    }
                }
            }
        }
        Some(NodeOrToken::Node(ch)) if ch.kind() == LeekSyntaxKind::IdentExpr => {
            param_nodes.push(ch.clone());
            i += 1;
            while i + 1 < parts.len() {
                match (&parts[i], &parts[i + 1]) {
                    (NodeOrToken::Token(t), NodeOrToken::Node(p))
                        if t.kind() == LeekSyntaxKind::Comma
                            && p.kind() == LeekSyntaxKind::IdentExpr =>
                    {
                        i += 2;
                        param_nodes.push(p.clone());
                    }
                    _ => break,
                }
            }
        }
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "malformed ArrowFnExpr: expected parameter",
            ));
        }
    }

    let _arrow = match parts.get(i) {
        Some(NodeOrToken::Token(t)) if is_arrow_token(t) => {
            i += 1;
            t
        }
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "malformed ArrowFnExpr: expected `=>`",
            ));
        }
    };
    let body_n = match parts.get(i) {
        Some(NodeOrToken::Node(ch))
            if ch.kind() == LeekSyntaxKind::Block || is_expr_shape(ch.kind()) =>
        {
            ch
        }
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "malformed ArrowFnExpr: missing body",
            ));
        }
    };
    i += 1;
    if i < parts.len() {
        return Err(diag(
            "INTERNAL_ERROR",
            span,
            "trailing tokens in ArrowFnExpr",
        ));
    }

    let mut params = Vec::with_capacity(param_nodes.len());
    for pn in param_nodes {
        let ph = lower_ident_expr(&pn, ctx)?;
        let p = match ph {
            HirExpr::Ident { name, span: ps } => HirParam {
                name: NameDef { name, span: ps },
                by_ref: false,
                decl_ty: None,
                default: None,
            },
            _ => unreachable!(),
        };
        params.push(p);
    }

    let hir = if body_n.kind() == LeekSyntaxKind::Block {
        HirExpr::FunctionLiteral {
            params,
            body: lower_block_stmts(&body_n, ctx)?,
            span,
        }
    } else {
        let body = lower_expr(&body_n, ctx)?;
        HirExpr::ArrowClosure {
            params,
            body: Box::new(body),
            span,
        }
    };

    Ok(hir)
}

fn lower_as_cast_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let mut it = non_trivia(n);

    let expr_n = match it.next() {
        Some(NodeOrToken::Node(ch)) if is_expr_shape(ch.kind()) => ch,
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "malformed AsCastExpr: missing expr",
            ));
        }
    };
    let expr = lower_expr(&expr_n, ctx)?;

    let as_tok = match it.next() {
        Some(NodeOrToken::Token(t))
            if t.kind() == LeekSyntaxKind::Kw && token_text(&t, ctx.src) == "as" =>
        {
            t
        }
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "malformed AsCastExpr: missing `as` keyword",
            ));
        }
    };
    let _ = as_tok; // span already covers entire node

    let mut type_toks: Vec<SyntaxToken<LeekLanguage>> = Vec::new();
    for p in it {
        if let NodeOrToken::Token(t) = p {
            type_toks.push(t);
        }
    }
    if type_toks.is_empty() {
        return Err(diag("TYPE_EXPECTED", span, "expected type after `as`"));
    }
    let ty =
        parse_type_expr(&type_toks, ctx.src).map_err(|msg| diag("TYPE_EXPECTED", span, msg))?;
    Ok(HirExpr::Cast {
        expr: Box::new(expr),
        ty,
        span,
    })
}

fn parse_type_expr(tokens: &[SyntaxToken<LeekLanguage>], src: &str) -> Result<HirTypeExpr, String> {
    struct Ts<'a> {
        toks: &'a [SyntaxToken<LeekLanguage>],
        i: usize,
        src: &'a str,
        /// Matches parser `type_gt_slack`: `>>` / `>>>` close multiple generic levels.
        angle_slack: u8,
    }
    impl<'a> Ts<'a> {
        fn peek(&self) -> Option<&'a SyntaxToken<LeekLanguage>> {
            self.toks.get(self.i)
        }
        fn bump(&mut self) -> Option<&'a SyntaxToken<LeekLanguage>> {
            let t = self.toks.get(self.i);
            if t.is_some() {
                self.i += 1;
            }
            t
        }
        fn at_end(&self) -> bool {
            self.i >= self.toks.len()
        }
        fn peek_text(&self, s: &str) -> bool {
            self.peek().is_some_and(|t| token_text(t, self.src) == s)
        }
        fn expect_text(&mut self, s: &str) -> Result<(), String> {
            if self.peek_text(s) {
                self.bump();
                Ok(())
            } else {
                Err(format!("expected `{s}` in type expression"))
            }
        }
        fn expect_ident_or_kw(&mut self) -> Result<String, String> {
            match self.bump() {
                Some(t) if matches!(t.kind(), LeekSyntaxKind::Ident | LeekSyntaxKind::Kw) => {
                    Ok(token_text(t, self.src).to_string())
                }
                _ => Err("expected type name".into()),
            }
        }
        fn expect_generic_gt(&mut self) -> Result<(), String> {
            if self.angle_slack > 0 {
                self.angle_slack -= 1;
                return Ok(());
            }
            match self.peek() {
                Some(t) if t.kind() == LeekSyntaxKind::Operator => {
                    let tx = token_text(t, self.src);
                    match tx {
                        ">" => {
                            self.bump();
                            Ok(())
                        }
                        ">>" => {
                            self.bump();
                            self.angle_slack = 1;
                            Ok(())
                        }
                        ">>>" => {
                            self.bump();
                            self.angle_slack = 2;
                            Ok(())
                        }
                        _ => Err("expected `>`".into()),
                    }
                }
                _ => Err("expected `>`".into()),
            }
        }
    }

    fn parse_union(ts: &mut Ts<'_>) -> Result<HirTypeExpr, String> {
        let mut tys = vec![parse_nullable(ts)?];
        while ts.peek_text("|") {
            ts.bump();
            tys.push(parse_nullable(ts)?);
        }
        if tys.len() == 1 {
            Ok(tys.remove(0))
        } else {
            Ok(HirTypeExpr::Union(tys))
        }
    }

    fn parse_nullable(ts: &mut Ts<'_>) -> Result<HirTypeExpr, String> {
        let mut ty = parse_primary(ts)?;
        if ts.peek_text("?") {
            ts.bump();
            ty = HirTypeExpr::Nullable(Box::new(ty));
        }
        Ok(ty)
    }

    fn parse_primary(ts: &mut Ts<'_>) -> Result<HirTypeExpr, String> {
        let base = ts.expect_ident_or_kw()?;
        if !ts.peek_text("<") {
            return Ok(HirTypeExpr::Named(base));
        }
        ts.expect_text("<")?;
        let mut args = Vec::new();
        loop {
            args.push(parse_union(ts)?);
            if ts.peek().is_some_and(|t| t.kind() == LeekSyntaxKind::Comma) {
                ts.bump();
                continue;
            }
            break;
        }
        let ret = if ts.peek().is_some_and(|t| t.kind() == LeekSyntaxKind::Arrow) {
            ts.bump();
            Some(Box::new(parse_union(ts)?))
        } else {
            None
        };
        ts.expect_generic_gt()?;
        Ok(HirTypeExpr::Generic { base, args, ret })
    }

    let mut ts = Ts {
        toks: tokens,
        i: 0,
        src,
        angle_slack: 0,
    };
    let out = parse_union(&mut ts)?;
    if !ts.at_end() {
        return Err("unexpected trailing tokens in type expression".into());
    }
    Ok(out)
}

pub(super) fn lower_array_literal(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let mut elements = Vec::new();
    for el in non_trivia(n) {
        if let NodeOrToken::Node(ch) = el {
            if is_expr_shape(ch.kind()) {
                elements.push(lower_expr(&ch, ctx)?);
            }
        }
    }
    if ctx.language_version < 4 {
        Ok(HirExpr::New {
            type_name: "LegacyLeekArrayList".into(),
            args: elements,
            span,
        })
    } else {
        Ok(HirExpr::ArrayLiteral { elements, span })
    }
}

pub(super) fn lower_map_literal(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let mut entries = Vec::new();
    let mut it = non_trivia(n).peekable();
    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BracketOpen => {}
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "map literal must start with `[`",
            ));
        }
    }
    loop {
        match it.peek() {
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BracketClose => break,
            None => {
                return Err(diag("UNCOMPLETE_EXPRESSION", span, "unclosed map literal"));
            }
            _ => {}
        }
        let key_n = match it.next() {
            Some(NodeOrToken::Node(ch)) if is_expr_shape(ch.kind()) => ch,
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span,
                    "expected map key expression",
                ));
            }
        };
        let colon_t = match it.next() {
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Operator => t,
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span,
                    "expected `:` after map key",
                ));
            }
        };
        if token_text(&colon_t, ctx.src) != ":" {
            return Err(diag(
                "INVALID_OPERATOR",
                span_of_range(colon_t.text_range()),
                "expected `:` between map key and value",
            ));
        }
        let val_n = match it.next() {
            Some(NodeOrToken::Node(ch)) if is_expr_shape(ch.kind()) => ch,
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span,
                    "expected map value expression",
                ));
            }
        };
        entries.push((lower_expr(&key_n, ctx)?, lower_expr(&val_n, ctx)?));
        if let Some(NodeOrToken::Token(c)) = it.peek() {
            if c.kind() == LeekSyntaxKind::Comma {
                it.next();
            }
        }
    }
    Ok(HirExpr::MapLiteral { entries, span })
}

fn lower_object_property_key(
    key_n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    match key_n.kind() {
        LeekSyntaxKind::IdentExpr => {
            let t = one_token_leaf(key_n)?;
            Ok(HirExpr::String(token_text(&t, ctx.src).to_string()))
        }
        LeekSyntaxKind::LiteralExpr => lower_literal(key_n, ctx),
        _ => Err(diag(
            "UNCOMPLETE_EXPRESSION",
            span_of_node(key_n),
            "expected identifier or literal as object property name",
        )),
    }
}

pub(super) fn lower_object_literal(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let mut entries = Vec::new();
    let mut it = non_trivia(n).peekable();
    match it.next() {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BraceOpen => {}
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "object literal must start with `{`",
            ));
        }
    }
    loop {
        match it.peek() {
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BraceClose => break,
            None => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span,
                    "unclosed object literal",
                ));
            }
            _ => {}
        }
        let key_n = match it.next() {
            Some(NodeOrToken::Node(ch))
                if ch.kind() == LeekSyntaxKind::IdentExpr
                    || ch.kind() == LeekSyntaxKind::LiteralExpr =>
            {
                ch
            }
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span,
                    "expected object property name",
                ));
            }
        };
        let colon_t = match it.next() {
            Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::Operator => t,
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span,
                    "expected `:` after property name",
                ));
            }
        };
        if token_text(&colon_t, ctx.src) != ":" {
            return Err(diag(
                "INVALID_OPERATOR",
                span_of_range(colon_t.text_range()),
                "expected `:` between property name and value",
            ));
        }
        let val_n = match it.next() {
            Some(NodeOrToken::Node(ch)) if is_expr_shape(ch.kind()) => ch,
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span,
                    "expected value expression",
                ));
            }
        };
        entries.push((
            lower_object_property_key(&key_n, ctx)?,
            lower_expr(&val_n, ctx)?,
        ));
        if let Some(NodeOrToken::Token(c)) = it.peek() {
            if c.kind() == LeekSyntaxKind::Comma {
                it.next();
            }
        }
    }
    Ok(HirExpr::ObjectLiteral { entries, span })
}

pub(super) fn lower_interval_literal(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    let mut i = 0usize;
    let min_closed = match parts.get(i) {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BracketOpen => {
            i += 1;
            true
        }
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BracketClose => {
            i += 1;
            false
        }
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "interval literal must start with `[` or `]`",
            ));
        }
    };
    let min_h = match parts.get(i) {
        Some(NodeOrToken::Node(ch)) if is_expr_shape(ch.kind()) => {
            let e = lower_expr(ch, ctx)?;
            i += 1;
            Some(e)
        }
        _ => None,
    };
    match parts.get(i) {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::DotDot => i += 1,
        _ => {
            return Err(diag(
                "INTERNAL_ERROR",
                span,
                "interval literal missing `..`",
            ));
        }
    }
    let max_h = match parts.get(i) {
        Some(NodeOrToken::Node(ch)) if is_expr_shape(ch.kind()) => {
            let e = lower_expr(ch, ctx)?;
            i += 1;
            Some(e)
        }
        _ => None,
    };
    let max_closed = match parts.get(i) {
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BracketClose => true,
        Some(NodeOrToken::Token(t)) if t.kind() == LeekSyntaxKind::BracketOpen => false,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected `]` or `[` after interval literal",
            ));
        }
    };
    let args = match (min_h, max_h) {
        (None, None) if max_closed => Vec::new(),
        (None, None) => vec![
            HirExpr::Bool(min_closed),
            HirExpr::Real(f64::NEG_INFINITY),
            HirExpr::Bool(false),
            HirExpr::Real(f64::INFINITY),
        ],
        (Some(a), None) => vec![
            HirExpr::Bool(min_closed),
            a,
            HirExpr::Bool(max_closed),
            HirExpr::Real(f64::INFINITY),
        ],
        (None, Some(b)) => vec![
            HirExpr::Bool(min_closed),
            HirExpr::Real(f64::NEG_INFINITY),
            HirExpr::Bool(max_closed),
            b,
        ],
        (Some(a), Some(b)) => vec![HirExpr::Bool(min_closed), a, HirExpr::Bool(max_closed), b],
    };
    Ok(HirExpr::New {
        type_name: "Interval".into(),
        args,
        span,
    })
}

pub(super) fn lower_set_literal(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let mut args = Vec::new();
    for el in non_trivia(n) {
        if let NodeOrToken::Node(ch) = el {
            if is_expr_shape(ch.kind()) {
                args.push(lower_expr(&ch, ctx)?);
            }
        }
    }
    Ok(HirExpr::New {
        type_name: "SetLiteral".into(),
        args,
        span,
    })
}

pub(super) fn lower_ternary_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 5 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed TernaryExpr: {} elements", parts.len()),
        ));
    }
    let cond_n = match &parts[0] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected condition expression",
            ));
        }
    };
    match &parts[1] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == "?" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected `?` in ternary",
            ));
        }
    }
    let then_n = match &parts[2] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected then-expression",
            ));
        }
    };
    match &parts[3] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == ":" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected `:` in ternary",
            ));
        }
    }
    let else_n = match &parts[4] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected else-expression",
            ));
        }
    };
    Ok(HirExpr::Ternary {
        cond: Box::new(lower_expr(cond_n, ctx)?),
        then_expr: Box::new(lower_expr(then_n, ctx)?),
        else_expr: Box::new(lower_expr(else_n, ctx)?),
        span,
    })
}

pub(super) fn lower_not_in_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 4 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed NotInExpr: {} elements", parts.len()),
        ));
    }
    let elem_n = match &parts[0] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected left operand of `not in`",
            ));
        }
    };
    match &parts[1] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "not" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `not`",
            ));
        }
    }
    match &parts[2] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "in" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected `in` after `not`",
            ));
        }
    }
    let container_n = match &parts[3] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected container expression",
            ));
        }
    };
    Ok(HirExpr::Binary {
        op: HirBinOp::NotIn,
        left: Box::new(lower_expr(elem_n, ctx)?),
        right: Box::new(lower_expr(container_n, ctx)?),
    })
}

pub(super) fn lower_unary_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 2 {
        return Err(diag(
            "INTERNAL_ERROR",
            span,
            format!("malformed UnaryExpr: {} elements", parts.len()),
        ));
    }
    let inner = match &parts[1] {
        NodeOrToken::Node(node) if is_expr_shape(node.kind()) => node,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected operand expression",
            ));
        }
    };
    match &parts[0] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Operator => {
            match token_text(t, ctx.src) {
                "@" => {
                    let expr = lower_expr(inner, ctx)?;
                    Ok(HirExpr::RefTo {
                        expr: Box::new(expr),
                        span,
                    })
                }
                "-" => Ok(HirExpr::Unary {
                    op: HirUnaryOp::Neg,
                    expr: Box::new(lower_expr(inner, ctx)?),
                }),
                "!" => Ok(HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(lower_expr(inner, ctx)?),
                }),
                "~" => Ok(HirExpr::Unary {
                    op: HirUnaryOp::BitNot,
                    expr: Box::new(lower_expr(inner, ctx)?),
                }),
                other => Err(diag(
                    "INVALID_OPERATOR",
                    span_of_range(t.text_range()),
                    format!("unsupported unary operator `{other}`"),
                )),
            }
        }
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && {
                let s = token_text(t, ctx.src);
                if ctx.language_version <= 2 {
                    s.eq_ignore_ascii_case("not")
                } else {
                    s == "not"
                }
            } =>
        {
            Ok(HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(lower_expr(inner, ctx)?),
            })
        }
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "typeof" =>
        {
            Ok(HirExpr::Unary {
                op: HirUnaryOp::Typeof,
                expr: Box::new(lower_expr(inner, ctx)?),
            })
        }
        _ => Err(diag(
            "INVALID_OPERATOR",
            span,
            "expected unary operator or `not`",
        )),
    }
}

pub(super) fn lower_new_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 2 {
        return Err(diag(
            "INTERNAL_ERROR",
            span,
            format!("malformed NewExpr: {} elements", parts.len()),
        ));
    }
    match &parts[0] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "new" => {}
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected `new` keyword",
            ));
        }
    }
    let type_node = match &parts[1] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::IdentExpr => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected type name after `new`",
            ));
        }
    };
    let type_name = match lower_ident_expr(type_node, ctx)? {
        HirExpr::Ident { name, .. } => name,
        _ => unreachable!("lower_ident_expr returns Ident"),
    };
    if parts.len() == 2 {
        return Ok(HirExpr::New {
            type_name,
            args: Vec::new(),
            span,
        });
    }
    if !matches!(
        &parts[2],
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenOpen
    ) {
        return Err(diag("OPENING_PARENTHESIS_EXPECTED", span, "expected `(`"));
    }
    let mut args = Vec::new();
    let mut i = 3usize;
    while i < parts.len() {
        match &parts[i] {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenClose => break,
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Comma => {
                i += 1;
            }
            NodeOrToken::Node(node) if is_expr_shape(node.kind()) => {
                args.push(lower_expr(node, ctx)?);
                i += 1;
            }
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span,
                    "expected argument or `)`",
                ));
            }
        }
    }
    Ok(HirExpr::New {
        type_name,
        args,
        span,
    })
}

pub(super) fn lower_index_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 4 {
        return Err(diag(
            "INTERNAL_ERROR",
            span,
            format!("malformed IndexExpr: {} elements", parts.len()),
        ));
    }
    let base_n = match &parts[0] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected base expression",
            ));
        }
    };
    if !matches!(&parts[1], NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::BracketOpen) {
        return Err(diag("UNCOMPLETE_EXPRESSION", span, "expected `[`"));
    }
    let idx_n = match &parts[2] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected index expression",
            ));
        }
    };
    if !matches!(&parts[3], NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::BracketClose) {
        return Err(diag("UNCOMPLETE_EXPRESSION", span, "expected `]`"));
    }
    let base = lower_expr(base_n, ctx)?;
    let index = lower_expr(idx_n, ctx)?;
    Ok(HirExpr::Index {
        base: Box::new(base),
        index: Box::new(index),
        span,
    })
}

pub(super) fn lower_array_slice_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    if ctx.language_version <= 3 {
        return Err(diag(
            "CLOSING_SQUARE_BRACKET_EXPECTED",
            span,
            "expected `]`",
        ));
    }
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 4 {
        return Err(diag(
            "INTERNAL_ERROR",
            span,
            format!("malformed ArraySliceExpr: {} elements", parts.len()),
        ));
    }
    let base_n = match &parts[0] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected base expression",
            ));
        }
    };
    if !matches!(&parts[1], NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::BracketOpen) {
        return Err(diag("UNCOMPLETE_EXPRESSION", span, "expected `[`"));
    }
    let mut i = 2usize;
    let start: Option<HirExpr> = match &parts[i] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == ":" =>
        {
            None
        }
        NodeOrToken::Node(sn) if is_expr_shape(sn.kind()) => {
            let s = lower_expr(sn, ctx)?;
            i += 1;
            Some(s)
        }
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected slice start or `:`",
            ));
        }
    };
    let colon_t = match &parts[i] {
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Operator => t,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected `:` in array slice",
            ));
        }
    };
    if token_text(colon_t, ctx.src) != ":" {
        return Err(diag(
            "INVALID_OPERATOR",
            span_of_range(colon_t.text_range()),
            "expected `:` in array slice",
        ));
    }
    i += 1;
    let (end, step) = if i < parts.len()
        && matches!(&parts[i], NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::BracketClose)
    {
        (None, None)
    } else if i < parts.len()
        && matches!(
            &parts[i],
            NodeOrToken::Token(t)
                if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == ":"
        )
    {
        i += 1;
        if i < parts.len()
            && matches!(&parts[i], NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::BracketClose)
        {
            // `[::]` / `[start::]` — default step.
            (None, None)
        } else {
            let st_n = match &parts[i] {
                NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
                _ => {
                    return Err(diag(
                        "UNCOMPLETE_EXPRESSION",
                        span,
                        "expected slice step expression",
                    ));
                }
            };
            let st = lower_expr(st_n, ctx)?;
            i += 1;
            (None, Some(st))
        }
    } else {
        let en = match &parts[i] {
            NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span,
                    "expected slice end or `]`",
                ));
            }
        };
        let e = lower_expr(en, ctx)?;
        i += 1;
        if i < parts.len()
            && matches!(
                &parts[i],
                NodeOrToken::Token(t)
                    if t.kind() == LeekSyntaxKind::Operator && token_text(t, ctx.src) == ":"
            )
        {
            i += 1;
            if i < parts.len()
                && matches!(&parts[i], NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::BracketClose)
            {
                (Some(e), None)
            } else {
                let st_n = match &parts[i] {
                    NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
                    _ => {
                        return Err(diag(
                            "UNCOMPLETE_EXPRESSION",
                            span,
                            "expected slice step expression",
                        ));
                    }
                };
                let st = lower_expr(st_n, ctx)?;
                i += 1;
                (Some(e), Some(st))
            }
        } else {
            (Some(e), None)
        }
    };
    if !matches!(&parts[i], NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::BracketClose) {
        return Err(diag("UNCOMPLETE_EXPRESSION", span, "expected `]`"));
    }
    let base = lower_expr(base_n, ctx)?;
    Ok(HirExpr::ArraySlice {
        base: Box::new(base),
        start: start.map(Box::new),
        end: end.map(Box::new),
        step: step.map(Box::new),
        span,
    })
}

pub(super) fn lower_member_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 3 {
        return Err(diag(
            "INTERNAL_ERROR",
            span,
            format!("malformed MemberExpr: {} elements", parts.len()),
        ));
    }
    let base_n = match &parts[0] {
        NodeOrToken::Node(x) if is_expr_shape(x.kind()) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span,
                "expected base expression",
            ));
        }
    };
    if !matches!(&parts[1], NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Dot) {
        return Err(diag("UNCOMPLETE_EXPRESSION", span, "expected `.`"));
    }
    let field: String = match &parts[2] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::IdentExpr => {
            let field_h = lower_ident_expr(x, ctx)?;
            match field_h {
                HirExpr::Ident { name, .. } => name,
                _ => unreachable!(),
            }
        }
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::LiteralExpr => {
            let t = one_token_leaf(x)?;
            if t.kind() == LeekSyntaxKind::Kw {
                token_text(&t, ctx.src).to_string()
            } else {
                return Err(diag("UNCOMPLETE_EXPRESSION", span, "expected field name"));
            }
        }
        _ => {
            return Err(diag("UNCOMPLETE_EXPRESSION", span, "expected field name"));
        }
    };
    let base = lower_expr(base_n, ctx)?;
    Ok(HirExpr::Member {
        base: Box::new(base),
        field,
        span,
    })
}

pub(super) fn lower_call_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let call_span = span_of_node(n);
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() < 3 {
        return Err(diag(
            "INTERNAL_ERROR",
            call_span,
            format!("malformed CallExpr: {} elements", parts.len()),
        ));
    }
    let callee_node = match &parts[0] {
        NodeOrToken::Node(x) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                call_span,
                "expected callee expression",
            ));
        }
    };
    let callee = lower_expr(callee_node, ctx)?;
    let mut span = call_span;
    span.start = span_start_of_first_non_trivia_token(callee_node);
    if !matches!(
        &parts[1],
        NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenOpen
    ) {
        return Err(diag(
            "OPENING_PARENTHESIS_EXPECTED",
            call_span,
            "expected `(`",
        ));
    }
    let mut args = Vec::new();
    let mut i = 2usize;
    while i < parts.len() {
        match &parts[i] {
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::ParenClose => break,
            NodeOrToken::Token(t) if t.kind() == LeekSyntaxKind::Comma => {
                i += 1;
            }
            NodeOrToken::Node(node) if is_expr_shape(node.kind()) => {
                args.push(lower_expr(node, ctx)?);
                i += 1;
            }
            _ => {
                return Err(diag(
                    "UNCOMPLETE_EXPRESSION",
                    span_of_node(n),
                    "expected argument or `)`",
                ));
            }
        }
    }
    Ok(HirExpr::Call {
        callee: Box::new(callee),
        args,
        span,
    })
}

pub(super) fn is_expr_shape(k: LeekSyntaxKind) -> bool {
    matches!(
        k,
        LeekSyntaxKind::Expr
            | LeekSyntaxKind::BinaryExpr
            | LeekSyntaxKind::UnaryExpr
            | LeekSyntaxKind::LiteralExpr
            | LeekSyntaxKind::IdentExpr
            | LeekSyntaxKind::ParenExpr
            | LeekSyntaxKind::CallExpr
            | LeekSyntaxKind::ArrayLiteralExpr
            | LeekSyntaxKind::MapLiteralExpr
            | LeekSyntaxKind::ObjectLiteralExpr
            | LeekSyntaxKind::IntervalLiteralExpr
            | LeekSyntaxKind::SetLiteralExpr
            | LeekSyntaxKind::NewExpr
            | LeekSyntaxKind::IndexExpr
            | LeekSyntaxKind::ArraySliceExpr
            | LeekSyntaxKind::MemberExpr
            | LeekSyntaxKind::TernaryExpr
            | LeekSyntaxKind::NotInExpr
            | LeekSyntaxKind::AsCastExpr
            | LeekSyntaxKind::PrefixCastExpr
            | LeekSyntaxKind::ArrowFnExpr
            | LeekSyntaxKind::FunctionValueExpr
            | LeekSyntaxKind::PostUpdateExpr
            | LeekSyntaxKind::PreUpdateExpr
            | LeekSyntaxKind::AssignExpr
    )
}

pub(super) fn lower_binary(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 3 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed BinaryExpr: {} elements", parts.len()),
        ));
    }
    let left_n = match &parts[0] {
        NodeOrToken::Node(x) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected left operand",
            ));
        }
    };
    let op_tok = match &parts[1] {
        NodeOrToken::Token(t)
            if t.kind() == LeekSyntaxKind::Operator
                || t.kind() == LeekSyntaxKind::WordOp
                || (t.kind() == LeekSyntaxKind::Kw && token_text(t, ctx.src) == "in") =>
        {
            t
        }
        _ => {
            return Err(diag(
                "INVALID_OPERATOR",
                span_of_node(n),
                "expected operator token",
            ));
        }
    };
    let right_n = match &parts[2] {
        NodeOrToken::Node(x) => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected right operand",
            ));
        }
    };
    let op = map_bin_op(token_text(op_tok, ctx.src))
        .map_err(|m| diag("INVALID_OPERATOR", span_of_range(op_tok.text_range()), m))?;
    let left = lower_expr(left_n, ctx)?;
    let right = lower_expr(right_n, ctx)?;
    Ok(HirExpr::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    })
}

pub(super) fn map_bin_op(s: &str) -> Result<HirBinOp, String> {
    // Some lexemes can span internal whitespace (e.g. `is not` is lexed as one operator token).
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match s.as_str() {
        "+" => Ok(HirBinOp::Add),
        "-" => Ok(HirBinOp::Sub),
        "*" => Ok(HirBinOp::Mul),
        "/" => Ok(HirBinOp::Div),
        "%" => Ok(HirBinOp::Rem),
        "==" => Ok(HirBinOp::Eq),
        "!=" => Ok(HirBinOp::Ne),
        "is" => Ok(HirBinOp::Eq),
        "is not" => Ok(HirBinOp::Ne),
        "===" => Ok(HirBinOp::StrictEq),
        "!==" => Ok(HirBinOp::StrictNe),
        "<" => Ok(HirBinOp::Lt),
        "<=" => Ok(HirBinOp::Le),
        ">" => Ok(HirBinOp::Gt),
        ">=" => Ok(HirBinOp::Ge),
        "&&" => Ok(HirBinOp::LogicalAnd),
        "||" => Ok(HirBinOp::LogicalOr),
        "and" => Ok(HirBinOp::LogicalAnd),
        "or" => Ok(HirBinOp::LogicalOr),
        "xor" | "^" => Ok(HirBinOp::BitXor),
        "instanceof" => Ok(HirBinOp::Instanceof),
        "in" => Ok(HirBinOp::In),
        "**" => Ok(HirBinOp::Pow),
        "\\" => Ok(HirBinOp::IntDiv),
        "??" => Ok(HirBinOp::NullishCoalesce),
        "&" => Ok(HirBinOp::BitAnd),
        "|" => Ok(HirBinOp::BitOr),
        "<<" => Ok(HirBinOp::Shl),
        ">>" => Ok(HirBinOp::Shr),
        ">>>" => Ok(HirBinOp::UShr),
        _ => Err(format!("unsupported binary operator `{s}`")),
    }
}

pub(super) fn one_token_leaf(
    n: &SyntaxNode<LeekLanguage>,
) -> Result<SyntaxToken<LeekLanguage>, HirLoweringDiagnostic> {
    let mut it = non_trivia(n);
    let t = match it.next() {
        Some(NodeOrToken::Token(t)) => t,
        Some(NodeOrToken::Node(_)) => {
            return Err(diag(
                "INTERNAL_ERROR",
                span_of_node(n),
                "expected token leaf",
            ));
        }
        None => {
            return Err(diag(
                "INTERNAL_ERROR",
                span_of_node(n),
                "missing token in leaf expression",
            ));
        }
    };
    if it.next().is_some() {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            "leaf expression must contain one token",
        ));
    }
    Ok(t)
}

fn parse_java_hex_float_lit(compact: &str) -> Option<f64> {
    let compact = compact.strip_prefix('-').unwrap_or(compact);
    let compact = compact.strip_prefix('+').unwrap_or(compact);
    let rest = compact
        .strip_prefix("0x")
        .or_else(|| compact.strip_prefix("0X"))?;
    let (mantissa, exp_part) = match rest.split_once(['p', 'P']) {
        Some((m, e)) => (m, e),
        None => (rest, "0"),
    };
    let exp: i32 = exp_part.parse().ok()?;
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((a, b)) => (a, b),
        None => (mantissa, ""),
    };
    let mut sig: f64 = 0.0;
    for c in int_part.chars() {
        let v = c.to_digit(16)? as f64;
        sig = sig * 16.0 + v;
    }
    let mut div = 1.0;
    for c in frac_part.chars() {
        div *= 16.0;
        sig += (c.to_digit(16)? as f64) / div;
    }
    Some(sig * 2.0_f64.powi(exp))
}

pub(super) fn lower_literal(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let t = one_token_leaf(n)?;
    let text = token_text(&t, ctx.src);
    match t.kind() {
        LeekSyntaxKind::Number => {
            if text.contains("__") {
                return Err(diag(
                    "MULTIPLE_NUMERIC_SEPARATORS",
                    span_of_range(t.text_range()),
                    "multiple adjacent `_` in numeric literal",
                ));
            }
            let compact: String = text.chars().filter(|&c| c != '_').collect();
            let mut hex_body = compact.as_str();
            let neg = if let Some(r) = hex_body.strip_prefix('-') {
                hex_body = r;
                true
            } else if let Some(r) = hex_body.strip_prefix('+') {
                hex_body = r;
                false
            } else {
                false
            };
            if let Some(rest) = hex_body
                .strip_prefix("0x")
                .or_else(|| hex_body.strip_prefix("0X"))
            {
                let use_float = rest.contains('.') || rest.contains('p') || rest.contains('P');
                if use_float {
                    let v = parse_java_hex_float_lit(&compact).ok_or_else(|| {
                        diag(
                            "INVALID_NUMBER",
                            span_of_range(t.text_range()),
                            "invalid hex float literal",
                        )
                    })?;
                    return Ok(HirExpr::Real(if neg { -v } else { v }));
                }
                return i64::from_str_radix(rest, 16)
                    .map(HirExpr::Integer)
                    .map_err(|_| {
                        diag(
                            "INVALID_NUMBER",
                            span_of_range(t.text_range()),
                            "invalid hex integer literal",
                        )
                    });
            }
            if let Some(rest) = compact
                .strip_prefix("0b")
                .or_else(|| compact.strip_prefix("0B"))
            {
                return i64::from_str_radix(rest, 2)
                    .map(HirExpr::Integer)
                    .map_err(|_| {
                        diag(
                            "INVALID_NUMBER",
                            span_of_range(t.text_range()),
                            "invalid binary integer literal",
                        )
                    });
            }
            let is_real = compact.contains('.') || compact.contains('e') || compact.contains('E');
            if is_real {
                compact.parse::<f64>().map(HirExpr::Real).map_err(|_| {
                    diag(
                        "INVALID_NUMBER",
                        span_of_range(t.text_range()),
                        "invalid number literal",
                    )
                })
            } else if let Ok(i) = compact.parse::<i64>() {
                Ok(HirExpr::Integer(i))
            } else {
                compact.parse::<f64>().map(HirExpr::Real).map_err(|_| {
                    diag(
                        "INVALID_NUMBER",
                        span_of_range(t.text_range()),
                        "invalid number literal",
                    )
                })
            }
        }
        LeekSyntaxKind::String => {
            let s = unquote_string(text)
                .map_err(|m| diag("VALUE_EXPECTED", span_of_range(t.text_range()), m))?;
            Ok(HirExpr::String(s))
        }
        LeekSyntaxKind::Kw => {
            let key = if ctx.language_version <= 2 {
                text.to_ascii_lowercase()
            } else {
                text.to_string()
            };
            match key.as_str() {
                "true" => Ok(HirExpr::Bool(true)),
                "false" => Ok(HirExpr::Bool(false)),
                "null" => Ok(HirExpr::Null),
                "this" => Ok(HirExpr::This),
                "class" => Ok(HirExpr::ClassSelf {
                    span: span_of_range(t.text_range()),
                }),
                "super" => Ok(HirExpr::Ident {
                    name: "super".to_string(),
                    span: span_of_range(t.text_range()),
                }),
                _ => Err(diag(
                    "KEYWORD_UNEXPECTED",
                    span_of_range(t.text_range()),
                    format!("unsupported keyword literal `{text}`"),
                )),
            }
        }
        LeekSyntaxKind::Lemniscate => Ok(HirExpr::Real(f64::INFINITY)),
        LeekSyntaxKind::Pi => Ok(HirExpr::Real(std::f64::consts::PI)),
        _ => Err(diag(
            "INVALID_NUMBER",
            span_of_range(t.text_range()),
            format!("unexpected token kind in literal: {:?}", t.kind()),
        )),
    }
}

pub(super) fn lower_ident_expr(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let t = one_token_leaf(n)?;
    if t.kind() != LeekSyntaxKind::Ident {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_range(t.text_range()),
            "IdentExpr must wrap an Ident token",
        ));
    }
    Ok(HirExpr::Ident {
        name: token_text(&t, ctx.src).to_string(),
        span: span_of_range(t.text_range()),
    })
}

pub(super) fn lower_paren(
    n: &SyntaxNode<LeekLanguage>,
    ctx: &LowerCtx,
) -> Result<HirExpr, HirLoweringDiagnostic> {
    let parts: Vec<_> = non_trivia(n).collect();
    if parts.len() != 3 {
        return Err(diag(
            "INTERNAL_ERROR",
            span_of_node(n),
            format!("malformed ParenExpr: {} elements", parts.len()),
        ));
    }
    let inner = match &parts[1] {
        NodeOrToken::Node(x) if x.kind() == LeekSyntaxKind::Expr => x,
        _ => {
            return Err(diag(
                "UNCOMPLETE_EXPRESSION",
                span_of_node(n),
                "expected Expr inside parentheses",
            ));
        }
    };
    lower_expr_wrapped(inner, ctx)
}
