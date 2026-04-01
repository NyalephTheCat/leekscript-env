use crate::ast::literal::LitStr;
use crate::syntax::kinds::K;
use sipha::AstNode;
use sipha::prelude::*;
use sipha::types::IntoSyntaxKind;

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::IncludeStmt)]
pub struct IncludeStmt(SyntaxNode);

impl IncludeStmt {
    pub fn path(&self) -> Option<LitStr> {
        self.syntax()
            .token(K::String.into_syntax_kind())
            .map(LitStr::new)
    }
}
