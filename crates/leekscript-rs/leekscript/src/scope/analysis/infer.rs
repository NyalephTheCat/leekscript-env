//! Expression type inference helpers (phase 2).

use std::collections::HashMap;

use sipha::tree::ast::AstNode;
use sipha::tree::red::{SyntaxElement, SyntaxNode, SyntaxToken};

use crate::Span;
use crate::ast::types::TypeExpr;
use crate::syntax::kinds::{Lex, Node};

use crate::scope::extract::leek_ty_from_type_expr_with_templates;
use crate::scope::leek_ty::LeekTy;
use crate::scope::model::{
    ExprTypeKey, SemanticCode, SemanticDiagnostic, SemanticSeverity, Symbol, SymbolId, SymbolKind,
};
use crate::vm::host::java_ops;

use super::analyzer::Analyzer;

/// If the chain receiver was nullable (`T?`), the operation may not run — wrap the result once
/// in [`LeekTy::Nullable`] unless it is already nullable or unknown.
fn propagate_nullable_optional_chain(base_was_nullable: bool, ty: LeekTy) -> LeekTy {
    if !base_was_nullable {
        return ty;
    }
    match ty {
        LeekTy::Unknown | LeekTy::Nullable(_) => ty,
        other => LeekTy::Nullable(Box::new(other)),
    }
}

fn push_nullable_chain_warning(a: &mut Analyzer, span: Span, message: &str) {
    a.diagnostics.push(SemanticDiagnostic {
        severity: SemanticSeverity::Warning,
        code: SemanticCode::NullableChainAccess,
        message: message.to_string(),
        span,
        related_span: None,
    });
}

pub(crate) fn set_var_inferred_if_unannotated(a: &mut Analyzer, sid: SymbolId, ty: LeekTy) {
    let sym = &mut a.graph.symbols[sid.0 as usize];
    if sym.declared_ty.is_none() {
        sym.inferred_ty = Some(ty);
    }
}

pub(crate) fn binary_expr_is_instanceof(node: &SyntaxNode) -> bool {
    node.kind_as::<Node>() == Some(Node::BinaryExpr)
        && node
            .child_tokens()
            .any(|t| t.kind_as::<Lex>() == Some(Lex::InstanceofKw))
}

pub(crate) fn infer_binary(a: &mut Analyzer, node: &SyntaxNode) -> LeekTy {
    if binary_expr_is_instanceof(node) {
        return LeekTy::Boolean;
    }
    if node.child_tokens().any(|t| {
        matches!(
            t.kind_as::<Lex>(),
            Some(
                Lex::EqEq
                    | Lex::NotEq
                    | Lex::EqEqEq
                    | Lex::NotEqEq
                    | Lex::Lt
                    | Lex::Lte
                    | Lex::Gt
                    | Lex::Gte
            )
        )
    }) {
        return LeekTy::Boolean;
    }
    if node
        .child_tokens()
        .any(|t| matches!(t.kind_as::<Lex>(), Some(Lex::AndAnd | Lex::OrOr)))
    {
        return LeekTy::Boolean;
    }
    if node
        .child_tokens()
        .any(|t| matches!(t.kind_as::<Lex>(), Some(Lex::IsKw | Lex::InKw | Lex::XorKw)))
    {
        return LeekTy::Boolean;
    }

    let kids: Vec<_> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Node>() != Some(Node::Trivia))
        .collect();
    let (lk, rk) = if kids.len() >= 2 {
        (
            expr_span_ty(a, &kids[0]),
            expr_span_ty(a, &kids[kids.len() - 1]),
        )
    } else {
        let prefix = java_ops::prefix_before_first_binary_op(node);
        let suffix = java_ops::suffix_after_first_binary_op(node);
        let lk = if !prefix.is_empty() {
            ty_from_binary_operand_elements(a, &prefix)
        } else {
            binary_infix_lhs_ty(a, node).unwrap_or(LeekTy::Unknown)
        };
        let rk = ty_from_binary_operand_elements(a, &suffix);
        (lk, rk)
    };
    LeekTy::coerce_binary_op(&lk, &rk)
}

/// Immediate CST parent of `target` under `root` (only follows [`SyntaxNode::child_nodes`], not tokens).
fn syntax_immediate_parent(root: &SyntaxNode, target: &SyntaxNode) -> Option<SyntaxNode> {
    for c in root.child_nodes() {
        if c.offset() == target.offset()
            && c.kind() == target.kind()
            && c.text_len() == target.text_len()
        {
            return Some(root.clone());
        }
        if let Some(p) = syntax_immediate_parent(&c, target) {
            return Some(p);
        }
    }
    None
}

/// Left operand of a [`Node::BinaryExpr`] from sipha’s left-assoc infix shape: `[lhs, BinaryExpr(op rhs), …]`.
fn binary_infix_lhs_ty(a: &mut Analyzer, bin: &SyntaxNode) -> Option<LeekTy> {
    let root = a.syntax_node_stack.first()?;
    let parent = syntax_immediate_parent(root, bin)?;
    let ch: Vec<_> = parent
        .children()
        .filter(|el| !crate::syntax::syntax_el_is_trivia(el))
        .collect();
    let idx = ch.iter().position(|el| {
        matches!(
            el,
            SyntaxElement::Node(n) if n.offset() == bin.offset() && n.kind() == bin.kind()
        )
    })?;
    if idx == 0 {
        return None;
    }
    match &ch[idx - 1] {
        SyntaxElement::Node(n) => Some(expr_span_ty(a, n)),
        // Under [`Node::Expr`], the LHS is often a literal token (`1 + 2`) rather than a subtree node.
        SyntaxElement::Token(t) => {
            let ty = ty_from_binary_token_operand(a, t);
            (ty != LeekTy::Unknown).then_some(ty)
        }
    }
}

