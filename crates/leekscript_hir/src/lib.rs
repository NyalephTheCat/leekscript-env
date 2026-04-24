//! High-level intermediate representation for LeekScript.
//!
//! Produced by lowering a grammar-shaped rowan tree ([`leekscript_syntax`]) after a successful parse.
//! Intended for future codegen, interpretation, and semantic analysis.

mod lower;
mod nodes;
pub mod refs;

pub use lower::{lower_file, HirLoweringDiagnostic};
pub use nodes::{
    HirAssignOp, HirBinOp, HirClassMember, HirExpr, HirFieldVisibility, HirFile, HirForStep,
    HirForUpdate, HirParam, HirStmt, HirSwitchClause, HirTypeExpr, HirUnaryOp, NameDef,
};
