use super::Block;
use crate::ast::expr::Expr;
use crate::ast::type_expr::TypeExpr;
use crate::syntax::kinds::K;
use sipha::prelude::*;
use sipha::tree::ast::AstNode;
use sipha::tree::ast::AstNodeExt;
use sipha::types::IntoSyntaxKind;
#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::VarDecl)]
pub struct VarDecl(SyntaxNode);

impl VarDecl {
    /// First declared identifier (after `var` / `let`).
    pub fn first_name(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == K::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }
}

#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::FunctionDecl)]
pub struct FunctionDecl(SyntaxNode);

impl FunctionDecl {
    /// Function name (first `ident` after `function`).
    pub fn name(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == K::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }

    /// Result type only when spelled with `->` / `=>` after `)` (not parameter types in `T name` form).
    pub fn return_type(&self) -> Option<TypeExpr> {
        let arrow = K::Arrow.into_syntax_kind();
        let mut after_arrow = false;
        for el in self.syntax().children() {
            if el.is_trivia() {
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
}

/// Expression statement: `expr;` wrapped in `K::Stmt`.
#[derive(Debug, Clone, sipha::AstNode)]
#[ast(kind = K::Stmt)]
pub struct ExprStmt(SyntaxNode);

impl ExprStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }
}