fn ty_from_binary_token_operand(a: &mut Analyzer, t: &SyntaxToken) -> LeekTy {
    if t.kind_as::<Lex>() == Some(Lex::Number) {
        let key = ExprTypeKey::from_span(t.text_range());
        if let Some(ty) = a.expr_types.get(&key).cloned() {
            if ty != LeekTy::Unknown {
                return ty;
            }
        }
        return LeekTy::from_number_literal_text(t.text());
    }
    if t.kind_as::<Lex>() == Some(Lex::Ident) {
        let key = ExprTypeKey::from_span(t.text_range());
        if let Some(sid) = a.resolve_here(t.text()) {
            if let Some(ty) = a.expr_types.get(&key).cloned() {
                if ty != LeekTy::Unknown {
                    return a.narrowing.with_narrowing(sid, ty);
                }
            }
            let sym = &a.graph.symbols[sid.0 as usize];
            let base = sym.effective_ty();
            return a.narrowing.with_narrowing(sid, base);
        }
    }
    let key = ExprTypeKey::from_span(t.text_range());
    a.expr_types.get(&key).cloned().unwrap_or(LeekTy::Unknown)
}

/// Operand slice before/after the operator inside a [`Node::BinaryExpr`], or a one-node RHS suffix.
fn ty_from_binary_operand_elements(a: &mut Analyzer, parts: &[SyntaxElement]) -> LeekTy {
    let parts: Vec<_> = parts
        .iter()
        .filter(|el| !crate::syntax::syntax_el_is_trivia(el))
        .collect();
    if parts.is_empty() {
        return LeekTy::Unknown;
    }
    if parts.len() == 1 {
        return match parts[0] {
            SyntaxElement::Node(n) => {
                let t = expr_span_ty(a, n);
                if t != LeekTy::Unknown {
                    return t;
                }
                // Primary / mult subtree may share the literal token’s span key; a parent insert can
                // overwrite [`ExprTypeKey`] with [`LeekTy::Unknown`]. Recover from the number token.
                n.descendant_semantic_tokens()
                    .into_iter()
                    .filter(|t| t.kind_as::<Lex>() == Some(Lex::Number))
                    .last()
                    .map(|t| ty_from_binary_token_operand(a, &t))
                    .unwrap_or(LeekTy::Unknown)
            }
            SyntaxElement::Token(t) => ty_from_binary_token_operand(a, t),
        };
    }
    if let SyntaxElement::Node(n) = parts[0] {
        let t = expr_span_ty(a, n);
        if t != LeekTy::Unknown {
            return t;
        }
    }
    for el in &parts {
        if let SyntaxElement::Node(n) = *el {
            let t = expr_span_ty(a, n);
            if t != LeekTy::Unknown {
                return t;
            }
        }
    }
    if let SyntaxElement::Token(t) = *parts.last().expect("non_empty") {
        return ty_from_binary_token_operand(a, t);
    }
    LeekTy::Unknown
}

pub(crate) fn infer_interval_ty(a: &mut Analyzer, node: &SyntaxNode) -> LeekTy {
    let mut acc = None::<LeekTy>;
    for tok in node.descendant_semantic_tokens() {
        if tok.kind_as::<Lex>() == Some(Lex::Number) {
            let nt = LeekTy::from_number_literal_text(tok.text());
            acc = Some(match acc {
                None => nt,
                Some(prev) => LeekTy::unify_binary_numeric(&prev, &nt),
            });
        }
    }
    if let Some(t) = acc {
        return LeekTy::Interval(Box::new(LeekTy::interval_inner(t)));
    }
    for n in node.descendant_nodes() {
        if n.kind_as::<Node>() != Some(Node::Expr) {
            continue;
        }
        let et = expr_span_ty(a, &n);
        if !LeekTy::is_interval_element(&et) {
            continue;
        }
        acc = Some(match acc {
            None => et,
            Some(prev) => LeekTy::unify_binary_numeric(&prev, &et),
        });
    }
    let inner = acc.map(LeekTy::interval_inner).unwrap_or(LeekTy::Unknown);
    LeekTy::Interval(Box::new(inner))
}

fn ty_from_semantic_tokens(a: &Analyzer, node: &SyntaxNode) -> LeekTy {
    if node
        .descendant_nodes()
        .any(|n| n.kind_as::<Node>() == Some(Node::BinaryExpr))
    {
        return LeekTy::Unknown;
    }
    // Prefer the rightmost typed leaf so `RegisterManager.get(x)` does not pick `RegisterManager`
    // (class) from the first ident token.
    let mut best: Option<(u32, LeekTy)> = None;
    for tok in node.descendant_semantic_tokens() {
        let key = ExprTypeKey::from_span(tok.text_range());
        if let Some(t) = a.expr_types.get(&key) {
            if *t != LeekTy::Unknown {
                let start = tok.text_range().start;
                best = Some(match best {
                    None => (start, t.clone()),
                    Some((s0, t0)) if start >= s0 => (start, t.clone()),
                    Some(prev) => prev,
                });
            }
        }
    }
    best.map(|(_, t)| t).unwrap_or(LeekTy::Unknown)
}

