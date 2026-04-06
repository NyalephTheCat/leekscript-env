//! Java `LeekExpression`–style static operation counts (`getOperations()` after analyze), used with
//! [`Opcode::ChargeOps`](super::opcode::Opcode::ChargeOps) at loop boundaries to align with
//! `leekscript/compiler/bloc/{While,For,DoWhile}Block` (`ops(…, getOperations())`, `addCounter(1)`).

use sipha::prelude::AstNode;
use sipha::tree::ast::AstNodeExt;
use sipha::tree::red::{SyntaxElement, SyntaxNode};
use sipha::types::IntoSyntaxKind;

use crate::ast::{
    BinaryExpr, CallExpr, Expr, IndexExpr, MemberExpr, ParenExpr, TernaryExpr, UnaryExpr,
};
use crate::syntax::kinds::{Lex, Node};
use crate::syntax::syntax_el_is_trivia;

// `leekscript/runner/values/LeekValueType.java`
const MUL_COST: u32 = 2;
const DIV_COST: u32 = 5;
const MOD_COST: u32 = 5;
pub(crate) fn binary_op_kind(k: Lex) -> bool {
    matches!(
        k,
        Lex::Plus
            | Lex::Minus
            | Lex::Star
            | Lex::Slash
            | Lex::Backslash
            | Lex::Percent
            | Lex::EqEq
            | Lex::NotEq
            | Lex::EqEqEq
            | Lex::NotEqEq
            | Lex::Lt
            | Lex::Lte
            | Lex::Gt
            | Lex::Gte
            | Lex::AndAnd
            | Lex::OrOr
            | Lex::InstanceofKw
            | Lex::XorKw
    )
}

pub(crate) fn first_binary_op_token(bin: &SyntaxNode) -> Option<Lex> {
    let mut saw_starstar = false;
    for el in bin.children() {
        if syntax_el_is_trivia(&el) {
            continue;
        }
        let SyntaxElement::Token(t) = &el else {
            continue;
        };
        let Some(k) = t.kind_as::<Lex>() else {
            continue;
        };
        if k == Lex::InKw {
            return Some(Lex::InKw);
        }
        if binary_op_kind(k) {
            return Some(k);
        }
        if k == Lex::StarStar {
            saw_starstar = true;
        }
    }
    if saw_starstar {
        Some(Lex::StarStar)
    } else {
        None
    }
}

/// Non-trivia [`SyntaxElement`](SyntaxElement)s after the first binary operator token under `bin`.
pub(crate) fn suffix_after_first_binary_op(bin: &SyntaxNode) -> Vec<SyntaxElement> {
    let parts: Vec<_> = bin.children().filter(|e| !syntax_el_is_trivia(e)).collect();
    if let Some(in_pos) = parts.iter().position(|el| {
        matches!(
            el,
            SyntaxElement::Token(t) if t.kind_as::<Lex>() == Some(Lex::InKw)
        )
    }) {
        return parts[in_pos.saturating_add(1)..].to_vec();
    }
    // Prefer non-`**` operators when mixed (`a ** b == c`).
    if let Some(pos) = parts.iter().position(|el| {
        matches!(
            el,
            SyntaxElement::Token(t) if t.kind_as::<Lex>().is_some_and(binary_op_kind)
        )
    }) {
        return parts[pos.saturating_add(1)..].to_vec();
    }
    if let Some(pos) = parts.iter().position(|el| {
        matches!(
            el,
            SyntaxElement::Token(t) if t.kind_as::<Lex>() == Some(Lex::StarStar)
        )
    }) {
        return parts[pos.saturating_add(1)..].to_vec();
    }
    Vec::new()
}

/// Elements before the first binary operator token (lhs of one `BinaryExpr` node).
pub(crate) fn prefix_before_first_binary_op(bin: &SyntaxNode) -> Vec<SyntaxElement> {
    let parts: Vec<_> = bin.children().filter(|e| !syntax_el_is_trivia(e)).collect();
    if let Some(in_pos) = parts.iter().position(|el| {
        matches!(
            el,
            SyntaxElement::Token(t) if t.kind_as::<Lex>() == Some(Lex::InKw)
        )
    }) {
        let mut lhs_end = in_pos;
        if in_pos > 0
            && matches!(
                &parts[in_pos - 1],
                SyntaxElement::Token(t) if t.kind_as::<Lex>() == Some(Lex::NotKw)
            )
        {
            lhs_end = in_pos - 1;
        }
        return parts[..lhs_end].to_vec();
    }
    // Prefer non-`**` operators when mixed.
    if let Some(pos) = parts.iter().position(|el| {
        matches!(
            el,
            SyntaxElement::Token(t) if t.kind_as::<Lex>().is_some_and(binary_op_kind)
        )
    }) {
        return parts[..pos].to_vec();
    }
    if let Some(pos) = parts.iter().position(|el| {
        matches!(
            el,
            SyntaxElement::Token(t) if t.kind_as::<Lex>() == Some(Lex::StarStar)
        )
    }) {
        return parts[..pos].to_vec();
    }
    parts
}

