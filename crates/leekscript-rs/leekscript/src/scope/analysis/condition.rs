//! Interpreting `if` / `while` condition syntax as narrowing facts.
//!
//! The implementation is token-sequence based (see [`facts_from_condition_syntax`]); a future
//! structured AST for conditions could replace the internals without changing call sites.

use std::collections::HashMap;

use sipha::tree::red::{SyntaxNode, SyntaxToken};

use crate::scope::leek_ty::LeekTy;
use crate::scope::model::SymbolId;
use crate::syntax::kinds::{Lex, Node};
use crate::syntax::syntax_el_is_trivia;

use super::analyzer::Analyzer;
use super::infer::{binary_expr_is_instanceof, unary_expr_leading_bang_token};

/// Positive narrowing facts implied when a condition is true (`instanceof`, `!= null`, `&&`).
pub(crate) fn facts_from_condition_syntax(
    a: &Analyzer,
    cond: &SyntaxNode,
) -> HashMap<SymbolId, LeekTy> {
    facts_from_condition_syntax_with_base(a, cond, |sid| a.symbol_effective_ty(sid))
}

pub(crate) fn facts_from_condition_syntax_with_base(
    a: &Analyzer,
    cond: &SyntaxNode,
    base_ty: impl Fn(SymbolId) -> LeekTy,
) -> HashMap<SymbolId, LeekTy> {
    let mut out = HashMap::new();
    collect_positive_narrowing_with_base(a, cond, &base_ty, &mut out);
    out
}

fn binary_expr_logical_or_operands(
    a: &Analyzer,
    expr: &SyntaxNode,
) -> Option<(SyntaxNode, SyntaxNode)> {
    if !binary_expr_has_op_token(expr, Lex::OrOr) {
        return None;
    }
    let l = short_circuit_or_lhs_operand(a, expr)?;
    let r = binary_expr_non_trivia_children(expr).last()?.clone();
    Some((l, r))
}

/// Refinements that hold when `expr` is **false** (e.g. evaluating the RHS of `A || B` after `A`
/// was falsy — short-circuit).
#[must_use]
pub(crate) fn facts_when_expression_known_false(
    a: &Analyzer,
    expr: &SyntaxNode,
    base_ty: &impl Fn(SymbolId) -> LeekTy,
) -> HashMap<SymbolId, LeekTy> {
    let inner = unwrap_expr_paren_chain(expr.clone());
    match inner.kind_as::<Node>() {
        Some(Node::BinaryExpr) => {
            if binary_expr_has_op_token(&inner, Lex::OrOr) {
                if let Some((l, r)) = binary_expr_logical_or_operands(a, &inner) {
                    let mut left_map = facts_when_expression_known_false(a, &l, base_ty);
                    let right_map = facts_when_expression_known_false(a, &r, base_ty);
                    for (k, v) in right_map {
                        left_map.insert(k, v);
                    }
                    return left_map;
                }
                return HashMap::new();
            }
            if binary_expr_has_op_token(&inner, Lex::AndAnd) {
                return HashMap::new();
            }
            if binary_expr_is_instanceof(&inner) {
                let mut out = HashMap::new();
                if !inner
                    .descendant_semantic_tokens()
                    .iter()
                    .any(|t| t.kind_as::<Lex>() == Some(Lex::AndAnd))
                {
                    collect_negated_instanceof_narrowing_with_base(a, &inner, base_ty, &mut out);
                }
                return out;
            }
            if binary_expr_has_op_token(&inner, Lex::EqEq)
                || binary_expr_has_op_token(&inner, Lex::EqEqEq)
            {
                return facts_eq_null_comparison_when_false(a, &inner, base_ty);
            }
        }
        Some(Node::UnaryExpr) if unary_expr_leading_bang_token(&inner) => {
            if let Some(op) = unary_leading_bang_operand(&inner) {
                return facts_from_condition_syntax_with_base(a, &op, |sid| base_ty(sid));
            }
        }
        _ => {}
    }
    HashMap::new()
}

fn binary_expr_has_op_token(expr: &SyntaxNode, op: Lex) -> bool {
    expr.child_tokens().any(|t| t.kind_as::<Lex>() == Some(op))
}