/// Receiver of `Foo.bar` is typed inside [`Node::MemberExpr`], so the `Foo` ident often has no
/// [`ExprTypeKey`] entry — resolve it from scopes when it is a lone identifier.
fn receiver_ty_from_simple_name(a: &Analyzer, recv: &SyntaxNode) -> Option<LeekTy> {
    let idents: Vec<_> = recv
        .descendant_semantic_tokens()
        .into_iter()
        .filter(|t| t.kind_as::<Lex>() == Some(Lex::Ident))
        .collect();
    if idents.len() != 1 {
        return None;
    }
    let name = idents[0].text();
    let sid = a.resolve_here(name)?;
    let sym = &a.graph.symbols[sid.0 as usize];
    let base = sym.effective_ty();
    Some(a.narrowing.with_narrowing(sid, base))
}

/// Right-hand name token of a `.field` / `.class` / `.super` member access (postfix chain).
pub(crate) fn member_expr_field_name(node: &SyntaxNode) -> Option<String> {
    node.descendant_tokens()
        .into_iter()
        .filter(|t| !t.is_trivia())
        .filter(|t| {
            matches!(
                t.kind_as::<Lex>(),
                Some(Lex::Ident | Lex::ClassKw | Lex::SuperKw)
            )
        })
        .max_by_key(|t| t.text_range().start)
        .and_then(|t| match t.kind_as::<Lex>() {
            Some(Lex::Ident) => Some(t.text().to_string()),
            Some(Lex::ClassKw) => Some("class".to_string()),
            Some(Lex::SuperKw) => Some("super".to_string()),
            _ => None,
        })
}

fn member_expr_field_span(node: &SyntaxNode) -> Option<Span> {
    node.descendant_tokens()
        .into_iter()
        .filter(|t| !t.is_trivia())
        .filter(|t| {
            matches!(
                t.kind_as::<Lex>(),
                Some(Lex::Ident | Lex::ClassKw | Lex::SuperKw)
            )
        })
        .max_by_key(|t| t.text_range().start)
        .map(|t| t.text_range())
}

fn class_member_matches_access(sym: &Symbol, want_static: bool) -> bool {
    if want_static {
        match sym.kind {
            SymbolKind::Constructor => true,
            SymbolKind::Field | SymbolKind::Method => sym.is_static,
            _ => false,
        }
    } else {
        !sym.is_static && matches!(sym.kind, SymbolKind::Field | SymbolKind::Method)
    }
}

/// [`SymbolId`] for `field` on `class_name` when it matches static vs instance access (same rules as
/// [`lookup_class_member_ty`]).
pub(crate) fn lookup_class_member_symbol(
    a: &Analyzer,
    class_name: &str,
    field: &str,
    want_static: bool,
) -> Option<SymbolId> {
    let &class_sc = a.graph.class_body_scope_by_name.get(class_name)?;
    let sc = a.graph.scopes.get(class_sc.0 as usize)?;
    let &sid = sc.symbols.get(field)?;
    let sym = &a.graph.symbols[sid.0 as usize];
    class_member_matches_access(sym, want_static).then_some(sid)
}

fn lookup_class_member_ty(
    a: &Analyzer,
    class_name: &str,
    field: &str,
    want_static: bool,
) -> LeekTy {
    let Some(sid) = lookup_class_member_symbol(a, class_name, field, want_static) else {
        return LeekTy::Unknown;
    };
    a.graph.symbols[sid.0 as usize].effective_ty()
}

/// Receiver type for `.field` (postfix operand before this [`Node::MemberExpr`]).
pub(crate) fn member_expr_receiver_ty(a: &mut Analyzer, node: &SyntaxNode) -> LeekTy {
    if let Some(obj) = node.child_nodes().next() {
        let mut t = expr_span_ty(a, &obj);
        if matches!(t, LeekTy::Unknown) {
            if let Some(r) = receiver_ty_from_simple_name(a, &obj) {
                t = r;
            }
        }
        t
    } else {
        postfix_suffix_operand_ty(a, node).unwrap_or(LeekTy::Unknown)
    }
}

/// Operand for a postfix suffix node (e.g. [`Node::MemberExpr`], postfix [`Node::UnaryExpr`] `!`) whose
/// CST stores only the suffix — the receiver is the previous non-trivia sibling under the same parent.
fn postfix_suffix_operand_ty(a: &mut Analyzer, suffix: &SyntaxNode) -> Option<LeekTy> {
    let parent = a.syntax_parent_of(suffix)?;
    let ch: Vec<_> = parent
        .children()
        .filter(|el| !crate::syntax::syntax_el_is_trivia(el))
        .collect();
    let idx = ch.iter().position(|el| {
        matches!(
            el,
            SyntaxElement::Node(n) if n.offset() == suffix.offset() && n.kind() == suffix.kind()
        )
    })?;
    if idx == 0 {
        return None;
    }
    match &ch[idx - 1] {
        SyntaxElement::Node(n) => Some(expr_span_ty(a, n)),
        SyntaxElement::Token(t) if t.kind_as::<Lex>() == Some(Lex::Ident) => {
            let key = ExprTypeKey::from_span(t.text_range());
            let sid = a.resolve_here(t.text())?;
            if let Some(ty) = a.expr_types.get(&key).cloned() {
                if ty != LeekTy::Unknown {
                    return Some(a.narrowing.with_narrowing(sid, ty));
                }
            }
            let sym = &a.graph.symbols[sid.0 as usize];
            let base = sym.effective_ty();
            Some(a.narrowing.with_narrowing(sid, base))
        }
        SyntaxElement::Token(t) if t.kind_as::<Lex>() == Some(Lex::ThisKw) => {
            a.implicit_this_ty()
        }
        _ => None,
    }
}

