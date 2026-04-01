use crate::syntax::kinds::K;
use sipha::AstNode;
use sipha::prelude::*;
use sipha::tree::ast::AstNodeExt;

/// Braced statement list (`K::Block`).
#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::Block)]
pub struct Block(SyntaxNode);

impl Block {
    /// Direct child statements (skips `{`, `}`, and non-`Stmt` members such as `ClassMember`).
    pub fn stmts(&self) -> impl Iterator<Item = super::Stmt> + '_ {
        AstNodeExt::children::<super::Stmt>(self.syntax())
    }
}
