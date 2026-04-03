//! Expression type inference helpers (phase 2).

use sipha::tree::red::SyntaxNode;

use crate::syntax::kinds::K;

use crate::scope::leek_ty::LeekTy;
use crate::scope::model::{ExprTypeKey, SymbolId};

use super::analyzer::Analyzer;

pub(crate) fn set_var_inferred_if_unannotated(a: &mut Analyzer, sid: SymbolId, ty: LeekTy) {
    let sym = &mut a.graph.symbols[sid.0 as usize];
    if sym.declared_ty.is_none() {
        sym.inferred_ty = Some(ty);
    }
}

pub(crate) fn binary_expr_is_instanceof(node: &SyntaxNode) -> bool {
    node.kind_as::<K>() == Some(K::BinaryExpr)
        && node
            .child_tokens()
            .any(|t| t.kind_as::<K>() == Some(K::InstanceofKw))
}

pub(crate) fn infer_binary(a: &Analyzer, node: &SyntaxNode) -> LeekTy {
    let kids: Vec<_> = node.child_nodes().collect();
    if kids.len() < 2 {
        return LeekTy::Unknown;
    }
    let l = &kids[0];
    let r = &kids[kids.len() - 1];
    if binary_expr_is_instanceof(node) {
        return LeekTy::Boolean;
    }
    if node.child_tokens().any(|t| {
        matches!(
            t.kind_as::<K>(),
            Some(K::EqEq | K::NotEq | K::EqEqEq | K::NotEqEq | K::Lt | K::Lte | K::Gt | K::Gte)
        )
    }) {
        return LeekTy::Boolean;
    }
    if node
        .child_tokens()
        .any(|t| matches!(t.kind_as::<K>(), Some(K::AndAnd | K::OrOr)))
    {
        return LeekTy::Boolean;
    }
    let lk = expr_span_ty(a, l);
    let rk = expr_span_ty(a, r);
    LeekTy::coerce_binary_op(&lk, &rk)
}

pub(crate) fn infer_interval_ty(a: &Analyzer, node: &SyntaxNode) -> LeekTy {
    let mut acc = None::<LeekTy>;
    for tok in node.descendant_semantic_tokens() {
        if tok.kind_as::<K>() == Some(K::Number) {
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
        if n.kind_as::<K>() != Some(K::Expr) {
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
        .any(|n| n.kind_as::<K>() == Some(K::BinaryExpr))
    {
        return LeekTy::Unknown;
    }
    for tok in node.descendant_semantic_tokens() {
        let key = ExprTypeKey::from_span(tok.text_range());
        if let Some(t) = a.expr_types.get(&key) {
            if *t != LeekTy::Unknown {
                return t.clone();
            }
        }
    }
    LeekTy::Unknown
}

pub(crate) fn expr_span_ty(a: &Analyzer, node: &SyntaxNode) -> LeekTy {
    let r = node.text_range();
    let key = ExprTypeKey::from_span(r);
    if let Some(t) = a.expr_types.get(&key) {
        if *t != LeekTy::Unknown {
            return t.clone();
        }
    }

    match node.kind_as::<K>() {
        Some(K::BinaryExpr) => infer_binary(a, node),
        Some(K::IntervalExpr) => infer_interval_ty(a, node),
        Some(K::Expr) => {
            if let Some(ch) = node.child_nodes().next() {
                let t = expr_span_ty(a, &ch);
                if t != LeekTy::Unknown {
                    return t;
                }
            }
            ty_from_semantic_tokens(a, node)
        }
        Some(K::UnaryExpr) => node
            .child_nodes()
            .last()
            .map(|c| expr_span_ty(a, &c))
            .unwrap_or(LeekTy::Unknown),
        Some(K::ParenExpr) => node
            .child_nodes()
            .find(|c| c.kind_as::<K>() == Some(K::Expr))
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
