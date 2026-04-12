use super::leek_ty::LeekTy;
use crate::Span;
use crate::syntax::ParsedDoxygen;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

/// Byte span used as a key for per-expression inferred types in [`crate::scope::AnalysisResult::expr_types`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExprTypeKey {
    pub start: u32,
    pub end: u32,
}

impl ExprTypeKey {
    #[must_use]
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub fn from_span(span: Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

impl From<Span> for ExprTypeKey {
    fn from(span: Span) -> Self {
        Self::from_span(span)
    }
}

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
    /// Type parameter from a declaration’s `<T, U>` list (experimental templates).
    TypeParam,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: SymbolId,
    pub scope_id: ScopeId,
    pub kind: SymbolKind,
    pub name: String,
    pub name_span: Span,
    /// `static` class field / method (only meaningful in class body scope).
    pub is_static: bool,
    pub declared_ty: Option<LeekTy>,
    /// Filled by inference when there is no annotation or to refine `Unknown`.
    pub inferred_ty: Option<LeekTy>,
    /// Parsed Doxygen comment (`/** … */`, `/// …`), if any.
    pub doc: Option<ParsedDoxygen>,
}

impl Symbol {
    /// Declared type, else inferred, else [`LeekTy::Unknown`].
    #[must_use]
    pub fn effective_ty(&self) -> LeekTy {
        self.declared_ty
            .clone()
            .or_else(|| self.inferred_ty.clone())
            .unwrap_or(LeekTy::Unknown)
    }

    /// Raw attached doc text (full comment body after comment markers).
    #[must_use]
    pub fn doc_raw(&self) -> Option<&str> {
        self.doc.as_ref().map(|d| d.raw.as_str())
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticCode {
    UndefinedName,
    IncompatibleInitializer,
    /// `===` / `!==` (deprecated since LeekScript 4 in the reference compiler).
    DeprecatedStrictEquality,
    /// Call to a function / global / method whose doc marks `@deprecated`.
    DeprecatedCallable,
    /// Member access, indexing, or call on a value that may be null (`T?`).
    NullableChainAccess,
    /// `return` with no value must be written as `return;`, not a bare `return` without `;`.
    BareReturnRequiresSemicolon,
}

/// Whether a diagnostic should fail `leekscript check` or only warn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SemanticSeverity {
    #[default]
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct SemanticDiagnostic {
    pub severity: SemanticSeverity,
    pub code: SemanticCode,
    pub message: String,
    pub span: Span,
    /// Optional second highlight (e.g. type annotation vs initializer).
    pub related_span: Option<Span>,
}