/// True if this [`Node::UnaryExpr`]’s first non-trivia child is a `!` token (prefix `!x`).
pub(crate) fn unary_expr_leading_bang_token(node: &SyntaxNode) -> bool {
    node.children()
        .find(|el| !crate::syntax::syntax_el_is_trivia(el))
        .is_some_and(|el| {
            el.as_token()
                .is_some_and(|t| t.kind_as::<Lex>() == Some(Lex::Bang))
        })
}

/// `ClassName.member` uses static members; `instance.member` uses instance fields/methods.
/// `x.class` → [`LeekTy::ClassObject`] for the runtime class of `x`; `x.super` → parent class object
/// when `class` … `extends` is present.
pub(crate) fn infer_member_expr(a: &mut Analyzer, node: &SyntaxNode) -> LeekTy {
    let obj_ty = member_expr_receiver_ty(a, node);
    let receiver_nullable = matches!(obj_ty, LeekTy::Nullable(_));
    let obj_ty_inner = match obj_ty {
        LeekTy::Nullable(inner) => (*inner).clone(),
        o => o,
    };
    let Some(field) = member_expr_field_name(node) else {
        return LeekTy::Unknown;
    };

    let member_ty = if field == "class" {
        match &obj_ty_inner {
            LeekTy::Class(cn) | LeekTy::ClassObject(cn) => LeekTy::ClassObject(cn.clone()),
            _ => LeekTy::Unknown,
        }
    } else if field == "super" {
        match &obj_ty_inner {
            LeekTy::Class(cn) => a
                .graph
                .class_extends
                .get(cn)
                .map(|p| LeekTy::ClassObject(p.clone()))
                .unwrap_or(LeekTy::Unknown),
            _ => LeekTy::Unknown,
        }
    } else {
        match obj_ty_inner {
            LeekTy::ClassObject(class_name) => lookup_class_member_ty(a, &class_name, &field, true),
            LeekTy::Class(class_name) => lookup_class_member_ty(a, &class_name, &field, false),
            _ => LeekTy::Unknown,
        }
    };

    let out = propagate_nullable_optional_chain(receiver_nullable, member_ty.clone());
    if receiver_nullable && member_ty != LeekTy::Unknown {
        let span = member_expr_field_span(node).unwrap_or_else(|| node.text_range());
        push_nullable_chain_warning(a, span, "member access on a value that may be null");
    }
    out
}

/// Resolve `name(`…`)` when the callee is not a direct sibling of [`Node::CallExpr`] (postfix suffix).
///
/// Scanning only inside [`Node::CallExpr`] hits `(` first and never sees `name`, so we walk the parse
/// root in source order and take the rightmost [`Lex::Ident`] strictly before this call's `(`.
fn callee_ty_from_tokens_before_call(a: &Analyzer, call: &SyntaxNode) -> LeekTy {
    let Some(root) = a.syntax_node_stack.first() else {
        return LeekTy::Unknown;
    };
    let cut = call
        .descendant_tokens()
        .into_iter()
        .find(|t| t.kind_as::<Lex>() == Some(Lex::LParen))
        .map(|t| t.text_range().start)
        .unwrap_or(call.text_range().start);
    let mut last_ident: Option<SyntaxToken> = None;
    for t in root.descendant_semantic_tokens() {
        if t.text_range().start >= cut {
            break;
        }
        if t.kind_as::<Lex>() == Some(Lex::Ident) {
            last_ident = Some(t.clone());
        }
    }
    let Some(tok) = last_ident else {
        return LeekTy::Unknown;
    };
    let key = ExprTypeKey::from_span(tok.text_range());
    if let Some(ty) = a.expr_types.get(&key) {
        if *ty != LeekTy::Unknown {
            return ty.clone();
        }
    }
    let name = tok.text();
    if let Some(sid) = a.resolve_here(name) {
        let sym = &a.graph.symbols[sid.0 as usize];
        if sym.kind == SymbolKind::Function {
            return sym.effective_ty();
        }
    }
    LeekTy::Unknown
}

fn call_expr_arg_types(a: &mut Analyzer, call: &SyntaxNode) -> Vec<LeekTy> {
    call.child_nodes()
        .filter(|n| n.kind_as::<Node>() != Some(Node::Trivia))
        .filter(|n| n.kind_as::<Node>() == Some(Node::Expr))
        .map(|n| expr_span_ty(a, &n))
        .collect()
}

fn ty_apply_type_param_subst(ty: &LeekTy, subst: &HashMap<String, LeekTy>) -> LeekTy {
    match ty {
        LeekTy::TypeParam(name) => subst.get(name).cloned().unwrap_or(LeekTy::TypeParam(name.clone())),
        LeekTy::Array(el) => LeekTy::Array(Box::new(ty_apply_type_param_subst(el, subst))),
        LeekTy::Set(el) => LeekTy::Set(Box::new(ty_apply_type_param_subst(el, subst))),
        LeekTy::Map(k, v) => LeekTy::Map(
            Box::new(ty_apply_type_param_subst(k, subst)),
            Box::new(ty_apply_type_param_subst(v, subst)),
        ),
        LeekTy::Interval(inner) => {
            LeekTy::Interval(Box::new(ty_apply_type_param_subst(inner, subst)))
        }
        LeekTy::Nullable(inner) => {
            LeekTy::Nullable(Box::new(ty_apply_type_param_subst(inner, subst)))
        }
        LeekTy::Union(parts) => LeekTy::Union(parts.iter().map(|p| ty_apply_type_param_subst(p, subst)).collect()),
        LeekTy::Function { params, ret } => LeekTy::Function {
            params: params.iter().map(|p| ty_apply_type_param_subst(p, subst)).collect(),
            ret: Box::new(ty_apply_type_param_subst(ret, subst)),
        },
        other => other.clone(),
    }
}

