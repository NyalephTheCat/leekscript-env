//! Control-flow narrowing: which syntax nodes open/close narrowing regions.

use std::collections::HashMap;

use sipha::tree::ast::AstNode;
use sipha::tree::red::SyntaxNode;

use crate::ast::{Block, IfStmt, Stmt, StmtBlock, WhileStmt};
use crate::scope::leek_ty::LeekTy;
use crate::scope::model::SymbolId;
use crate::syntax::kinds::Node;

use super::analyzer::Analyzer;
use super::condition::{
    facts_from_condition_negated_syntax, facts_from_condition_negated_syntax_with_base,
    facts_from_condition_syntax, facts_from_condition_syntax_with_base,
};

pub(crate) fn should_track_narrowing(a: &Analyzer, node: &SyntaxNode) -> bool {
    match node.kind_as::<Node>() {
        Some(Node::Block) => !is_class_body_block_for_narrowing(a, node),
        _ => {
            node_is_direct_if_or_while_branch_body(node, a)
                || stmt_is_direct_block_child_statement(node, a)
        }
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

/// `if (…) …` / `while (…) …` branch body (including a single wrapped statement).
fn node_is_direct_if_or_while_branch_body(node: &SyntaxNode, a: &Analyzer) -> bool {
    let Some(parent) = a.syntax_parent_of(node) else {
        return false;
    };
    if parent.kind_as::<Node>() == Some(Node::IfStmt) {
        let Some(ifs) = IfStmt::cast(parent.clone()) else {
            return false;
        };
        if let Some(tb) = ifs.then_branch() {
            if syntax_node_same_position(&stmt_block_syntax_root(&tb), node) {
                return true;
            }
        }
        if let Some(eb) = ifs.else_branch() {
            if syntax_node_same_position(&stmt_block_syntax_root(&eb), node) {
                return true;
            }
        }
        return false;
    }
    if parent.kind_as::<Node>() == Some(Node::WhileStmt) {
        let Some(ws) = WhileStmt::cast(parent.clone()) else {
            return false;
        };
        return ws
            .body()
            .map(|b| syntax_node_same_position(&stmt_block_syntax_root(&b), node))
            .unwrap_or(false);
    }
    false
}

fn stmt_is_direct_block_child_statement(node: &SyntaxNode, a: &Analyzer) -> bool {
    Stmt::cast(node.clone()).is_some_and(|_| {
        let Some(parent) = a.syntax_parent_of(node) else {
            return false;
        };
        parent.kind_as::<Node>() == Some(Node::Block) && !is_class_body_block_for_narrowing(a, &parent)
    })
}

fn stmt_syntax_root(s: &Stmt) -> SyntaxNode {
    match s {
        Stmt::Include(x) => x.syntax().clone(),
        Stmt::Return(x) => x.syntax().clone(),
        Stmt::Break(x) => x.syntax().clone(),
        Stmt::Continue(x) => x.syntax().clone(),
        Stmt::VarDecl(x) => x.syntax().clone(),
        Stmt::Function(x) => x.syntax().clone(),
        Stmt::Expr(x) => x.syntax().clone(),
        Stmt::Global(x) => x.syntax().clone(),
        Stmt::Else(x) => x.syntax().clone(),
        Stmt::Switch(x) => x.syntax().clone(),
        Stmt::Class(x) => x.syntax().clone(),
        Stmt::If(x) => x.syntax().clone(),
        Stmt::For(x) => x.syntax().clone(),
        Stmt::Foreach(x) => x.syntax().clone(),
        Stmt::DoWhile(x) => x.syntax().clone(),
        Stmt::While(x) => x.syntax().clone(),
        Stmt::Try(x) => x.syntax().clone(),
        Stmt::Throw(x) => x.syntax().clone(),
        Stmt::Import(x) => x.syntax().clone(),
        Stmt::Export(x) => x.syntax().clone(),
        Stmt::Goto(x) => x.syntax().clone(),
        Stmt::Package(x) => x.syntax().clone(),
        Stmt::Const(x) => x.syntax().clone(),
        Stmt::Match(x) => x.syntax().clone(),
        Stmt::Empty(x) => x.syntax().clone(),
        Stmt::Error(x) => x.syntax().clone(),
    }
}

fn stmt_block_always_abrupt(sb: &StmtBlock) -> bool {
    match sb {
        StmtBlock::Block(b) => block_always_abrupt(b),
        StmtBlock::Wrapped(st) => stmt_always_abrupt(st),
    }
}

fn block_always_abrupt(b: &Block) -> bool {
    for st in b.stmts() {
        if stmt_always_abrupt(&st) {
            return true;
        }
    }
    false
}

fn stmt_always_abrupt(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return(_) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::Throw(_) => true,
        Stmt::If(ifs) => {
            let has_else = ifs.else_branch().is_some();
            if !has_else {
                return false;
            }
            let then_ab = ifs
                .then_branch()
                .map(|b| stmt_block_always_abrupt(&b))
                .unwrap_or(true);
            let else_ab = ifs
                .else_branch()
                .map(|b| stmt_block_always_abrupt(&b))
                .unwrap_or(true);
            then_ab && else_ab
        }
        _ => false,
    }
}

/// Narrowing that applies to statements **after** this `if` when control can only reach them
/// under one outcome of the condition (early `return` / `throw` / always-abrupt `else`, etc.).
fn narrowing_after_guard_if(
    a: &Analyzer,
    ifs: &IfStmt,
    base_ty: &impl Fn(SymbolId) -> LeekTy,
) -> Option<HashMap<SymbolId, LeekTy>> {
    let then_br = ifs.then_branch()?;
    let then_ab = stmt_block_always_abrupt(&then_br);
    let else_br = ifs.else_branch();
    let else_ab = else_br
        .as_ref()
        .map(stmt_block_always_abrupt)
        .unwrap_or(false);
    let has_else = else_br.is_some();
    let cond_expr = ifs.condition()?;
    let cond = cond_expr.syntax();

    if has_else && else_ab && !then_ab {
        return Some(facts_from_condition_syntax_with_base(a, cond, base_ty));
    }
    if then_ab && !(has_else && else_ab) {
        let m = facts_from_condition_negated_syntax_with_base(a, cond, base_ty);
        return Some(m);
    }
    None
}

fn predecessor_guard_narrowing_maps(a: &Analyzer, node: &SyntaxNode) -> HashMap<SymbolId, LeekTy> {
    if !stmt_is_direct_block_child_statement(node, a) {
        return HashMap::new();
    }
    let Some(parent) = a.syntax_parent_of(node) else {
        return HashMap::new();
    };
    let Some(block) = Block::cast(parent) else {
        return HashMap::new();
    };
    let stmts: Vec<Stmt> = block.stmts().collect();
    let Some(idx) = stmts
        .iter()
        .position(|s| syntax_node_same_position(&stmt_syntax_root(s), node))
    else {
        return HashMap::new();
    };
    let mut refined: HashMap<SymbolId, LeekTy> = HashMap::new();
    for prec in stmts.iter().take(idx) {
        if let Stmt::If(ifs) = prec {
            if let Some(delta) = narrowing_after_guard_if(a, ifs, &|sid| {
                refined
                    .get(&sid)
                    .cloned()
                    .unwrap_or_else(|| a.symbol_effective_ty(sid))
            }) {
                refined.extend(delta);
            }
        }
    }
    refined
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
            n.kind_as::<Node>() == Some(Node::IfStmt) || n.kind_as::<Node>() == Some(Node::WhileStmt)
        })
    {
        if anc.kind_as::<Node>() == Some(Node::IfStmt) {
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
            } else if let Some(else_br) = ifs.else_branch() {
                if stmt_block_covers_node(&else_br, node) {
                    merge_narrow_maps(
                        &mut acc,
                        &facts_from_condition_negated_syntax(a, cond.syntax()),
                    );
                }
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
    merge_narrow_maps(&mut acc, &predecessor_guard_narrowing_maps(a, node));
    acc
}

fn merge_narrow_maps(acc: &mut HashMap<SymbolId, LeekTy>, more: &HashMap<SymbolId, LeekTy>) {
    for (k, v) in more {
        acc.insert(*k, v.clone());
    }
}