fn binary_expr_non_trivia_children(expr: &SyntaxNode) -> Vec<SyntaxNode> {
    expr.child_nodes()
        .filter(|n| n.kind_as::<Node>() != Some(Node::Trivia))
        .collect()
}

fn syntax_nodes_equal(a: &SyntaxNode, b: &SyntaxNode) -> bool {
    a.offset() == b.offset() && a.kind() == b.kind() && a.text_len() == b.text_len()
}

/// `!x` → operand `x` (after paren unwrap on the operand subtree root).
fn unary_leading_bang_operand(node: &SyntaxNode) -> Option<SyntaxNode> {
    if node.kind_as::<Node>() != Some(Node::UnaryExpr) || !unary_expr_leading_bang_token(node) {
        return None;
    }
    for el in node.children() {
        if syntax_el_is_trivia(&el) {
            continue;
        }
        if el
            .as_token()
            .is_some_and(|t| t.kind_as::<Lex>() == Some(Lex::Bang))
        {
            continue;
        }
        if let Some(n) = el.as_node() {
            return Some(unwrap_expr_paren_chain(n.clone()));
        }
    }
    None
}

/// When `x == null` / `null == x` is false, `x` is non-null (same refinement as `x != null` true).
fn facts_eq_null_comparison_when_false(
    a: &Analyzer,
    expr: &SyntaxNode,
    base_ty: &impl Fn(SymbolId) -> LeekTy,
) -> HashMap<SymbolId, LeekTy> {
    let mut out = HashMap::new();
    let mut scan_roots = vec![expr.clone()];
    let mut leading_ident_tokens: Vec<SyntaxToken> = Vec::new();
    let n_op_children = binary_expr_non_trivia_children(expr).len();
    if (binary_expr_has_op_token(expr, Lex::EqEq) || binary_expr_has_op_token(expr, Lex::EqEqEq))
        && n_op_children <= 1
        && let Some(gp) = parent_syntax_node(a, expr)
    {
        let mut prev_node: Option<SyntaxNode> = None;
        let mut prev_ident: Option<SyntaxToken> = None;
        for el in gp.children() {
            if syntax_el_is_trivia(&el) {
                continue;
            }
            if let Some(n) = el.as_node() {
                if syntax_nodes_equal(n, expr) {
                    if let Some(pn) = prev_node {
                        scan_roots.insert(0, pn);
                    } else if let Some(id) = prev_ident.take() {
                        leading_ident_tokens.push(id);
                    }
                    break;
                }
                prev_node = Some(n.clone());
                prev_ident = None;
            } else if let Some(t) = el.as_token() {
                if t.kind_as::<Lex>() == Some(Lex::Ident) {
                    prev_ident = Some(t.clone());
                    prev_node = None;
                }
            }
        }
    }
    let mut toks: Vec<SyntaxToken> = scan_roots
        .iter()
        .flat_map(|n| n.descendant_semantic_tokens())
        .chain(leading_ident_tokens)
        .collect();
    toks.sort_by_key(|t| t.text_range().start);
    toks.dedup_by_key(|t| t.text_range().start);
    if toks
        .iter()
        .any(|t| matches!(t.kind_as::<Lex>(), Some(Lex::AndAnd | Lex::OrOr)))
    {
        return out;
    }
    for w in toks.windows(3) {
        if w[0].kind_as::<Lex>() == Some(Lex::Ident)
            && matches!(w[1].kind_as::<Lex>(), Some(Lex::EqEq | Lex::EqEqEq))
            && w[2].kind_as::<Lex>() == Some(Lex::NullKw)
        {
            if toks.iter().any(|t| t.kind_as::<Lex>() == Some(Lex::Dot)) {
                continue;
            }
            if let Some(sym) = a.resolve_here(w[0].text()) {
                let base = base_ty(sym);
                if let Some(nn) = base.non_null_variant() {
                    out.insert(sym, nn);
                }
            }
        }
        if w[0].kind_as::<Lex>() == Some(Lex::NullKw)
            && matches!(w[1].kind_as::<Lex>(), Some(Lex::EqEq | Lex::EqEqEq))
            && w[2].kind_as::<Lex>() == Some(Lex::Ident)
        {
            if toks.iter().any(|t| t.kind_as::<Lex>() == Some(Lex::Dot)) {
                continue;
            }
            if let Some(sym) = a.resolve_here(w[2].text()) {
                let base = base_ty(sym);
                if let Some(nn) = base.non_null_variant() {
                    out.insert(sym, nn);
                }
            }
        }
    }
    out
}

