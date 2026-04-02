//! Type-syntax CST nodes (`K::TypeExpr` and structured layers).

mod type_expr;

pub use type_expr::{TypeExpr, TypeNullableType, TypePrimaryType, TypeUnionType};

use crate::syntax::kinds::K;
use sipha::AstEnum;

/// Any type-syntax CST node (full type or a structural layer).
#[derive(Debug, Clone, AstEnum)]
pub enum TypeNode {
    #[ast(kind = K::TypeExpr)]
    Root(TypeExpr),
    #[ast(kind = K::TypeUnionType)]
    Union(TypeUnionType),
    #[ast(kind = K::TypeNullableType)]
    Nullable(TypeNullableType),
    #[ast(kind = K::TypePrimaryType)]
    Primary(TypePrimaryType),
}
