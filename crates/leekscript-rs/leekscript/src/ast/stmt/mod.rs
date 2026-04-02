//! Statement AST nodes and the [`Stmt`] sum type.

mod block;
mod control;
mod decl;
mod flow;
mod include;
mod misc;

pub use block::Block;
pub use control::{DoWhileStmt, ForStmt, ForeachStmt, IfStmt, SwitchArm, SwitchStmt, WhileStmt};
pub use decl::{ExprStmt, FunctionDecl, VarDecl};
pub use flow::{BreakStmt, ContinueStmt, ReturnStmt};
pub use include::IncludeStmt;
pub use misc::{
    CatchClause, ClassDecl, ConstDecl, ElseStmt, EmptyStmt, ExportStmt, GlobalDecl, GotoStmt,
    ImportStmt, MatchStmt, PackageStmt, ThrowStmt, TryStmt,
};

use crate::syntax::kinds::K;
use sipha::AstEnum;

#[derive(Debug, Clone, AstEnum)]
pub enum Stmt {
    #[ast(kind = K::IncludeStmt)]
    Include(IncludeStmt),
    #[ast(kind = K::ReturnStmt)]
    Return(ReturnStmt),
    #[ast(kind = K::BreakStmt)]
    Break(BreakStmt),
    #[ast(kind = K::ContinueStmt)]
    Continue(ContinueStmt),
    #[ast(kind = K::VarDecl)]
    VarDecl(VarDecl),
    #[ast(kind = K::FunctionDecl)]
    Function(FunctionDecl),
    #[ast(kind = K::Stmt)]
    Expr(ExprStmt),
    #[ast(kind = K::GlobalDecl)]
    Global(GlobalDecl),
    #[ast(kind = K::ElseStmt)]
    Else(ElseStmt),
    #[ast(kind = K::SwitchStmt)]
    Switch(SwitchStmt),
    #[ast(kind = K::ClassDecl)]
    Class(ClassDecl),
    #[ast(kind = K::IfStmt)]
    If(IfStmt),
    #[ast(kind = K::ForStmt)]
    For(ForStmt),
    #[ast(kind = K::ForeachStmt)]
    Foreach(ForeachStmt),
    #[ast(kind = K::DoWhileStmt)]
    DoWhile(DoWhileStmt),
    #[ast(kind = K::WhileStmt)]
    While(WhileStmt),
    #[ast(kind = K::TryStmt)]
    Try(TryStmt),
    #[ast(kind = K::ThrowStmt)]
    Throw(ThrowStmt),
    #[ast(kind = K::ImportStmt)]
    Import(ImportStmt),
    #[ast(kind = K::ExportStmt)]
    Export(ExportStmt),
    #[ast(kind = K::GotoStmt)]
    Goto(GotoStmt),
    #[ast(kind = K::PackageStmt)]
    Package(PackageStmt),
    #[ast(kind = K::ConstDecl)]
    Const(ConstDecl),
    #[ast(kind = K::MatchStmt)]
    Match(MatchStmt),
    #[ast(kind = K::EmptyStmt)]
    Empty(EmptyStmt),
}

mod stmt_block;

pub use self::stmt_block::StmtBlock;