/// Whether `node` is the right-hand operand node of a parent `A || B` [`Node::BinaryExpr`].
#[must_use]
pub(crate) fn is_short_circuit_or_rhs_operand(a: &Analyzer, node: &SyntaxNode) -> bool {
    let Some(parent) = a.syntax_parent_of(node) else {
        return false;
    };
    if parent.kind_as::<Node>() != Some(Node::BinaryExpr) {
        return false;
    }
    if !binary_expr_has_op_token(&parent, Lex::OrOr) {
        return false;
    }
    let kids = binary_expr_non_trivia_children(&parent);
    let Some(rhs) = kids.last() else {
        return false;
    };
    syntax_nodes_equal(rhs, node)
}

fn immediate_parent_in_subtree(root: &SyntaxNode, target: &SyntaxNode) -> Option<SyntaxNode> {
    for child in root.child_nodes() {
        if syntax_nodes_equal(&child, target) {
            return Some(root.clone());
        }
        if let Some(p) = immediate_parent_in_subtree(&child, target) {
            return Some(p);
        }
    }
    None
}

fn parent_syntax_node(a: &Analyzer, node: &SyntaxNode) -> Option<SyntaxNode> {
    if let Some(p) = a.syntax_parent_of(node) {
        return Some(p);
    }
    let root = a.syntax_node_stack.first()?;
    immediate_parent_in_subtree(root, node)
}

/// Left operand of `lhs || rhs`.
///
/// Sipha's [`left_assoc_infix_level`](sipha::parse::expr::left_assoc_infix_level) puts only the
/// operator and the **right** operand inside each [`Node::BinaryExpr`]; the logical lhs is always the
/// preceding non-trivia [`SyntaxElement`]s under the same parent (often a node; the rhs subtree
/// may itself contain several nodes such as `MemberExpr` + relational `BinaryExpr`).
#[must_use]
pub(crate) fn short_circuit_or_lhs_operand(
    a: &Analyzer,
    or_binary: &SyntaxNode,
) -> Option<SyntaxNode> {
    if !binary_expr_has_op_token(or_binary, Lex::OrOr) {
        return None;
    }
    let gp = parent_syntax_node(a, or_binary)?;
    let mut prev: Option<SyntaxNode> = None;
    for el in gp.children() {
        if syntax_el_is_trivia(&el) {
            continue;
        }
        if let Some(n) = el.as_node() {
            if syntax_nodes_equal(n, or_binary) {
                return prev;
            }
            prev = Some(n.clone());
        }
    }
    None
}

/// Narrowing when the condition is **false** (`else` branch): e.g. `x instanceof C` → exclude `C`
/// from `x`'s union type. Skipped when the condition uses `&&` (negation is not a single type map).
///
/// For a leading logical `!`, uses positive facts on the operand (reaching code means the operand
/// is true), e.g. `if (!(x instanceof T)) return` → after the guard, `x` is `T`.
pub(crate) fn facts_from_condition_negated_syntax(
    a: &Analyzer,
    cond: &SyntaxNode,
) -> HashMap<SymbolId, LeekTy> {
    facts_from_condition_negated_syntax_with_base(a, cond, |sid| a.symbol_effective_ty(sid))
}

pub(crate) fn facts_from_condition_negated_syntax_with_base(
    a: &Analyzer,
    cond: &SyntaxNode,
    base_ty: impl Fn(SymbolId) -> LeekTy,
) -> HashMap<SymbolId, LeekTy> {
    let mut out = HashMap::new();
    if condition_contains_and_and(cond) {
        return out;
    }
    if let Some(inner) = peel_logical_not_operand(cond) {
        collect_positive_narrowing_with_base(a, &inner, &base_ty, &mut out);
        return out;
    }
    collect_negated_instanceof_narrowing_with_base(a, cond, &base_ty, &mut out);
    out
}

fn condition_contains_and_and(cond: &SyntaxNode) -> bool {
    cond.descendant_semantic_tokens()
        .into_iter()
        .any(|t| t.kind_as::<Lex>() == Some(Lex::AndAnd))
}

