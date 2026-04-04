//! Experimental template parameter lists (`<T, U>`) on declarations.

use crate::Span;
use crate::syntax::kinds::K;
use sipha::AstNode;
use sipha::prelude::*;

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::TemplateParams)]
pub struct TemplateParams(SyntaxNode);

impl TemplateParams {
    /// Type parameter names in source order (inside `<` … `>`).
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.syntax()
            .child_tokens()
            .filter(|t| t.kind_as::<K>() == Some(K::Ident))
            .map(|t| t.text().to_string())
            .collect()
    }

    /// Each template parameter name and the span of its identifier token.
    #[must_use]
    pub fn name_spans(&self) -> Vec<(String, Span)> {
        self.syntax()
            .child_tokens()
            .filter(|t| t.kind_as::<K>() == Some(K::Ident))
            .map(|t| (t.text().to_string(), t.text_range()))
            .collect()
    }
}
