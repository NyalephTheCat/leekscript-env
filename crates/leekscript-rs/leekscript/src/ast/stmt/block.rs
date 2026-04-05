use crate::syntax::kinds::Node;
use sipha::AstNode;
use sipha::prelude::*;
use sipha::tree::ast::AstNodeExt;

/// Braced statement list (`Node::Block`).
#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::Block)]
pub struct Block(SyntaxNode);

impl Block {
    /// Direct child statements (skips `{`, `}`, and non-`Stmt` members such as `ClassMember`).
    pub fn stmts(&self) -> impl Iterator<Item = super::Stmt> + '_ {
        AstNodeExt::children::<super::Stmt>(self.syntax())
    }
}
