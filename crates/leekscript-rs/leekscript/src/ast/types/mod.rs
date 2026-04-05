//! Type-syntax CST nodes (`Node::TypeExpr` and structured layers).

mod type_expr;

pub use type_expr::{TypeExpr, TypeNullableType, TypePrimaryType, TypeUnionType};

use crate::syntax::kinds::Node;
use sipha::AstEnum;

/// Any type-syntax CST node (full type or a structural layer).
#[derive(Debug, Clone, AstEnum)]
pub enum TypeNode {
    #[ast(kind = Node::TypeExpr)]
    Root(TypeExpr),
    #[ast(kind = Node::TypeUnionType)]
    Union(TypeUnionType),
    #[ast(kind = Node::TypeNullableType)]
    Nullable(TypeNullableType),
    #[ast(kind = Node::TypePrimaryType)]
    Primary(TypePrimaryType),
}
