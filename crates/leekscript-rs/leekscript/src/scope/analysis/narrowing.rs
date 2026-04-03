//! Control-flow narrowing: which syntax nodes open/close narrowing regions.

use std::collections::HashMap;

use sipha::tree::ast::AstNode;
use sipha::tree::red::SyntaxNode;

use crate::ast::{IfStmt, StmtBlock, WhileStmt};
use crate::scope::leek_ty::LeekTy;
use crate::scope::model::SymbolId;
use crate::syntax::kinds::K;

use super::analyzer::Analyzer;
use super::condition::facts_from_condition_syntax;

pub(crate) fn should_track_narrowing(a: &Analyzer, node: &SyntaxNode) -> bool {
    match node.kind_as::<K>() {
        Some(K::Block) => !is_class_body_block_for_narrowing(a, node),
        Some(K::Stmt) => stmt_is_direct_if_or_while_body(node, a),
        _ => false,
    }
}

fn is_class_body_block_for_narrowing(a: &Analyzer, node: &SyntaxNode) -> bool {
    a.skip_leave_block_span == Some(node.text_range())
}

fn stmt_block_syntax_root(sb: &StmtBlock) -> SyntaxNode {
    match sb {
        StmtBlock::Block(b) => b.syntax().clone(),
        StmtBlock::Wrapped(st) => st.syntax().clone(),
    }
}

fn syntax_node_same_position(a: &SyntaxNode, b: &SyntaxNode) -> bool {
    a.offset() == b.offset() && a.kind() == b.kind()
}

fn stmt_is_direct_if_or_while_body(stmt: &SyntaxNode, a: &Analyzer) -> bool {
    if stmt.kind_as::<K>() != Some(K::Stmt) {
        return false;
    }
    let Some(parent) = a.syntax_parent_of(stmt) else {
        return false;
    };
    if parent.kind_as::<K>() == Some(K::IfStmt) {
        let Some(ifs) = IfStmt::cast(parent.clone()) else {
            return false;
        };
        if let Some(tb) = ifs.then_branch() {
            if syntax_node_same_position(&stmt_block_syntax_root(&tb), stmt) {
                return true;
            }
        }
        if let Some(eb) = ifs.else_branch() {
            if syntax_node_same_position(&stmt_block_syntax_root(&eb), stmt) {
                return true;
            }
        }
        return false;
    }
    if parent.kind_as::<K>() == Some(K::WhileStmt) {
        let Some(ws) = WhileStmt::cast(parent.clone()) else {
            return false;
        };
        return ws
            .body()
            .map(|b| syntax_node_same_position(&stmt_block_syntax_root(&b), stmt))
            .unwrap_or(false);
    }
    false
}

fn stmt_block_covers_node(sb: &StmtBlock, node: &SyntaxNode) -> bool {
    let root = stmt_block_syntax_root(sb);
    let nr = node.text_range();
    let rr = root.text_range();
    nr.start >= rr.start && nr.end <= rr.end
}

pub(crate) fn accumulated_narrowing_maps(
    a: &Analyzer,
    node: &SyntaxNode,
) -> HashMap<SymbolId, LeekTy> {
    let mut acc = HashMap::new();
    for anc in
        a.syntax_node_stack.iter().rev().skip(1).filter(|n| {
            n.kind_as::<K>() == Some(K::IfStmt) || n.kind_as::<K>() == Some(K::WhileStmt)
        })
    {
        if anc.kind_as::<K>() == Some(K::IfStmt) {
            let Some(ifs) = IfStmt::cast(anc.clone()) else {
                continue;
            };
            let Some(cond) = ifs.condition() else {
                continue;
            };
            let Some(then_br) = ifs.then_branch() else {
                continue;
            };
            if stmt_block_covers_node(&then_br, node) {
                merge_narrow_maps(&mut acc, &facts_from_condition_syntax(a, cond.syntax()));
            }
        } else if let Some(ws) = WhileStmt::cast(anc.clone()) {
            let Some(cond) = ws.condition() else {
                continue;
            };
            if let Some(body) = ws.body() {
                if stmt_block_covers_node(&body, node) {
                    merge_narrow_maps(&mut acc, &facts_from_condition_syntax(a, cond.syntax()));
                }
            }
        }
    }
    acc
}

fn merge_narrow_maps(acc: &mut HashMap<SymbolId, LeekTy>, more: &HashMap<SymbolId, LeekTy>) {
    for (k, v) in more {
        acc.insert(*k, v.clone());
    }
}
