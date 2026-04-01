use crate::syntax::kinds::K;
use sipha::AstNode;
use sipha::prelude::*;

/// Any expression node (`K::Expr`).
#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::Expr)]
pub struct Expr(SyntaxNode);
