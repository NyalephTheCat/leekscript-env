use super::leek_ty::LeekTy;
use crate::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Function,
    Block,
    Class,
    Method,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Variable,
    Parameter,
    Function,
    Global,
    Class,
    Field,
    Method,
    /// `constructor (...) { }`
    Constructor,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: SymbolId,
    pub scope_id: ScopeId,
    pub kind: SymbolKind,
    pub name: String,
    pub name_span: Span,
    pub declared_ty: Option<LeekTy>,
    /// Filled by inference when there is no annotation or to refine `Unknown`.
    pub inferred_ty: Option<LeekTy>,
}

#[derive(Clone, Debug)]
pub struct Scope {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    /// Last declaration wins for duplicate names in the same scope.
    pub symbols: std::collections::HashMap<String, SymbolId>,
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub name: String,
    pub span: Span,
    pub resolved: Option<SymbolId>,
}

#[derive(Clone, Debug)]
pub struct SemanticDiagnostic {
    pub message: String,
    pub span: Span,
}