fn infer_call_type_param_subst_from_arg(
    expected: &LeekTy,
    actual: &LeekTy,
    out: &mut HashMap<String, LeekTy>,
) {
    match expected {
        LeekTy::TypeParam(name) => {
            match out.get(name) {
                None => {
                    out.insert(name.clone(), actual.clone());
                }
                Some(prev) => {
                    let joined = LeekTy::unify_inference(prev, actual);
                    out.insert(name.clone(), joined);
                }
            }
        }
        LeekTy::Nullable(inner) => {
            if matches!(actual, LeekTy::Null) {
                return;
            }
            if let LeekTy::Nullable(a_inner) = actual {
                infer_call_type_param_subst_from_arg(inner, a_inner, out);
            } else {
                infer_call_type_param_subst_from_arg(inner, actual, out);
            }
        }
        LeekTy::Array(e_el) => {
            if let LeekTy::Array(a_el) = actual {
                infer_call_type_param_subst_from_arg(e_el, a_el, out);
            }
        }
        LeekTy::Set(e_el) => {
            if let LeekTy::Set(a_el) = actual {
                infer_call_type_param_subst_from_arg(e_el, a_el, out);
            }
        }
        LeekTy::Map(e_k, e_v) => {
            if let LeekTy::Map(a_k, a_v) = actual {
                infer_call_type_param_subst_from_arg(e_k, a_k, out);
                infer_call_type_param_subst_from_arg(e_v, a_v, out);
            }
        }
        LeekTy::Interval(e_inner) => {
            if let LeekTy::Interval(a_inner) = actual {
                infer_call_type_param_subst_from_arg(e_inner, a_inner, out);
            }
        }
        LeekTy::Union(parts) => {
            // Best-effort: pick the first branch the actual can flow to.
            for p in parts {
                if LeekTy::is_assignable_to(actual, p) {
                    infer_call_type_param_subst_from_arg(p, actual, out);
                    break;
                }
            }
        }
        LeekTy::Function {
            params: e_params,
            ret: e_ret,
        } => {
            if let LeekTy::Function {
                params: a_params,
                ret: a_ret,
            } = actual
            {
                for (ep, ap) in e_params.iter().zip(a_params.iter()) {
                    infer_call_type_param_subst_from_arg(ep, ap, out);
                }
                infer_call_type_param_subst_from_arg(e_ret, a_ret, out);
            }
        }
        _ => {}
    }
}

/// Callee is usually the element before this [`Node::CallExpr`]; if the call is the first child of
/// [`Node::Expr`], recover the function name from the last identifier before `(`.
pub(crate) fn infer_call_expr(a: &mut Analyzer, call: &SyntaxNode) -> LeekTy {
    let Some(parent) = a.syntax_parent_of(call) else {
        return LeekTy::Unknown;
    };
    let children: Vec<SyntaxElement> = parent.children().collect();
    let Some(idx) = children.iter().position(|el| {
        matches!(
            el,
            SyntaxElement::Node(n)
                if n.offset() == call.offset()
                    && n.kind() == call.kind()
                    && n.text_len() == call.text_len()
        )
    }) else {
        return LeekTy::Unknown;
    };
    let callee_node = if idx > 0 {
        match &children[idx - 1] {
            SyntaxElement::Node(n) => Some(n.clone()),
            _ => None,
        }
    } else {
        None
    };
    let mut callee_ty = if idx > 0 {
        match &children[idx - 1] {
            SyntaxElement::Node(n) => expr_span_ty(a, n),
            SyntaxElement::Token(t) if t.kind_as::<Lex>() == Some(Lex::Ident) => {
                let key = ExprTypeKey::from_span(t.text_range());
                a.expr_types.get(&key).cloned().unwrap_or(LeekTy::Unknown)
            }
            _ => LeekTy::Unknown,
        }
    } else {
        LeekTy::Unknown
    };
    if matches!(callee_ty, LeekTy::Unknown) {
        callee_ty = callee_ty_from_tokens_before_call(a, call);
    }
    let callee_nullable = matches!(callee_ty, LeekTy::Nullable(_));
    let callee_inner = match callee_ty {
        LeekTy::Nullable(inner) => (*inner).clone(),
        o => o,
    };
    match callee_inner {
        LeekTy::Function { params, ret } => {
            let arg_tys = call_expr_arg_types(a, call);
            let mut subst: HashMap<String, LeekTy> = HashMap::new();
            for (expected, actual) in params.iter().zip(arg_tys.iter()) {
                infer_call_type_param_subst_from_arg(expected, actual, &mut subst);
            }
            let ret_ty = ty_apply_type_param_subst(ret.as_ref(), &subst);
            let out = propagate_nullable_optional_chain(callee_nullable, ret_ty.clone());
            if callee_nullable && ret_ty != LeekTy::Unknown {
                push_nullable_chain_warning(
                    a,
                    call.text_range(),
                    "call on a value that may be null",
                );
            }
            out
        }
        LeekTy::Union(parts) => {
            // If the callee is a union of function types, join possible return types.
            let mut outs: Vec<LeekTy> = Vec::new();
            for p in parts {
                if let LeekTy::Function { params, ret } = p {
                    let arg_tys = call_expr_arg_types(a, call);
                    let mut subst: HashMap<String, LeekTy> = HashMap::new();
                    for (expected, actual) in params.iter().zip(arg_tys.iter()) {
                        infer_call_type_param_subst_from_arg(expected, actual, &mut subst);
                    }
                    outs.push(ty_apply_type_param_subst(ret.as_ref(), &subst));
                }
            }
            if outs.is_empty() {
                LeekTy::Unknown
            } else if outs.len() == 1 {
                propagate_nullable_optional_chain(callee_nullable, outs[0].clone())
            } else {
                propagate_nullable_optional_chain(callee_nullable, LeekTy::Union(outs))
            }
        }
        LeekTy::ClassObject(cn) => {
            if let Some(n) = &callee_node {
                if n.kind_as::<Node>() == Some(Node::MemberExpr)
                    && member_expr_field_name(n).as_deref() == Some("super")
                {
                    return LeekTy::Void;
                }
            }
            let inst = LeekTy::Class(cn);
            let out = propagate_nullable_optional_chain(callee_nullable, inst.clone());
            if callee_nullable {
                push_nullable_chain_warning(
                    a,
                    call.text_range(),
                    "call on a value that may be null",
                );
            }
            out
        }
        _ => LeekTy::Unknown,
    }
}

