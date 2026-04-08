//! Formal parameter nodes (`Node::FnParam`) shared by function and method headers.

use crate::Span;
use crate::ast::binding_name::fn_param_binding_token;
use crate::ast::expr::Expr;
use crate::ast::types::TypeExpr;
use crate::syntax::kinds::Node;
use sipha::prelude::*;
use sipha::tree::ast::{AstNode, AstNodeExt};

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = Node::FnParam)]
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
        fn_param_binding_token(self.syntax()).map(|t| t.text().to_string())
    }

    #[must_use]
    pub fn name_span(&self) -> Option<Span> {
        fn_param_binding_token(self.syntax()).map(|t| t.text_range())
    }

    /// Default value after `=` when present (`method` parameters, or top-level / anonymous `function` in LSv4).
    #[must_use]
    pub fn default_expr(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }
}

/// Iterate direct `FnParam` children of a syntax node (e.g. [`FunctionDecl`](super::FunctionDecl)).
#[must_use]
pub fn fn_param_children(node: &SyntaxNode) -> impl Iterator<Item = FnParam> + '_ {
    AstNodeExt::children::<FnParam>(node)
}
