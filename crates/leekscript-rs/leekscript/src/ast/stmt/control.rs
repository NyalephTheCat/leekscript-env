use super::Block;
use super::Stmt;
use super::StmtBlock;
use crate::ast::expr::Expr;
use crate::syntax::kinds::K;
use sipha::prelude::*;
use sipha::tree::ast::AstNode;
use sipha::tree::ast::AstNodeExt;

fn first_branch_after_condition(syntax: &SyntaxNode) -> Option<StmtBlock> {
    let mut after_cond = false;
    for n in syntax.child_nodes() {
        if !after_cond {
            if Expr::can_cast(n.kind()) {
                after_cond = true;
            }
            continue;
        }
        if let Some(sb) = StmtBlock::cast_node(n) {
            return Some(sb);
        }
    }
    None
}

fn second_branch_after_condition(syntax: &SyntaxNode) -> Option<StmtBlock> {
    let mut after_cond = false;
    let mut seen = 0u8;
    for n in syntax.child_nodes() {
        if !after_cond {
            if Expr::can_cast(n.kind()) {
                after_cond = true;
            }
            continue;
        }
        if StmtBlock::cast_node(n.clone()).is_some() {
            seen += 1;
            if seen == 2 {
                return StmtBlock::cast_node(n);
            }
        }
    }
    None
}

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::IfStmt)]
pub struct IfStmt(SyntaxNode);

impl IfStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }

    /// `then` branch: `{ ... }` or a `K::Stmt`-wrapped statement (e.g. `if (x) return 1`).
    pub fn then_branch(&self) -> Option<StmtBlock> {
        first_branch_after_condition(self.syntax())
    }

    /// `else` branch when present (including `else if …`).
    pub fn else_branch(&self) -> Option<StmtBlock> {
        second_branch_after_condition(self.syntax())
    }

    /// `{ ... }` only; `None` when `then` is a single wrapped statement.
    pub fn then_block(&self) -> Option<Block> {
        self.then_branch().and_then(|b| b.into_block())
    }

    /// `else { ... }` only.
    pub fn else_block(&self) -> Option<Block> {
        self.else_branch().and_then(|b| b.into_block())
    }
}

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::WhileStmt)]
pub struct WhileStmt(SyntaxNode);

impl WhileStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }

    pub fn body(&self) -> Option<StmtBlock> {
        first_branch_after_condition(self.syntax())
    }
}

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::DoWhileStmt)]
pub struct DoWhileStmt(SyntaxNode);

impl DoWhileStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }

    pub fn body(&self) -> Option<StmtBlock> {
        for n in self.syntax().child_nodes() {
            if Expr::can_cast(n.kind()) {
                break;
            }
            if let Some(sb) = StmtBlock::cast_node(n) {
                return Some(sb);
            }
        }
        None
    }
}

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::ForStmt)]
pub struct ForStmt(SyntaxNode);

/// `for ( ; …` / `for (;;)` — header’s first clause is empty; CST expr children are `[cond, step]` not `[init, cond, step]`.
fn for_paren_header_starts_with_semicolon(syntax: &SyntaxNode) -> bool {
    let mut after_lparen = false;
    for el in syntax.children() {
        if crate::syntax::syntax_el_is_trivia(&el) {
            continue;
        }
        if let Some(t) = el.as_token() {
            let k = t.kind_as::<K>();
            if k == Some(K::LParen) {
                after_lparen = true;
                continue;
            }
            if after_lparen {
                return k == Some(K::Semi);
            }
        } else if after_lparen {
            return false;
        }
    }
    false
}

impl ForStmt {
    /// `for ( … init ; cond ; step )` initializer, if any (`None` for `for (; …` / `for (;;)`).
    pub fn init_expr(&self) -> Option<Expr> {
        if for_paren_header_starts_with_semicolon(self.syntax()) {
            return None;
        }
        AstNodeExt::children::<Expr>(self.syntax()).next()
    }

    pub fn condition_expr(&self) -> Option<Expr> {
        let mut it = AstNodeExt::children::<Expr>(self.syntax());
        if for_paren_header_starts_with_semicolon(self.syntax()) {
            return it.next();
        }
        it.nth(1)
    }

    pub fn step_expr(&self) -> Option<Expr> {
        let mut it = AstNodeExt::children::<Expr>(self.syntax());
        if for_paren_header_starts_with_semicolon(self.syntax()) {
            return it.nth(1);
        }
        it.nth(2)
    }

    pub fn body(&self) -> Option<StmtBlock> {
        last_stmt_block_child(self.syntax())
    }
}

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::ForeachStmt)]
pub struct ForeachStmt(SyntaxNode);

impl ForeachStmt {
    /// Expression after `in`.
    pub fn iterable(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }

    pub fn body(&self) -> Option<StmtBlock> {
        last_stmt_block_child(self.syntax())
    }
}

fn last_stmt_block_child(syntax: &SyntaxNode) -> Option<StmtBlock> {
    let nodes: Vec<_> = syntax.child_nodes().collect();
    nodes.into_iter().rev().find_map(StmtBlock::cast_node)
}

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::SwitchStmt)]
pub struct SwitchStmt(SyntaxNode);

impl SwitchStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }

    pub fn arms(&self) -> impl Iterator<Item = SwitchArm> + '_ {
        AstNodeExt::children::<SwitchArm>(self.syntax())
    }
}

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::SwitchArm)]
pub struct SwitchArm(SyntaxNode);

impl SwitchArm {
    /// `case` labels (expressions); `default` arms have none here.
    pub fn case_exprs(&self) -> impl Iterator<Item = Expr> + '_ {
        AstNodeExt::children::<Expr>(self.syntax())
    }

    pub fn stmts(&self) -> impl Iterator<Item = Stmt> + '_ {
        AstNodeExt::children::<Stmt>(self.syntax())
    }
}
