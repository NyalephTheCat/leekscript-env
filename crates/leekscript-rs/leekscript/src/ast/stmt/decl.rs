use super::Block;
use super::TemplateParams;
use super::params::{FnParam, fn_param_children};
use crate::ast::binding_name::function_decl_name_token;
use crate::ast::expr::Expr;
use crate::ast::types::TypeExpr;
use crate::syntax::{
    ParsedDoxygen, attached_docstring, attached_parsed_doxygen,
    kinds::{Lex, Node},
};
use sipha::prelude::*;
use sipha::tree::ast::AstNode;
use sipha::tree::ast::AstNodeExt;
use sipha::types::IntoSyntaxKind;
#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = Node::VarDecl)]
pub struct VarDecl(SyntaxNode);

impl VarDecl {
    /// Leading Doxygen doc comment (`/** … */`, `/// …`) on this declaration, if any.
    #[must_use]
    pub fn docstring(&self) -> Option<String> {
        attached_docstring(self.syntax())
    }

    /// Parsed Doxygen commands (`\brief`, `@param`, …), if any.
    #[must_use]
    pub fn parsed_docstring(&self) -> Option<ParsedDoxygen> {
        attached_parsed_doxygen(self.syntax())
    }

    /// First declared identifier (after `var` / `let`).
    pub fn first_name(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == Lex::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }
}

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = Node::FunctionDecl)]
pub struct FunctionDecl(SyntaxNode);

impl FunctionDecl {
    /// Leading Doxygen doc comment (`/** … */`, `/// …`) on this declaration, if any.
    #[must_use]
    pub fn docstring(&self) -> Option<String> {
        attached_docstring(self.syntax())
    }

    /// Parsed Doxygen commands (`\brief`, `@param`, …), if any.
    #[must_use]
    pub fn parsed_docstring(&self) -> Option<ParsedDoxygen> {
        attached_parsed_doxygen(self.syntax())
    }

    /// Function name (first binding token after `function`, including keywords allowed by `name`).
    pub fn name(&self) -> Option<String> {
        function_decl_name_token(self.syntax()).map(|t| t.text().to_string())
    }

    /// Template parameters `function name<T, U>(…)` when present (experimental).
    #[must_use]
    pub fn template_params(&self) -> Option<TemplateParams> {
        self.syntax().child::<TemplateParams>()
    }

    /// Result type only when spelled with `->` / `=>` after `)` (not parameter types in `T name` form).
    pub fn return_type(&self) -> Option<TypeExpr> {
        let arrow = Lex::Arrow.into_syntax_kind();
        let mut after_arrow = false;
        for el in self.syntax().children() {
            if crate::syntax::syntax_el_is_trivia(&el) {
                continue;
            }
            if let Some(t) = el.as_token() {
                if t.kind() == arrow {
                    after_arrow = true;
                }
                continue;
            }
            let Some(n) = el.as_node() else {
                continue;
            };
            if Block::can_cast(n.kind()) {
                break;
            }
            if after_arrow {
                if let Some(te) = TypeExpr::cast(n.clone()) {
                    return Some(te);
                }
            }
        }
        None
    }

    pub fn body(&self) -> Option<Block> {
        self.syntax().child::<Block>()
    }

    /// Formal parameters inside `( … )` in source order.
    pub fn fn_params(&self) -> impl Iterator<Item = FnParam> + '_ {
        fn_param_children(self.syntax())
    }
}

/// Expression statement: `expr;` wrapped in `Node::Stmt`.
#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = Node::Stmt)]
pub struct ExprStmt(SyntaxNode);

impl ExprStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }
}
