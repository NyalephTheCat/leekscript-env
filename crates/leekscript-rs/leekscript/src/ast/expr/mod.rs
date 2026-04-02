//! Expression AST nodes and the [`Expr`] sum type.

mod nodes;
mod root;

pub use nodes::{
    AnonFunctionExpr, ArrayExpr, BinaryExpr, BracketMapExpr, BuiltinStringifyExpr,
    BuiltinTypeNameExpr, CallExpr, CastExpr, ClassRefExpr, IfExpr, IndexExpr, IntervalExpr,
    LambdaExpr, MemberExpr, NewExpr, ObjectExpr, ParenExpr, SetExpr, SuperExpr, TernaryExpr,
    UnaryExpr,
};
pub use root::ExprRoot;

use crate::syntax::kinds::K;
use sipha::AstEnum;

/// Any expression CST node: the `expr` rule root (`K::Expr`) or a nested expression shape.
#[derive(Debug, Clone, AstEnum)]
pub enum Expr {
    #[ast(kind = K::Expr)]
    Root(ExprRoot),
    #[ast(kind = K::BinaryExpr)]
    Binary(BinaryExpr),
    #[ast(kind = K::UnaryExpr)]
    Unary(UnaryExpr),
    #[ast(kind = K::TernaryExpr)]
    Ternary(TernaryExpr),
    #[ast(kind = K::CastExpr)]
    Cast(CastExpr),
    #[ast(kind = K::CallExpr)]
    Call(CallExpr),
    #[ast(kind = K::IndexExpr)]
    Index(IndexExpr),
    #[ast(kind = K::MemberExpr)]
    Member(MemberExpr),
    #[ast(kind = K::ParenExpr)]
    Paren(ParenExpr),
    #[ast(kind = K::LambdaExpr)]
    Lambda(LambdaExpr),
    #[ast(kind = K::BracketMapExpr)]
    BracketMap(BracketMapExpr),
    #[ast(kind = K::ArrayExpr)]
    Array(ArrayExpr),
    #[ast(kind = K::ObjectExpr)]
    Object(ObjectExpr),
    #[ast(kind = K::SetExpr)]
    Set(SetExpr),
    #[ast(kind = K::IntervalExpr)]
    Interval(IntervalExpr),
    #[ast(kind = K::AnonFunctionExpr)]
    AnonFunction(AnonFunctionExpr),
    #[ast(kind = K::IfExpr)]
    If(IfExpr),
    #[ast(kind = K::NewExpr)]
    New(NewExpr),
    #[ast(kind = K::SuperExpr)]
    Super(SuperExpr),
    #[ast(kind = K::ClassRefExpr)]
    ClassRef(ClassRefExpr),
    #[ast(kind = K::BuiltinTypeNameExpr)]
    BuiltinTypeName(BuiltinTypeNameExpr),
    #[ast(kind = K::BuiltinStringifyExpr)]
    BuiltinStringify(BuiltinStringifyExpr),
}
