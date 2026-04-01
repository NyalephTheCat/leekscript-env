use crate::syntax::kinds::K;
use sipha::prelude::*;

/// Type syntax T, T?, T|U,...
#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::TypeExpr)]
pub struct TypeExpr(SyntaxNode);
