use super::{Block, Stmt};
use sipha::prelude::*;
use sipha::tree::ast::AstNode;

/// `stmt_or_block`: braced block vs a single statement wrapped in `Node::Stmt`.
#[derive(Debug, Clone)]
pub enum StmtBlock {
    Block(Block),
    /// Single-statement branch (`Node::Stmt` wrapper).
    Wrapped(Stmt),
}

impl StmtBlock {
    pub(crate) fn cast_node(n: SyntaxNode) -> Option<Self> {
        if let Some(b) = Block::cast(n.clone()) {
            return Some(Self::Block(b));
        }
        Stmt::cast(n).map(Self::Wrapped)
    }

    pub fn as_block(&self) -> Option<&Block> {
        match self {
            Self::Block(b) => Some(b),
            Self::Wrapped(_) => None,
        }
    }

    pub fn into_block(self) -> Option<Block> {
        match self {
            Self::Block(b) => Some(b),
            Self::Wrapped(_) => None,
        }
    }
}
