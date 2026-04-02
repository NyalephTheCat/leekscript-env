use crate::syntax::kinds::K;
use sipha::prelude::*;
use sipha::tree::ast::AstNode;

/// Root of a full type from `ls_type` / `lambda_return_type` (`K::TypeExpr`).
///
/// Structure: [`TypeExpr`] → [`TypeUnionType`] → one or more [`TypeNullableType`] (`T | U?`).
/// Each nullable wraps [`TypePrimaryType`] (keyword, `ident`, or `Foo<…>`). See [`super::TypeNode`].
#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::TypeExpr)]
pub struct TypeExpr(SyntaxNode);

impl TypeExpr {
    /// `T | U | …` layer (always present for a parsed `ls_type` / `lambda_return_type`).
    #[must_use]
    pub fn union_type(&self) -> Option<TypeUnionType> {
        self.syntax().child::<TypeUnionType>()
    }
}

/// `integer | real | …` — one or more [`TypeNullableType`] separated by `|`.
#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::TypeUnionType)]
pub struct TypeUnionType(SyntaxNode);

impl TypeUnionType {
    /// Nullable segments in source order (`T`, `U?`, …).
    #[must_use]
    pub fn nullable_members(&self) -> Vec<TypeNullableType> {
        self.syntax()
            .child_nodes()
            .filter_map(|n| TypeNullableType::cast(n))
            .collect()
    }
}

/// `T` or `T?`.
#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::TypeNullableType)]
pub struct TypeNullableType(SyntaxNode);

impl TypeNullableType {
    #[must_use]
    pub fn primary(&self) -> Option<TypePrimaryType> {
        self.syntax().child::<TypePrimaryType>()
    }

    /// `true` when this segment ends with `?` (nullable / optional type).
    #[must_use]
    pub fn is_optional(&self) -> bool {
        self.syntax()
            .child_tokens()
            .any(|t| t.kind_as::<K>() == Some(K::Question))
    }
}

/// A single primary: builtin keyword, `ident`, or generic application (`Array<…>`, `Map<…,…>`, …).
#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::TypePrimaryType)]
pub struct TypePrimaryType(SyntaxNode);

impl TypePrimaryType {
    /// User-defined class / type name (`Foo` in `Foo x`).
    #[must_use]
    pub fn ident_text(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind_as::<K>() == Some(K::Ident))
            .map(|t| t.text().to_string())
    }

    /// Top-level [`TypeExpr`] arguments inside `<…>` for this primary (`Array<integer>` → one;
    /// `Map<K, V>` → two), in source order — direct child [`TypeExpr`] nodes only.
    #[must_use]
    pub fn generic_argument_roots(&self) -> Vec<TypeExpr> {
        self.syntax()
            .child_nodes()
            .filter_map(|n| TypeExpr::cast(n))
            .collect()
    }
}