/// `cond ? a : b` — value type from `a` and `b` (not the condition).
pub(crate) fn infer_ternary_expr(a: &mut Analyzer, node: &SyntaxNode) -> LeekTy {
    let branches: Vec<_> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Node>() == Some(Node::Expr))
        .collect();
    if branches.len() != 2 {
        return LeekTy::Unknown;
    }
    let t = expr_span_ty(a, &branches[0]);
    let e = expr_span_ty(a, &branches[1]);
    LeekTy::ternary_inference(&t, &e)
}

fn set_literal_element_exprs(node: &SyntaxNode) -> Vec<SyntaxNode> {
    node.child_nodes()
        .filter(|n| n.kind_as::<Node>() != Some(Node::Trivia))
        .filter(|n| n.kind_as::<Node>() == Some(Node::Expr))
        .collect()
}

/// `[]` that infers as `Array<?>` (not `[:]`, not an interval literal, no elements).
pub(crate) fn is_untyped_empty_array_literal(node: &SyntaxNode) -> bool {
    if node.kind_as::<Node>() != Some(Node::ArrayExpr) {
        return false;
    }
    if !array_literal_element_exprs(node).is_empty() {
        return false;
    }
    let kids: Vec<_> = node
        .child_nodes()
        .filter(|c| c.kind_as::<Node>() != Some(Node::Trivia))
        .collect();
    let is_bracket_map_shape = kids
        .iter()
        .any(|c| c.kind_as::<Node>() == Some(Node::BracketMapExpr))
        || (kids.len() == 2
            && kids[0].kind_as::<Node>() == Some(Node::Expr)
            && kids[1].kind_as::<Node>() == Some(Node::BracketMapExpr));
    if is_bracket_map_shape {
        return false;
    }
    if node
        .descendant_nodes()
        .any(|n| n.kind_as::<Node>() == Some(Node::IntervalExpr))
    {
        return false;
    }
    let toks: Vec<_> = node
        .descendant_tokens()
        .into_iter()
        .filter(|t| !t.is_trivia())
        .collect();
    !toks.windows(3).any(|w| {
        w[0].kind_as::<Lex>() == Some(Lex::LBracket)
            && w[1].kind_as::<Lex>() == Some(Lex::Colon)
            && w[2].kind_as::<Lex>() == Some(Lex::RBracket)
    })
}

/// `<>` — empty set literal.
pub(crate) fn is_untyped_empty_set_literal(node: &SyntaxNode) -> bool {
    node.kind_as::<Node>() == Some(Node::SetExpr) && set_literal_element_exprs(node).is_empty()
}

/// When a variable is annotated with `Array<T>` / `Set<T>` (or nullable thereof) and the RHS is an
/// empty literal typed as `Array<?>` / `Set<?>`, refine hovers to the declared type.
pub(crate) fn typed_empty_collection_refinement(declared: &LeekTy, rhs: &LeekTy) -> Option<LeekTy> {
    match (declared, rhs) {
        (LeekTy::Array(_), LeekTy::Array(v)) if **v == LeekTy::Unknown => Some(declared.clone()),
        (LeekTy::Set(_), LeekTy::Set(v)) if **v == LeekTy::Unknown => Some(declared.clone()),
        (LeekTy::Nullable(inner), LeekTy::Array(v))
            if **v == LeekTy::Unknown && matches!(&**inner, LeekTy::Array(_)) =>
        {
            Some(declared.clone())
        }
        (LeekTy::Nullable(inner), LeekTy::Set(v))
            if **v == LeekTy::Unknown && matches!(&**inner, LeekTy::Set(_)) =>
        {
            Some(declared.clone())
        }
        _ => None,
    }
}

