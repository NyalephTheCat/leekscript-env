//! Lexical scopes, symbol tables, reference resolution, and lightweight type inference.
//!
//! Built with the sipha [`Visitor`](crate::visit::Visitor) over the CST. Run
//! [`run_semantic_analysis`](analysis::run_semantic_analysis) on a syntax root after parsing.
//!
//! Types use [`LeekTy`](leek_ty::LeekTy); assignment compatibility uses [`LeekTy::is_assignable_to`].

mod analysis;
mod extract;
mod leek_ty;
mod model;

pub use crate::syntax::{DoxygenParam, DoxygenRetval, DoxygenThrows, ParsedDoxygen, parse_doxygen};
pub use analysis::{AnalysisResult, run_semantic_analysis};
pub use leek_ty::LeekTy;
pub use model::{
    ExprTypeKey, Reference, Scope, ScopeId, ScopeKind, SemanticCode, SemanticDiagnostic,
    SemanticSeverity, Symbol, SymbolId, SymbolKind,
};
