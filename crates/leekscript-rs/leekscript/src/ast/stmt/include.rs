use crate::ast::literal::LitStr;
use crate::syntax::kinds::K;
use sipha::AstNode;
use sipha::prelude::*;
use sipha::tree::ast::AstTokenExt;

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::IncludeStmt)]
pub struct IncludeStmt(SyntaxNode);

impl IncludeStmt {
    pub fn path(&self) -> Option<LitStr> {
        self.syntax().token_ast::<LitStr>()
    }
}
