use std::collections::{HashMap, HashSet};

use crate::Span;
use crate::scope::leek_ty::LeekTy;
use crate::scope::model::{ExprTypeKey, Scope, ScopeId, ScopeKind, Symbol, SymbolId, SymbolKind};
use crate::syntax::ParsedDoxygen;

use super::phase::AnalysisPhase;

/// Scopes, symbols, and declaration bookkeeping shared across analysis phases.
pub(crate) struct ScopeGraph {
    pub scopes: Vec<Scope>,
    pub symbols: Vec<Symbol>,
    next_scope_id: u32,
    next_symbol_id: u32,
    /// Span keys for binding identifiers (skip treating them as value reads in phase 2).
    pub binding_spans: HashSet<ExprTypeKey>,
}

impl ScopeGraph {
    pub(crate) fn new() -> Self {
        Self {
            scopes: Vec::new(),
            symbols: Vec::new(),
            next_scope_id: 0,
            next_symbol_id: 0,
            binding_spans: HashSet::new(),
        }
    }

    pub(crate) fn alloc_scope(&mut self, parent: Option<ScopeId>, kind: ScopeKind) -> ScopeId {
        let id = ScopeId(self.next_scope_id);
        self.next_scope_id += 1;
        self.scopes.push(Scope {
            id,
            parent,
            kind,
            symbols: HashMap::new(),
        });
        id
    }

    pub(crate) fn declare(
        &mut self,
        phase: AnalysisPhase,
        scope_id: ScopeId,
        name: String,
        name_span: Span,
        kind: SymbolKind,
        declared_ty: Option<LeekTy>,
        doc: Option<ParsedDoxygen>,
    ) -> SymbolId {
        let id = SymbolId(self.next_symbol_id);
        self.next_symbol_id += 1;
        let sym = Symbol {
            id,
            scope_id,
            kind,
            name: name.clone(),
            name_span,
            declared_ty,
            inferred_ty: None,
            doc,
        };
        let sid = id.0 as usize;
        if sid >= self.symbols.len() {
            self.symbols.resize(
                sid + 1,
                Symbol {
                    id: SymbolId(0),
                    scope_id: ScopeId(0),
                    kind: SymbolKind::Variable,
                    name: String::new(),
                    name_span: Span::new(0, 0),
                    declared_ty: None,
                    inferred_ty: None,
                    doc: None,
                },
            );
        }
        self.symbols[sid] = sym;
        if let Some(sc) = self.scopes.get_mut(scope_id.0 as usize) {
            sc.symbols.insert(name, id);
        }
        if phase.is_build_scopes() {
            self.binding_spans.insert(ExprTypeKey::from_span(name_span));
        }
        id
    }
}
