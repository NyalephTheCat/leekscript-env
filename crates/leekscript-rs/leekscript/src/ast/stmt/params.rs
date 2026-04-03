//! Formal parameter nodes (`K::FnParam`) shared by function and method headers.

use crate::Span;
use crate::ast::types::TypeExpr;
use crate::syntax::kinds::K;
use sipha::prelude::*;
use sipha::tree::ast::{AstNode, AstNodeExt};
use sipha::types::IntoSyntaxKind;

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::FnParam)]
pub struct FnParam(SyntaxNode);

impl FnParam {
    /// Optional type in `T name` form.
    #[must_use]
    pub fn type_expr(&self) -> Option<TypeExpr> {
        self.syntax().child::<TypeExpr>()
    }

    /// Binding identifier (after optional `@`).
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == K::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }

    #[must_use]
    pub fn name_span(&self) -> Option<Span> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == K::Ident.into_syntax_kind())
            .map(|t| t.text_range())
    }
}

/// Iterate direct `FnParam` children of a syntax node (e.g. [`FunctionDecl`](super::FunctionDecl)).
#[must_use]
pub fn fn_param_children(node: &SyntaxNode) -> impl Iterator<Item = FnParam> + '_ {
    AstNodeExt::children::<FnParam>(node)
}
