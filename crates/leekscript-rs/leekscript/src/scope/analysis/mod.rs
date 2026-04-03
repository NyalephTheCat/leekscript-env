//! Visitor-based scope construction, reference resolution, and light type inference.
//!
//! ## Two-phase walk
//!
//! 1. **`AnalysisPhase::BuildScopes`** — `WalkOptions::nodes_only()` (no tokens). Allocates the
//!    module scope first, then on each relevant CST node pushes child scopes in a fixed order and
//!    declares symbols. Every `push_child_scope` in this phase records that [`ScopeId`] in order
//!    for replay in phase 2.
//! 2. **`AnalysisPhase::ResolveAndInfer`** — full walk (nodes + tokens). Does **not** allocate new
//!    scopes: each `push_child_scope` **replays** the next id from the same `scope_push_order`
//!    sequence so stack structure matches phase 1 exactly.
//!
//! Between phases, [`ScopeGraph::binding_spans`] is preserved (moved back after
//! `std::mem::take` so phase 1 can be collected without cloning the set). Other transient walk
//! state (`scope_stack`, `pending_class_body`, `skip_leave_block_span`) is reset before phase 2.
//!
//! Phase 2 records [`AnalysisResult::expr_types`] and applies control-flow narrowing for
//! `instanceof`, `!= null` / `null !=`, and `&&` in `if` / `while` bodies.

mod analyzer;
mod condition;
mod deprecations;
mod enter_leave;
mod graph;
mod infer;
mod narrowing;
mod narrowing_env;
mod phase;
mod spans;
mod visitor;

use std::collections::HashMap;

use sipha::tree::red::SyntaxNode;

use crate::Span;
use crate::parse::Version;
use crate::scope::leek_ty::LeekTy;
use crate::scope::model::{
    ExprTypeKey, Reference, Scope, ScopeId, SemanticDiagnostic, Symbol, SymbolId,
};

use analyzer::Analyzer;

/// Full result of [`run_semantic_analysis`].
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub scopes: Vec<Scope>,
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    pub expr_types: HashMap<ExprTypeKey, LeekTy>,
    pub diagnostics: Vec<SemanticDiagnostic>,
}

impl AnalysisResult {
    #[must_use]
    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id.0 as usize)
    }

    #[must_use]
    pub fn expr_type_at(&self, span: Span) -> Option<&LeekTy> {
        self.expr_types.get(&ExprTypeKey::from_span(span))
    }

    #[must_use]
    pub fn resolve_name(&self, name: &str, from_scope: ScopeId) -> Option<SymbolId> {
        let mut cur = Some(from_scope);
        while let Some(sid) = cur {
            let sc = self.scopes.get(sid.0 as usize)?;
            if let Some(sym) = sc.symbols.get(name) {
                return Some(*sym);
            }
            cur = sc.parent;
        }
        None
    }
}

/// Run scope construction, resolve identifiers, and infer simple expression types.
///
/// `version` controls deprecation rules (e.g. `===` / `!==` warnings apply for LS 4+).
#[must_use]
pub fn run_semantic_analysis(root: &SyntaxNode, version: Version) -> AnalysisResult {
    let a = Analyzer::run_two_phase(root, version);
    AnalysisResult {
        scopes: a.graph.scopes,
        symbols: a.graph.symbols,
        references: a.references,
        expr_types: a.expr_types,
        diagnostics: a.diagnostics,
    }
}

#[cfg(test)]
mod narrowing_smoke {
    use super::run_semantic_analysis;
    use crate::LeekTy;
    use crate::Version;
    use crate::parse_doc;
    use crate::scope::model::ExprTypeKey;
    use crate::syntax::kinds::K;
    use sipha::types::IntoSyntaxKind;

    #[test]
    fn full_analysis_instanceof_narrows_x_in_then() {
        let doc = parse_doc(
            "function f(any x) { if (x instanceof String) { return x; } }",
            Version::V4,
        )
        .unwrap();
        let a = run_semantic_analysis(doc.root(), Version::V4);
        let str_ty = LeekTy::Class("String".to_string());
        let narrowed = a
            .references
            .iter()
            .filter(|r| r.name == "x")
            .any(|r| a.expr_types.get(&ExprTypeKey::from_span(r.span)) == Some(&str_ty));
        assert!(narrowed, "{:?}", a.expr_types);
    }

    #[test]
    fn instanceof_binary_non_trivia_token_sequence() {
        let doc = parse_doc(
            "function f(any x) { if (x instanceof String) { } }",
            Version::V4,
        )
        .unwrap();
        let bins = doc.root().find_all_nodes(K::BinaryExpr.into_syntax_kind());
        let n = bins
            .iter()
            .find(|b| b.text_range().start == 25)
            .expect("binary at offset 25");
        let kinds: Vec<_> = n.non_trivia_tokens().map(|t| t.kind_as::<K>()).collect();
        assert!(
            kinds.iter().any(|k| *k == Some(K::InstanceofKw)),
            "expected InstanceofKw in {:?}",
            kinds
        );
    }
}

#[cfg(test)]
mod analysis_smoke {
    use super::run_semantic_analysis;
    use crate::Version;
    use crate::parse_doc;
    use crate::scope::model::{SemanticCode, SemanticDiagnostic};

    #[test]
    fn undefined_diagnostic_snapshot() {
        let doc = parse_doc("function f() { return z; }", Version::V4).unwrap();
        let a = run_semantic_analysis(doc.root(), Version::V4);
        let d: Vec<&SemanticDiagnostic> = a
            .diagnostics
            .iter()
            .filter(|d| d.code == SemanticCode::UndefinedName)
            .collect();
        assert_eq!(d.len(), 1, "{:?}", a.diagnostics);
        assert!(d[0].message.contains("z"), "message={:?}", d[0].message);
    }

    #[test]
    fn analysis_does_not_panic_on_snippets() {
        for src in [
            "",
            "function f() {}",
            "class C { }",
            ";;;",
            "var x = ",
            "function g() { var a; if (a) { } }",
        ] {
            if let Ok(doc) = parse_doc(src, Version::V4) {
                let _ = run_semantic_analysis(doc.root(), Version::V4);
            }
        }
    }
}
