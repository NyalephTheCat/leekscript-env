use crate::ast::expr::Expr;
use crate::syntax::kinds::{Lex, Node};
use crate::syntax::syntax_el_is_trivia;
use sipha::AstNode;
use sipha::prelude::*;
use sipha::tree::ast::AstNodeExt;
use sipha::tree::red::SyntaxElement;
use sipha::types::IntoSyntaxKind;

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ReturnStmt)]
pub struct ReturnStmt(SyntaxNode);

impl ReturnStmt {
    /// `return` value, if any (`return;` omits this).
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }

    /// `return? expr` — only return when `expr` is truthy (Java LeekScript conditional return).
    #[must_use]
    pub fn is_optional(&self) -> bool {
        self.syntax().children().any(|el| {
            if syntax_el_is_trivia(&el) {
                return false;
            }
            matches!(
                &el,
                SyntaxElement::Token(t) if Lex::from_syntax_kind(t.kind()) == Some(Lex::Question)
            )
        })
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::BreakStmt)]
pub struct BreakStmt(SyntaxNode);

impl BreakStmt {
    /// Optional break level (`break 2`). `None` if omitted or if the literal is not a plain decimal integer.
    pub fn level(&self) -> Option<u32> {
        self.syntax()
            .token(Lex::Number.into_syntax_kind())
            .and_then(|t| t.text().parse().ok())
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ContinueStmt)]
pub struct ContinueStmt(SyntaxNode);

impl ContinueStmt {
    /// Optional continue level (`continue 2`). `None` if omitted or if the literal is not a plain decimal integer.
    pub fn level(&self) -> Option<u32> {
        self.syntax()
            .token(Lex::Number.into_syntax_kind())
            .and_then(|t| t.text().parse().ok())
    }
}
