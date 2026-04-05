use crate::syntax::kinds::Node;
use sipha::AstNode;
use sipha::prelude::*;

/// Full expression from the `expr` / `assign` rule (`Node::Expr`).
#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::Expr)]
pub struct ExprRoot(SyntaxNode);
