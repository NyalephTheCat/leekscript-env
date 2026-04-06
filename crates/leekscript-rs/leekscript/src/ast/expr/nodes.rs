//! Concrete expression CST nodes (one struct per `K::…Expr` syntax kind).

use crate::ast::stmt::TemplateParams;
use crate::syntax::kinds::Node;
use sipha::AstNode;
use sipha::prelude::*;

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::BinaryExpr)]
pub struct BinaryExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::UnaryExpr)]
pub struct UnaryExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::TernaryExpr)]
pub struct TernaryExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::CastExpr)]
pub struct CastExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::CallExpr)]
pub struct CallExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::IndexExpr)]
pub struct IndexExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::MemberExpr)]
pub struct MemberExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ParenExpr)]
pub struct ParenExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::LambdaExpr)]
pub struct LambdaExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::BracketMapExpr)]
pub struct BracketMapExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ArrayExpr)]
pub struct ArrayExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ObjectExpr)]
pub struct ObjectExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::SetExpr)]
pub struct SetExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::IntervalExpr)]
pub struct IntervalExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::AnonFunctionExpr)]
pub struct AnonFunctionExpr(SyntaxNode);

impl AnonFunctionExpr {
    /// Leading `<T>` on `function<T>(…) { }` when present (experimental).
    #[must_use]
    pub fn template_params(&self) -> Option<TemplateParams> {
        self.syntax().child::<TemplateParams>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::IfExpr)]
pub struct IfExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::NewExpr)]
pub struct NewExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::SuperExpr)]
pub struct SuperExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ClassRefExpr)]
pub struct ClassRefExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::BuiltinTypeNameExpr)]
pub struct BuiltinTypeNameExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::BuiltinStringifyExpr)]
pub struct BuiltinStringifyExpr(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::RefExpr)]
pub struct RefExpr(SyntaxNode);