fn bin_fragment_extra_cost(op: Lex) -> u32 {
    match op {
        Lex::Star => MUL_COST,
        Lex::Slash | Lex::Backslash => DIV_COST,
        Lex::Percent => MOD_COST,
        Lex::AndAnd | Lex::OrOr => 0,
        Lex::InKw => 1,
        _ => 1,
    }
}

/// Extra operation cost for `+=` / `-=` / … (Java `LeekExpression` analyze for compound assign).
pub(crate) fn compound_assign_bin_extra(assign_op: Lex) -> u32 {
    match assign_op {
        Lex::StarEq => MUL_COST,
        Lex::SlashEq | Lex::PercentEq => MOD_COST,
        _ => 1,
    }
}

/// Operation count for `parts` shaped like a level root: `[lhs, BinaryExpr, …]` (same as VM compiler).
pub(crate) fn java_ops_expr_flat(parts: &[SyntaxElement]) -> u32 {
    if parts.is_empty() {
        return 0;
    }
    if parts.len() == 1 {
        return match &parts[0] {
            SyntaxElement::Token(_) => 0,
            SyntaxElement::Node(n) => java_ops_syntax(n),
        };
    }
    let SyntaxElement::Node(last) = parts.last().expect("len >= 2") else {
        return 1;
    };
    if TernaryExpr::can_cast(last.kind()) {
        let lhs_o = java_ops_expr_flat(&parts[..parts.len() - 1]);
        let Some(t) = TernaryExpr::cast(last.clone()) else {
            return lhs_o.saturating_add(1);
        };
        let arms: Vec<Expr> = AstNodeExt::children::<Expr>(t.syntax()).collect();
        if arms.len() == 2 {
            return lhs_o
                .saturating_add(1)
                .saturating_add(java_analyzed_ops(&arms[0]))
                .saturating_add(java_analyzed_ops(&arms[1]));
        }
        return lhs_o.saturating_add(1);
    }
    if !BinaryExpr::can_cast(last.kind()) {
        return 1;
    }
    let op = first_binary_op_token(last).unwrap_or(Lex::Plus);
    let lhs = &parts[..parts.len() - 1];
    let lhs_o = java_ops_expr_flat(lhs);
    let suff = suffix_after_first_binary_op(last);
    let rhs_o = java_ops_infix_suffix(&suff);
    if matches!(op, Lex::AndAnd | Lex::OrOr) {
        return 0;
    }
    lhs_o + rhs_o + bin_fragment_extra_cost(op)
}

/// Suffix after a binary op token inside one `BinaryExpr` (`compile_infix_suffix` shape).
pub(crate) fn java_ops_infix_suffix(parts: &[SyntaxElement]) -> u32 {
    if parts.is_empty() {
        return 0;
    }
    if parts.len() == 1 {
        return match &parts[0] {
            SyntaxElement::Token(_) => 0,
            SyntaxElement::Node(n) => java_ops_syntax(n),
        };
    }
    let SyntaxElement::Node(last) = parts.last().expect("len >= 2") else {
        return 1;
    };
    if !BinaryExpr::can_cast(last.kind()) {
        return 1;
    }
    let op = first_binary_op_token(last).unwrap_or(Lex::Plus);
    let lhs = &parts[..parts.len() - 1];
    let lhs_o = match lhs {
        [SyntaxElement::Token(_)] => 0,
        [SyntaxElement::Node(n)] => java_ops_syntax(n),
        _ => java_ops_expr_flat(lhs),
    };
    let suff = suffix_after_first_binary_op(last);
    let rhs_o = java_ops_infix_suffix(&suff);
    if matches!(op, Lex::AndAnd | Lex::OrOr) {
        return 0;
    }
    lhs_o + rhs_o + bin_fragment_extra_cost(op)
}

/// Java `getOperations()`-style count for any expression node the VM can compile.
pub(crate) fn java_analyzed_ops(expr: &Expr) -> u32 {
    java_ops_syntax(expr.syntax())
}

pub(crate) fn java_analyzed_ops_syntax(n: &SyntaxNode) -> u32 {
    java_ops_syntax(n)
}

