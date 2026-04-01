use crate::ast::expr::Expr;
use crate::syntax::kinds::K;
use sipha::AstNode;
use sipha::prelude::*;
use sipha::tree::ast::AstNodeExt;
use sipha::types::IntoSyntaxKind;

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ReturnStmt)]
pub struct ReturnStmt(SyntaxNode);

impl ReturnStmt {
    /// `return` value, if any (`return;` omits this).
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::BreakStmt)]
pub struct BreakStmt(SyntaxNode);

impl BreakStmt {
    /// Optional break level (`break 2`). `None` if omitted or if the literal is not a plain decimal integer.
    pub fn level(&self) -> Option<u32> {
        self.syntax()
            .token(K::Number.into_syntax_kind())
            .and_then(|t| t.text().parse().ok())
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ContinueStmt)]
pub struct ContinueStmt(SyntaxNode);

impl ContinueStmt {
    /// Optional continue level (`continue 2`). `None` if omitted or if the literal is not a plain decimal integer.
    pub fn level(&self) -> Option<u32> {
        self.syntax()
            .token(K::Number.into_syntax_kind())
            .and_then(|t| t.text().parse().ok())
    }
}
