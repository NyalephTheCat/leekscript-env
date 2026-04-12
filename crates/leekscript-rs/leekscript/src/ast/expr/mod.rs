//! Expression AST nodes and the [`Expr`] sum type.

mod nodes;
mod root;

pub use nodes::{
    AnonFunctionExpr, ArrayExpr, BinaryExpr, BracketMapExpr, BuiltinStringifyExpr,
    BuiltinTypeNameExpr, CallExpr, CastExpr, ClassRefExpr, IfExpr, IndexExpr, IntervalExpr,
    LambdaExpr, MemberExpr, NewExpr, ObjectExpr, ParenExpr, RefExpr, SetExpr, SuperExpr,
    TernaryExpr, UnaryExpr,
};
pub use root::ExprRoot;

use crate::syntax::kinds::Node;
use sipha::AstEnum;

/// Any expression CST node: the `expr` rule root (`Node::Expr`) or a nested expression shape.
#[derive(Debug, Clone, AstEnum)]
pub enum Expr {
    #[ast(kind = Node::Expr)]
    Root(ExprRoot),
    #[ast(kind = Node::BinaryExpr)]
    Binary(BinaryExpr),
    #[ast(kind = Node::UnaryExpr)]
    Unary(UnaryExpr),
    #[ast(kind = Node::TernaryExpr)]
    Ternary(TernaryExpr),
    #[ast(kind = Node::CastExpr)]
    Cast(CastExpr),
    #[ast(kind = Node::CallExpr)]
    Call(CallExpr),
    #[ast(kind = Node::IndexExpr)]
    Index(IndexExpr),
    #[ast(kind = Node::MemberExpr)]
    Member(MemberExpr),
    #[ast(kind = Node::ParenExpr)]
    Paren(ParenExpr),
    #[ast(kind = Node::LambdaExpr)]
    Lambda(LambdaExpr),
    #[ast(kind = Node::BracketMapExpr)]
    BracketMap(BracketMapExpr),
    #[ast(kind = Node::ArrayExpr)]
    Array(ArrayExpr),
    #[ast(kind = Node::ObjectExpr)]
    Object(ObjectExpr),
    #[ast(kind = Node::SetExpr)]
    Set(SetExpr),
    #[ast(kind = Node::IntervalExpr)]
    Interval(IntervalExpr),
    #[ast(kind = Node::AnonFunctionExpr)]
    AnonFunction(AnonFunctionExpr),
    #[ast(kind = Node::IfExpr)]
    If(IfExpr),
    #[ast(kind = Node::NewExpr)]
    New(NewExpr),
    #[ast(kind = Node::SuperExpr)]
    Super(SuperExpr),
    #[ast(kind = Node::ClassRefExpr)]
    ClassRef(ClassRefExpr),
    #[ast(kind = Node::BuiltinTypeNameExpr)]
    BuiltinTypeName(BuiltinTypeNameExpr),
    #[ast(kind = Node::BuiltinStringifyExpr)]
    BuiltinStringify(BuiltinStringifyExpr),
    #[ast(kind = Node::RefExpr)]
    Ref(RefExpr),
}
