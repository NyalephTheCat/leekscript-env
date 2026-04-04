//! Concrete expression CST nodes (one struct per `K::…Expr` syntax kind).

use crate::ast::stmt::TemplateParams;
use crate::syntax::kinds::K;
use sipha::AstNode;
use sipha::prelude::*;

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::BinaryExpr)]
pub struct BinaryExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::UnaryExpr)]
pub struct UnaryExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::TernaryExpr)]
pub struct TernaryExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::CastExpr)]
pub struct CastExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::CallExpr)]
pub struct CallExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::IndexExpr)]
pub struct IndexExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::MemberExpr)]
pub struct MemberExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ParenExpr)]
pub struct ParenExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::LambdaExpr)]
pub struct LambdaExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::BracketMapExpr)]
pub struct BracketMapExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ArrayExpr)]
pub struct ArrayExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ObjectExpr)]
pub struct ObjectExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::SetExpr)]
pub struct SetExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::IntervalExpr)]
pub struct IntervalExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::AnonFunctionExpr)]
pub struct AnonFunctionExpr(SyntaxNode);

impl AnonFunctionExpr {
    /// Leading `<T>` on `function<T>(…) { }` when present (experimental).
    #[must_use]
    pub fn template_params(&self) -> Option<TemplateParams> {
        self.syntax().child::<TemplateParams>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::IfExpr)]
pub struct IfExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::NewExpr)]
pub struct NewExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::SuperExpr)]
pub struct SuperExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::ClassRefExpr)]
pub struct ClassRefExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::BuiltinTypeNameExpr)]
pub struct BuiltinTypeNameExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = K::BuiltinStringifyExpr)]
pub struct BuiltinStringifyExpr(SyntaxNode);
