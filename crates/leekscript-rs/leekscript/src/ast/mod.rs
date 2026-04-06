//! Typed CST wrappers for LeekScript (sipha [`AstNode`](sipha::tree::ast::AstNode)).
//!
//! Types are written **before** names (`T x`, e.g. `integer n`, `global string s`), not `x: T`.
//! Function return types still use `->` / `=>` after the parameter list.

pub(crate) mod binding_name;
mod expr;
mod literal;
mod root;
mod stmt;
pub mod types;

pub use expr::{
    AnonFunctionExpr, ArrayExpr, BinaryExpr, BracketMapExpr, BuiltinStringifyExpr,
    BuiltinTypeNameExpr, CallExpr, CastExpr, ClassRefExpr, Expr, ExprRoot, IfExpr, IndexExpr,
    IntervalExpr, LambdaExpr, MemberExpr, NewExpr, ObjectExpr, ParenExpr, SetExpr, SuperExpr,
    TernaryExpr, UnaryExpr, RefExpr,
};
pub use literal::LitStr;
pub use root::Root;
pub use stmt::{
    Block, BreakStmt, CatchClause, ClassDecl, ClassMember, ConstDecl, ContinueStmt, DoWhileStmt,
    ElseStmt, EmptyStmt, ExportStmt, ExprStmt, FnParam, ForStmt, ForeachStmt, FunctionDecl,
    GlobalDecl, GotoStmt, IfStmt, ImportStmt, IncludeStmt, MatchStmt, PackageStmt, ReturnStmt,
    Stmt, StmtBlock, SwitchArm, SwitchStmt, TemplateParams, ThrowStmt, TryStmt, VarDecl, WhileStmt,
    fn_param_children,
};
pub use types::{TypeExpr, TypeNode, TypeNullableType, TypePrimaryType, TypeUnionType};
