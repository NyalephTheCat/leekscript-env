use crate::syntax::kinds::K;
use sipha::AstNode;
use sipha::prelude::*;

/// Full expression from the `expr` / `assign` rule (`K::Expr`).
#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::Expr)]
pub struct ExprRoot(SyntaxNode);
