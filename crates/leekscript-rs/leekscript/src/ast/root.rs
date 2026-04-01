use crate::syntax::kinds::K;
use sipha::AstNode;
use sipha::prelude::*;

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::Root)]
pub struct Root(SyntaxNode);
