//! Typed AST nodes (extend with `Expr`, `Stmt`, … as the grammar parser grows).

use crate::language::LeekLanguage;
use rowan::SyntaxNode;

/// Common super-trait for typed wrappers around [`SyntaxNode`](rowan::SyntaxNode).
pub trait AstNode {
    fn syntax(&self) -> &SyntaxNode<LeekLanguage>;
}