fn unwrap_expr_paren_chain(mut n: SyntaxNode) -> SyntaxNode {
    loop {
        match n.kind_as::<Node>() {
            Some(Node::Expr) => {
                // `Expr` may hold `lhs instanceof T` as `Ident` token + `BinaryExpr` whose span is
                // only ` instanceof T` (lhs is a sibling). Do not unwrap to the bare `BinaryExpr` or
                // token-based narrowing loses the receiver ident.
                if let Some(bin) = n
                    .child_nodes()
                    .find(|c| c.kind_as::<Node>() == Some(Node::BinaryExpr))
                {
                    let sem = bin.descendant_semantic_tokens();
                    if sem
                        .first()
                        .is_some_and(|t| t.kind_as::<Lex>() == Some(Lex::InstanceofKw))
                    {
                        break;
                    }
                }
                let next = n.child_nodes().next();
                if let Some(next) = next {
                    n = next;
                    continue;
                }
            }
            Some(Node::ParenExpr) => {
                let next = n.child_nodes().next();
                if let Some(next) = next {
                    n = next;
                    continue;
                }
            }
            _ => {}
        }
        break;
    }
    n
}

fn peel_logical_not_operand(cond: &SyntaxNode) -> Option<SyntaxNode> {
    let inner = unwrap_expr_paren_chain(cond.clone());
    if inner.kind_as::<Node>() != Some(Node::UnaryExpr) {
        return None;
    }
    if !unary_expr_leading_bang_token(&inner) {
        return None;
    }
    for el in inner.children() {
        if syntax_el_is_trivia(&el) {
            continue;
        }
        if el
            .as_token()
            .is_some_and(|t| t.kind_as::<Lex>() == Some(Lex::Bang))
        {
            continue;
        }
        if let Some(n) = el.as_node() {
            return Some(unwrap_expr_paren_chain(n.clone()));
        }
    }
    None
}

fn collect_positive_narrowing_with_base(
    a: &Analyzer,
    n: &SyntaxNode,
    base_ty: &impl Fn(SymbolId) -> LeekTy,
    out: &mut HashMap<SymbolId, LeekTy>,
) {
    match n.kind_as::<Node>() {
        Some(Node::ParenExpr) | Some(Node::Expr) | Some(Node::BinaryExpr) => {
            extract_narrowing_from_expr_subtree(a, n, base_ty, out);
        }
        _ => {}
    }
}

/// Split `toks` on `sep` (e.g. `&&`); if `sep` never appears, returns one segment (`toks`).
fn split_semantic_token_segments<'a>(toks: &'a [SyntaxToken], sep: Lex) -> Vec<&'a [SyntaxToken]> {
    if toks.is_empty() {
        return Vec::new();
    }
    if !toks.iter().any(|t| t.kind_as::<Lex>() == Some(sep)) {
        return vec![toks];
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, t) in toks.iter().enumerate() {
        if t.kind_as::<Lex>() == Some(sep) {
            if start < i {
                out.push(&toks[start..i]);
            }
            start = i + 1;
        }
    }
    if start < toks.len() {
        out.push(&toks[start..]);
    }
    out
}

fn extract_narrowing_from_expr_subtree(
    a: &Analyzer,
    expr: &SyntaxNode,
    base_ty: &impl Fn(SymbolId) -> LeekTy,
    out: &mut HashMap<SymbolId, LeekTy>,
) {
    let toks = expr.descendant_semantic_tokens();
    for seg in split_semantic_token_segments(&toks, Lex::AndAnd) {
        for w in seg.windows(3) {
            if w[0].kind_as::<Lex>() == Some(Lex::Ident)
                && w[1].kind_as::<Lex>() == Some(Lex::InstanceofKw)
                && w[2].kind_as::<Lex>() == Some(Lex::Ident)
                && w[2].text().chars().next().is_some_and(|c| c.is_uppercase())
            {
                let name = w[0].text();
                if seg.iter().any(|t| t.kind_as::<Lex>() == Some(Lex::Dot)) {
                    continue;
                }
                if let Some(sym) = a.resolve_here(name) {
                    let base = base_ty(sym);
                    if matches!(base, LeekTy::TypeParam(_)) {
                        continue;
                    }
                    out.insert(sym, LeekTy::Class(w[2].text().to_string()));
                }
            }
        }
        for w in seg.windows(3) {
            if w[0].kind_as::<Lex>() == Some(Lex::Ident)
                && matches!(w[1].kind_as::<Lex>(), Some(Lex::NotEq | Lex::NotEqEq))
                && w[2].kind_as::<Lex>() == Some(Lex::NullKw)
            {
                if let Some(sym) = a.resolve_here(w[0].text()) {
                    let base = base_ty(sym);
                    if let Some(nn) = base.non_null_variant() {
                        out.insert(sym, nn);
                    }
                }
            }
            if w[0].kind_as::<Lex>() == Some(Lex::NullKw)
                && matches!(w[1].kind_as::<Lex>(), Some(Lex::NotEq | Lex::NotEqEq))
                && w[2].kind_as::<Lex>() == Some(Lex::Ident)
            {
                if let Some(sym) = a.resolve_here(w[2].text()) {
                    let base = base_ty(sym);
                    if let Some(nn) = base.non_null_variant() {
                        out.insert(sym, nn);
                    }
                }
            }
        }
    }
}

