//! Typed CST wrappers for LeekScript (sipha [`AstNode`](sipha::tree::ast::AstNode)).
//!
//! Types are written **before** names (`T x`, e.g. `integer n`, `global string s`), not `x: T`.
//! Function return types still use `->` / `=>` after the parameter list.

mod expr;
mod literal;
mod root;
mod stmt;
mod type_expr;

pub use expr::Expr;
pub use literal::LitStr;
pub use root::Root;
pub use stmt::{
    Block, BreakStmt, CatchClause, ClassDecl, ConstDecl, ContinueStmt, DoWhileStmt, ElseStmt,
    EmptyStmt, ExportStmt, ExprStmt, ForStmt, ForeachStmt, FunctionDecl, GlobalDecl, GotoStmt,
    IfStmt, ImportStmt, IncludeStmt, MatchStmt, PackageStmt, ReturnStmt, Stmt, StmtBlock,
    SwitchArm, SwitchStmt, ThrowStmt, TryStmt, VarDecl, WhileStmt,
};
pub use type_expr::TypeExpr;