fn flatten_one_expr_layer(items: &[SyntaxElement]) -> Vec<SyntaxElement> {
    let mut out = Vec::new();
    for el in items {
        if let SyntaxElement::Node(node) = el {
            if node.kind() == Node::Expr.into_syntax_kind() {
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

fn java_ops_syntax(n: &SyntaxNode) -> u32 {
    if let Some(p) = ParenExpr::cast(n.clone()) {
        if let Some(inner_e) = p.syntax().child::<Expr>() {
            return java_analyzed_ops(&inner_e);
        }
        let lparen = Lex::LParen.into_syntax_kind();
        let rparen = Lex::RParen.into_syntax_kind();
        let full: Vec<_> = p
            .syntax()
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();
        if let (Some(open_idx), Some(close_idx)) = (
            full.iter()
                .position(|e| matches!(e, SyntaxElement::Token(t) if t.kind() == lparen)),
            full.iter()
                .rposition(|e| matches!(e, SyntaxElement::Token(t) if t.kind() == rparen)),
        ) {
            if close_idx > open_idx + 1 {
                let inner = &full[open_idx + 1..close_idx];
                let flat = flatten_one_expr_layer(inner);
                return java_ops_expr_flat(&flat);
            }
        }
        let inner: Vec<_> = p
            .syntax()
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();
        if inner.len() == 1 {
            if let SyntaxElement::Node(c) = &inner[0] {
                return java_ops_syntax(c);
            }
        }
    }
    if n.kind() == Node::Expr.into_syntax_kind() {
        let parts: Vec<_> = n.children().filter(|e| !syntax_el_is_trivia(e)).collect();
        return java_ops_expr_flat(&parts);
    }
    if let Some(ix) = IndexExpr::cast(n.clone()) {
        let subs: Vec<Expr> = AstNodeExt::children::<Expr>(ix.syntax()).collect();
        if subs.len() >= 2 {
            return java_analyzed_ops(&subs[0]).saturating_add(java_analyzed_ops(&subs[1]));
        }
        if let Some(e) = subs.first() {
            return java_analyzed_ops(e);
        }
        return 0;
    }
    if let Some(m) = MemberExpr::cast(n.clone()) {
        let subs: Vec<Expr> = AstNodeExt::children::<Expr>(m.syntax()).collect();
        if let Some(e) = subs.first() {
            return java_analyzed_ops(e).saturating_add(1);
        }
        return 1;
    }
    if let Some(c) = CallExpr::cast(n.clone()) {
        let subs: Vec<Expr> = AstNodeExt::children::<Expr>(c.syntax()).collect();
        let mut s = 1u32;
        for e in subs {
            s = s.saturating_add(java_analyzed_ops(&e));
        }
        return s;
    }
    if let Some(u) = UnaryExpr::cast(n.clone()) {
        return java_ops_unary(u.syntax());
    }
    if let Some(t) = TernaryExpr::cast(n.clone()) {
        let arms: Vec<Expr> = AstNodeExt::children::<Expr>(t.syntax()).collect();
        let mut c = 1u32;
        for e in arms {
            c = c.saturating_add(java_analyzed_ops(&e));
        }
        return c;
    }
    if BinaryExpr::can_cast(n.kind()) {
        if matches!(first_binary_op_token(n), Some(Lex::AndAnd) | Some(Lex::OrOr)) {
            return 0;
        }
        let lhs = prefix_before_first_binary_op(n);
        let lhs_o = java_ops_expr_flat(&lhs);
        let suff = suffix_after_first_binary_op(n);
        let rhs_o = java_ops_infix_suffix(&suff);
        let op = first_binary_op_token(n).unwrap_or(Lex::Plus);
        return lhs_o + rhs_o + bin_fragment_extra_cost(op);
    }
    if crate::ast::ArrayExpr::can_cast(n.kind()) {
        let items: Vec<Expr> = AstNodeExt::children::<Expr>(n).collect();
        let n_items = u32::try_from(items.len()).unwrap_or(0);
        let mut sum = 0u32;
        for e in items {
            sum = sum.saturating_add(java_analyzed_ops(&e));
        }
        return sum.saturating_add(n_items.saturating_mul(2));
    }
    if crate::ast::SetExpr::can_cast(n.kind()) {
        let items: Vec<Expr> = AstNodeExt::children::<Expr>(n).collect();
        let n_items = u32::try_from(items.len()).unwrap_or(0);
        let mut sum = 0u32;
        for e in items {
            sum = sum.saturating_add(java_analyzed_ops(&e));
        }
        return sum.saturating_add(n_items.saturating_mul(2));
    }
    // Literals, `null`, parenthesized, identifier-only leaves: 0 in Java for locals.
    0
}

fn java_ops_unary(n: &SyntaxNode) -> u32 {
    let semantic: Vec<_> = n.children().filter(|e| !syntax_el_is_trivia(e)).collect();
    let minus = Lex::Minus.into_syntax_kind();
    let bang = Lex::Bang.into_syntax_kind();
    let not_kw = Lex::NotKw.into_syntax_kind();
    let mut i = 0usize;
    let mut has_not = false;
    while i < semantic.len() {
        let SyntaxElement::Token(t) = &semantic[i] else {
            break;
        };
        let k = t.kind();
        if k == minus {
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
    let inner = match operand {
        [SyntaxElement::Node(inner)] => java_ops_syntax(inner),
        [SyntaxElement::Token(_)] => 0,
        _ => 0,
    };
    inner + u32::from(has_not)
}

/// `not in` under one relational [`Node::BinaryExpr`] (`[lhs, NOT, IN, rhs]`).
pub(crate) fn relational_in_has_not(bin: &SyntaxNode) -> bool {
    let parts: Vec<_> = bin.children().filter(|e| !syntax_el_is_trivia(e)).collect();
    let Some(in_pos) = parts.iter().position(|el| {
        matches!(
            el,
            SyntaxElement::Token(t) if t.kind_as::<Lex>() == Some(Lex::InKw)
        )
    }) else {
        return false;
    };
    in_pos > 0
        && matches!(
            &parts[in_pos - 1],
            SyntaxElement::Token(t) if t.kind_as::<Lex>() == Some(Lex::NotKw)
        )
}
