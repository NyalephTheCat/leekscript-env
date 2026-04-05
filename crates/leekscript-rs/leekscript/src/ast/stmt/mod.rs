//! Statement AST nodes and the [`Stmt`] sum type.

mod block;
mod control;
mod decl;
mod flow;
mod include;
mod misc;
mod params;
mod template_params;

pub use block::Block;
pub use control::{DoWhileStmt, ForStmt, ForeachStmt, IfStmt, SwitchArm, SwitchStmt, WhileStmt};
pub use decl::{ExprStmt, FunctionDecl, VarDecl};
pub use flow::{BreakStmt, ContinueStmt, ReturnStmt};
pub use include::IncludeStmt;
pub use misc::{
    CatchClause, ClassDecl, ClassMember, ConstDecl, ElseStmt, EmptyStmt, ErrorStmt, ExportStmt,
    GlobalDecl, GotoStmt, ImportStmt, MatchStmt, PackageStmt, ThrowStmt, TryStmt,
};
pub use params::{FnParam, fn_param_children};
pub use template_params::TemplateParams;

use crate::syntax::kinds::Node;
use sipha::AstEnum;

#[derive(Debug, Clone, AstEnum)]
pub enum Stmt {
    #[ast(kind = Node::IncludeStmt)]
    Include(IncludeStmt),
    #[ast(kind = Node::ReturnStmt)]
    Return(ReturnStmt),
    #[ast(kind = Node::BreakStmt)]
    Break(BreakStmt),
    #[ast(kind = Node::ContinueStmt)]
    Continue(ContinueStmt),
    #[ast(kind = Node::VarDecl)]
    VarDecl(VarDecl),
    #[ast(kind = Node::FunctionDecl)]
    Function(FunctionDecl),
    #[ast(kind = Node::Stmt)]
    Expr(ExprStmt),
    #[ast(kind = Node::GlobalDecl)]
    Global(GlobalDecl),
    #[ast(kind = Node::ElseStmt)]
    Else(ElseStmt),
    #[ast(kind = Node::SwitchStmt)]
    Switch(SwitchStmt),
    #[ast(kind = Node::ClassDecl)]
    Class(ClassDecl),
    #[ast(kind = Node::IfStmt)]
    If(IfStmt),
    #[ast(kind = Node::ForStmt)]
    For(ForStmt),
    #[ast(kind = Node::ForeachStmt)]
    Foreach(ForeachStmt),
    #[ast(kind = Node::DoWhileStmt)]
    DoWhile(DoWhileStmt),
    #[ast(kind = Node::WhileStmt)]
    While(WhileStmt),
    #[ast(kind = Node::TryStmt)]
    Try(TryStmt),
    #[ast(kind = Node::ThrowStmt)]
    Throw(ThrowStmt),
    #[ast(kind = Node::ImportStmt)]
    Import(ImportStmt),
    #[ast(kind = Node::ExportStmt)]
    Export(ExportStmt),
    #[ast(kind = Node::GotoStmt)]
    Goto(GotoStmt),
    #[ast(kind = Node::PackageStmt)]
    Package(PackageStmt),
    #[ast(kind = Node::ConstDecl)]
    Const(ConstDecl),
    #[ast(kind = Node::MatchStmt)]
    Match(MatchStmt),
    #[ast(kind = Node::EmptyStmt)]
    Empty(EmptyStmt),
    #[ast(kind = Node::ErrorStmt)]
    Error(ErrorStmt),
}

mod stmt_block;

pub use self::stmt_block::StmtBlock;
