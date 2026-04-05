use crate::syntax::kinds::Node;
use sipha::AstNode;
use sipha::prelude::*;

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::Root)]
pub struct Root(SyntaxNode);
