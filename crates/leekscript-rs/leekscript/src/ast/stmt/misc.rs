use super::Block;
use crate::ast::expr::Expr;
use crate::ast::literal::LitStr;
use crate::ast::types::TypeExpr;
use crate::syntax::kinds::K;
use sipha::AstNode;
use sipha::prelude::*;
use sipha::tree::ast::{AstNodeExt, AstTokenExt};
use sipha::types::IntoSyntaxKind;

/// Empty statement: a single `;`.
#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::EmptyStmt)]
pub struct EmptyStmt(SyntaxNode);

/// Parse error placeholder (recovery mode); empty CST node at the error offset.
#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ErrorStmt)]
pub struct ErrorStmt(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::GlobalDecl)]
pub struct GlobalDecl(SyntaxNode);

impl GlobalDecl {
    /// Optional type in `T name` form (`global integer x` — not `global x: integer`).
    pub fn type_expr(&self) -> Option<TypeExpr> {
        self.syntax().child::<TypeExpr>()
    }

    pub fn first_name(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == K::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ClassDecl)]
pub struct ClassDecl(SyntaxNode);

impl ClassDecl {
    /// Class name (`class` … `{`); stops before `extends` if present.
    pub fn name(&self) -> Option<String> {
        let ext = K::ExtendsKw.into_syntax_kind();
        let id = K::Ident.into_syntax_kind();
        for t in self.syntax().child_tokens() {
            let k = t.kind();
            if k == ext {
                break;
            }
            if k == id {
                return Some(t.text().to_string());
            }
        }
        None
    }

    /// Superclass name after `extends`, if any.
    pub fn extends(&self) -> Option<String> {
        let ext = K::ExtendsKw.into_syntax_kind();
        let id = K::Ident.into_syntax_kind();
        let mut after_extends = false;
        for t in self.syntax().child_tokens() {
            let k = t.kind();
            if k == ext {
                after_extends = true;
                continue;
            }
            if after_extends && k == id {
                return Some(t.text().to_string());
            }
        }
        None
    }

    pub fn body(&self) -> Option<Block> {
        self.syntax().child::<Block>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ConstDecl)]
pub struct ConstDecl(SyntaxNode);

impl ConstDecl {
    pub fn first_name(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == K::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ElseStmt)]
pub struct ElseStmt(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::TryStmt)]
pub struct TryStmt(SyntaxNode);

impl TryStmt {
    pub fn try_block(&self) -> Option<Block> {
        self.syntax().child::<Block>()
    }

    pub fn catch_clauses(&self) -> impl Iterator<Item = CatchClause> + '_ {
        AstNodeExt::children::<CatchClause>(self.syntax())
    }

    /// `finally { ... }` when present (second direct `Block` after the `try` body).
    pub fn finally_block(&self) -> Option<Block> {
        let mut it = AstNodeExt::children::<Block>(self.syntax());
        let _try_body = it.next()?;
        it.next()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::CatchClause)]
pub struct CatchClause(SyntaxNode);

impl CatchClause {
    /// `T` in `catch (T name) { … }` (type before the binding).
    pub fn param_type(&self) -> Option<TypeExpr> {
        self.syntax().child::<TypeExpr>()
    }

    pub fn param_name(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == K::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }

    pub fn block(&self) -> Option<Block> {
        self.syntax().child::<Block>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ThrowStmt)]
pub struct ThrowStmt(SyntaxNode);

impl ThrowStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ImportStmt)]
pub struct ImportStmt(SyntaxNode);

impl ImportStmt {
    pub fn string_path(&self) -> Option<LitStr> {
        self.syntax().token_ast::<LitStr>()
    }

    /// `import foo.bar` segments when not using a string literal.
    pub fn name_segments(&self) -> Option<Vec<String>> {
        if self.string_path().is_some() {
            return None;
        }
        let segs: Vec<_> = self
            .syntax()
            .child_tokens()
            .filter(|t| t.kind() == K::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
            .collect();
        if segs.is_empty() { None } else { Some(segs) }
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ExportStmt)]
pub struct ExportStmt(SyntaxNode);

impl ExportStmt {
    pub fn block(&self) -> Option<Block> {
        self.syntax().child::<Block>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::GotoStmt)]
pub struct GotoStmt(SyntaxNode);

impl GotoStmt {
    pub fn label(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == K::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::PackageStmt)]
pub struct PackageStmt(SyntaxNode);

impl PackageStmt {
    pub fn segments(&self) -> impl Iterator<Item = String> + '_ {
        self.syntax()
            .child_tokens()
            .filter(|t| t.kind() == K::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }

    /// `a.b.c` as declared after `package`.
    pub fn qualified_name(&self) -> String {
        self.segments().collect::<Vec<_>>().join(".")
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::MatchStmt)]
pub struct MatchStmt(SyntaxNode);

impl MatchStmt {
    pub fn scrutinee(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }
}
