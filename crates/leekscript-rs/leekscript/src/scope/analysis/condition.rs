//! Interpreting `if` / `while` condition syntax as narrowing facts.
//!
//! The implementation is token-sequence based (see [`facts_from_condition_syntax`]); a future
//! structured AST for conditions could replace the internals without changing call sites.

use std::collections::HashMap;

use sipha::tree::red::{SyntaxNode, SyntaxToken};

use crate::scope::leek_ty::LeekTy;
use crate::scope::model::SymbolId;
use crate::syntax::kinds::K;

use super::analyzer::Analyzer;

/// Positive narrowing facts implied when a condition is true (`instanceof`, `!= null`, `&&`).
pub(crate) fn facts_from_condition_syntax(
    a: &Analyzer,
    cond: &SyntaxNode,
) -> HashMap<SymbolId, LeekTy> {
    let mut out = HashMap::new();
    collect_positive_narrowing(a, cond, &mut out);
    out
}

fn collect_positive_narrowing(a: &Analyzer, n: &SyntaxNode, out: &mut HashMap<SymbolId, LeekTy>) {
    match n.kind_as::<K>() {
        Some(K::ParenExpr) | Some(K::Expr) => {
            extract_narrowing_from_expr_subtree(a, n, out);
        }
        _ => {}
    }
}

/// Split `toks` on `sep` (e.g. `&&`); if `sep` never appears, returns one segment (`toks`).
fn split_semantic_token_segments<'a>(toks: &'a [SyntaxToken], sep: K) -> Vec<&'a [SyntaxToken]> {
    if toks.is_empty() {
        return Vec::new();
    }
    if !toks.iter().any(|t| t.kind_as::<K>() == Some(sep)) {
        return vec![toks];
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, t) in toks.iter().enumerate() {
        if t.kind_as::<K>() == Some(sep) {
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
    out: &mut HashMap<SymbolId, LeekTy>,
) {
    let toks = expr.descendant_semantic_tokens();
    for seg in split_semantic_token_segments(&toks, K::AndAnd) {
        for w in seg.windows(3) {
            if w[0].kind_as::<K>() == Some(K::Ident)
                && w[1].kind_as::<K>() == Some(K::InstanceofKw)
                && w[2].kind_as::<K>() == Some(K::Ident)
                && w[2].text().chars().next().is_some_and(|c| c.is_uppercase())
            {
                let name = w[0].text();
                if seg.iter().any(|t| t.kind_as::<K>() == Some(K::Dot)) {
                    continue;
                }
                if let Some(sym) = a.resolve_here(name) {
                    out.insert(sym, LeekTy::Class(w[2].text().to_string()));
                }
            }
        }
        for w in seg.windows(3) {
            if w[0].kind_as::<K>() == Some(K::Ident)
                && matches!(w[1].kind_as::<K>(), Some(K::NotEq | K::NotEqEq))
                && w[2].kind_as::<K>() == Some(K::NullKw)
            {
                if let Some(sym) = a.resolve_here(w[0].text()) {
                    let base = a.symbol_effective_ty(sym);
                    if let Some(nn) = base.non_null_variant() {
                        out.insert(sym, nn);
                    }
                }
            }
            if w[0].kind_as::<K>() == Some(K::NullKw)
                && matches!(w[1].kind_as::<K>(), Some(K::NotEq | K::NotEqEq))
                && w[2].kind_as::<K>() == Some(K::Ident)
            {
                if let Some(sym) = a.resolve_here(w[2].text()) {
                    let base = a.symbol_effective_ty(sym);
                    if let Some(nn) = base.non_null_variant() {
                        out.insert(sym, nn);
                    }
                }
            }
        }
    }
}