fn array_literal_element_exprs(node: &SyntaxNode) -> Vec<SyntaxNode> {
    let direct: Vec<_> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Node>() != Some(Node::Trivia))
        .filter(|n| n.kind_as::<Node>() == Some(Node::Expr))
        .collect();
    if !direct.is_empty() {
        return direct;
    }
    node.child_nodes()
        .filter(|n| n.kind_as::<Node>() != Some(Node::Trivia))
        .flat_map(|n| {
            n.child_nodes()
                .filter(|c| c.kind_as::<Node>() != Some(Node::Trivia))
                .filter(|c| c.kind_as::<Node>() == Some(Node::Expr))
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(crate) fn infer_array_expr(a: &mut Analyzer, node: &SyntaxNode) -> LeekTy {
    let kids: Vec<_> = node
        .child_nodes()
        .filter(|c| c.kind_as::<Node>() != Some(Node::Trivia))
        .collect();
    // `[:]` — empty map literal wrapped in [`Node::ArrayExpr`].
    if kids.len() == 1 && kids[0].kind_as::<Node>() == Some(Node::BracketMapExpr) {
        return infer_bracket_map_expr(a, &kids[0]);
    }
    // `[key: val, …]` — key is a sibling [`Node::Expr`] before [`Node::BracketMapExpr`] (see grammar).
    if kids.len() == 2
        && kids[0].kind_as::<Node>() == Some(Node::Expr)
        && kids[1].kind_as::<Node>() == Some(Node::BracketMapExpr)
    {
        let k_ty = expr_span_ty(a, &kids[0]);
        let inner_exprs: Vec<_> = kids[1]
            .child_nodes()
            .filter(|n| n.kind_as::<Node>() != Some(Node::Trivia))
            .filter(|n| n.kind_as::<Node>() == Some(Node::Expr))
            .collect();
        if inner_exprs.is_empty() {
            return LeekTy::Map(Box::new(k_ty), Box::new(LeekTy::Unknown));
        }
        let mut kt = k_ty;
        let mut vt = expr_span_ty(a, &inner_exprs[0]);
        for chunk in inner_exprs[1..].chunks(2) {
            if chunk.len() == 2 {
                kt = LeekTy::unify_inference(&kt, &expr_span_ty(a, &chunk[0]));
                vt = LeekTy::unify_inference(&vt, &expr_span_ty(a, &chunk[1]));
            }
        }
        return LeekTy::Map(Box::new(kt), Box::new(vt));
    }

    // `[1..2]`, `]1..2[`, … — interval literal uses [`Node::IntervalExpr`] under [`Node::ArrayExpr`]
    // (not a one- or two-element `Array` of integers). Skip when this is clearly a map literal.
    let is_bracket_map_shape = kids
        .iter()
        .any(|c| c.kind_as::<Node>() == Some(Node::BracketMapExpr))
        || (kids.len() == 2
            && kids[0].kind_as::<Node>() == Some(Node::Expr)
            && kids[1].kind_as::<Node>() == Some(Node::BracketMapExpr));
    if !is_bracket_map_shape {
        if let Some(iv) = node
            .descendant_nodes()
            .find(|n| n.kind_as::<Node>() == Some(Node::IntervalExpr))
        {
            return infer_interval_ty(a, &iv);
        }
    }

    let exprs = array_literal_element_exprs(node);
    if exprs.is_empty() {
        // `[:]` must never be inferred as an empty array; only check when there are no element
        // exprs so `[a, [:]]` does not match a nested empty map's tokens.
        let toks: Vec<_> = node
            .descendant_tokens()
            .into_iter()
            .filter(|t| !t.is_trivia())
            .collect();
        if toks.windows(3).any(|w| {
            w[0].kind_as::<Lex>() == Some(Lex::LBracket)
                && w[1].kind_as::<Lex>() == Some(Lex::Colon)
                && w[2].kind_as::<Lex>() == Some(Lex::RBracket)
        }) {
            return LeekTy::Map(Box::new(LeekTy::Unknown), Box::new(LeekTy::Unknown));
        }
        return LeekTy::Array(Box::new(LeekTy::Unknown));
    }
    let mut acc = expr_span_ty(a, &exprs[0]);
    for e in exprs.iter().skip(1) {
        acc = LeekTy::unify_inference(&acc, &expr_span_ty(a, e));
    }
    LeekTy::Array(Box::new(acc))
}

pub(crate) fn infer_set_expr(a: &mut Analyzer, node: &SyntaxNode) -> LeekTy {
    let exprs = set_literal_element_exprs(node);
    if exprs.is_empty() {
        return LeekTy::Set(Box::new(LeekTy::Unknown));
    }
    let mut acc = expr_span_ty(a, &exprs[0]);
    for e in exprs.iter().skip(1) {
        acc = LeekTy::unify_inference(&acc, &expr_span_ty(a, e));
    }
    LeekTy::Set(Box::new(acc))
}

pub(crate) fn infer_bracket_map_expr(a: &mut Analyzer, node: &SyntaxNode) -> LeekTy {
    let exprs: Vec<_> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Node>() != Some(Node::Trivia))
        .filter(|n| n.kind_as::<Node>() == Some(Node::Expr))
        .collect();
    if exprs.is_empty() {
        return LeekTy::Map(Box::new(LeekTy::Unknown), Box::new(LeekTy::Unknown));
    }
    // Grammar: `BracketMapExpr` is `expr` (value for the key outside `[` … `:`) then
    // `(comma expr colon expr)*` for more `key: value` pairs — not alternating k,v throughout.
    let mut kt = LeekTy::Unknown;
    let mut vt = expr_span_ty(a, &exprs[0]);
    let mut i = 1;
    while i + 1 < exprs.len() {
        kt = LeekTy::unify_inference(&kt, &expr_span_ty(a, &exprs[i]));
        vt = LeekTy::unify_inference(&vt, &expr_span_ty(a, &exprs[i + 1]));
        i += 2;
    }
    LeekTy::Map(Box::new(kt), Box::new(vt))
}

pub(crate) fn infer_cast_expr(a: &Analyzer, node: &SyntaxNode) -> LeekTy {
    let mut templates: Vec<String> = Vec::new();
    for c in &a.class_template_stack {
        templates.extend(c.iter().cloned());
    }
    for f in &a.fn_template_stack {
        templates.extend(f.iter().cloned());
    }
    for ch in node.child_nodes() {
        if let Some(te) = TypeExpr::cast(ch.clone()) {
            return leek_ty_from_type_expr_with_templates(&te, &templates);
        }
    }
    LeekTy::Unknown
}

/// `base[index]` for arrays and maps.
///
/// Like [`Node::MemberExpr`] / [`Node::CallExpr`], [`Node::IndexExpr`] is usually a postfix suffix: the
/// receiver is the previous non-trivia sibling under the same parent, not a child of this node
/// (children are the `[` … `]` interior).
pub(crate) fn infer_index_expr(a: &mut Analyzer, node: &SyntaxNode) -> LeekTy {
    let mut base_ty_full = postfix_suffix_operand_ty(a, node).unwrap_or(LeekTy::Unknown);
    if matches!(base_ty_full, LeekTy::Unknown) {
        if let Some(base) = node.child_nodes().next() {
            base_ty_full = expr_span_ty(a, &base);
        }
    }
    let receiver_nullable = matches!(base_ty_full, LeekTy::Nullable(_));
    let base_ty = match base_ty_full {
        LeekTy::Nullable(inner) => (*inner).clone(),
        o => o,
    };
    let inner_ty = match base_ty {
        LeekTy::Array(el) | LeekTy::Set(el) => (*el).clone(),
        LeekTy::Map(_, v) => LeekTy::Nullable(Box::new((*v).clone())),
        _ => LeekTy::Unknown,
    };
    let out = propagate_nullable_optional_chain(receiver_nullable, inner_ty.clone());
    if receiver_nullable && inner_ty != LeekTy::Unknown {
        push_nullable_chain_warning(a, node.text_range(), "indexing a value that may be null");
    }
    out
}

pub(crate) fn expr_span_ty(a: &mut Analyzer, node: &SyntaxNode) -> LeekTy {
    let r = node.text_range();
    let key = ExprTypeKey::from_span(r);
    if let Some(t) = a.expr_types.get(&key) {
        if *t != LeekTy::Unknown {
            return t.clone();
        }
    }

    match node.kind_as::<Node>() {
        Some(Node::BinaryExpr) => infer_binary(a, node),
        Some(Node::IntervalExpr) => infer_interval_ty(a, node),
        Some(Node::MemberExpr) => infer_member_expr(a, node),
        Some(Node::IndexExpr) => infer_index_expr(a, node),
        Some(Node::CallExpr) => infer_call_expr(a, node),
        Some(Node::TernaryExpr) => infer_ternary_expr(a, node),
        Some(Node::ArrayExpr) => infer_array_expr(a, node),
        Some(Node::SetExpr) => infer_set_expr(a, node),
        Some(Node::BracketMapExpr) => infer_bracket_map_expr(a, node),
        Some(Node::CastExpr) => infer_cast_expr(a, node),
        Some(Node::Expr) => {
            // `assign` / `ternary`: condition and `? … : …` are siblings — do not stop at the condition.
            if let Some(tn) = node
                .child_nodes()
                .find(|c| c.kind_as::<Node>() == Some(Node::TernaryExpr))
            {
                return infer_ternary_expr(a, &tn);
            }
            // Postfix call `callee(args)`: callee is often a token / non-CallExpr node, so the first
            // `child_nodes()` entry may be the argument expression — still use the call's type.
            if let Some(call) = node
                .child_nodes()
                .find(|c| c.kind_as::<Node>() == Some(Node::CallExpr))
            {
                let t = expr_span_ty(a, &call);
                if t != LeekTy::Unknown {
                    return t;
                }
            }
            // Skip leading `Node::Trivia` nodes (grouped whitespace) so ` C.Z` is not mis-typed from
            // the trivia wrapper alone, falling through to `ty_from_semantic_tokens` (rightmost `C`).
            for ch in node
                .child_nodes()
                .filter(|c| c.kind_as::<Node>() != Some(Node::Trivia))
            {
                let t = expr_span_ty(a, &ch);
                if t != LeekTy::Unknown {
                    return t;
                }
            }
            ty_from_semantic_tokens(a, node)
        }
        Some(Node::UnaryExpr) => {
            let non_trivia_nodes: Vec<_> = node
                .child_nodes()
                .filter(|c| c.kind_as::<Node>() != Some(Node::Trivia))
                .collect();
            // Postfix `x!` — suffix [`Node::UnaryExpr`] holds only `!` (operand is a sibling).
            if non_trivia_nodes.is_empty() {
                let recv = postfix_suffix_operand_ty(a, node).unwrap_or(LeekTy::Unknown);
                return recv.non_null_variant().unwrap_or(recv);
            }
            // Prefix `!x` — boolean negation.
            if unary_expr_leading_bang_token(node) {
                return LeekTy::Boolean;
            }
            non_trivia_nodes
                .last()
                .map(|c| expr_span_ty(a, c))
                .unwrap_or(LeekTy::Unknown)
        }
        Some(Node::ParenExpr) => node
            .child_nodes()
            .find(|c| c.kind_as::<Node>() == Some(Node::Expr))
            .map(|c| expr_span_ty(a, &c))
            .unwrap_or(LeekTy::Unknown),
        _ => {
            if let Some(ch) = node.child_nodes().next() {
                let t = expr_span_ty(a, &ch);
                if t != LeekTy::Unknown {
                    return t;
                }
            }
            for ch in node.child_nodes().skip(1) {
                let t = expr_span_ty(a, &ch);
                if t != LeekTy::Unknown {
                    return t;
                }
            }
            ty_from_semantic_tokens(a, node)
        }
    }
}