fn collect_negated_instanceof_narrowing_with_base(
    a: &Analyzer,
    expr: &SyntaxNode,
    base_ty: &impl Fn(SymbolId) -> LeekTy,
    out: &mut HashMap<SymbolId, LeekTy>,
) {
    match expr.kind_as::<Node>() {
        Some(Node::ParenExpr) | Some(Node::Expr) | Some(Node::BinaryExpr) => {
            let toks = expr.descendant_semantic_tokens();
            for seg in split_semantic_token_segments(&toks, Lex::AndAnd) {
                for w in seg.windows(3) {
                    if w[0].kind_as::<Lex>() == Some(Lex::Ident)
                        && w[1].kind_as::<Lex>() == Some(Lex::InstanceofKw)
                        && w[2].kind_as::<Lex>() == Some(Lex::Ident)
                        && w[2].text().chars().next().is_some_and(|c| c.is_uppercase())
                    {
                        let name = w[0].text();
                        if seg.iter().any(|t| t.kind_as::<Lex>() == Some(Lex::Dot)) {
                            continue;
                        }
                        if let Some(sym) = a.resolve_here(name) {
                            let base = base_ty(sym);
                            if matches!(base, LeekTy::TypeParam(_)) {
                                continue;
                            }
                            let excluded = w[2].text();
                            if let Some(t) = base.exclude_instanceof_class(excluded) {
                                out.insert(sym, t);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use sipha::prelude::AstNode;
    use sipha::types::IntoSyntaxKind;

    use crate::Version;
    use crate::ast::IfStmt;
    use crate::parse_doc;
    use crate::syntax::kinds::{Lex, Node};

    use super::*;

    /// Parser may place `instanceof` lhs as a sibling token under `Expr`; peeling `!` must keep the
    /// `Expr` wrapper so token windows still see `lhs instanceof Type`.
    #[test]
    fn peel_bang_keeps_expr_when_instanceof_lhs_is_token_sibling() {
        let doc = parse_doc(
            concat!(
                "class GameState {}\n",
                "function f(GameState | Consequences state) {\n",
                "    if (!(state instanceof GameState)) { return; }\n",
                "}\n",
            ),
            Version::V4,
        )
        .expect("parse");
        let if_node = doc
            .root()
            .find_all_nodes(Node::IfStmt.into_syntax_kind())
            .into_iter()
            .next()
            .expect("if");
        let ifs = IfStmt::cast(if_node).expect("cast");
        let cond = ifs.condition().expect("cond");
        let inner = peel_logical_not_operand(cond.syntax()).expect("peel !");
        assert_eq!(inner.kind_as::<Node>(), Some(Node::Expr));
        let toks = inner.descendant_semantic_tokens();
        let window_ok = toks.windows(3).any(|w| {
            w[0].kind_as::<Lex>() == Some(Lex::Ident)
                && w[0].text() == "state"
                && w[1].kind_as::<Lex>() == Some(Lex::InstanceofKw)
                && w[2].kind_as::<Lex>() == Some(Lex::Ident)
                && w[2].text() == "GameState"
        });
        assert!(window_ok, "tokens: {:?}", toks);
    }
}
