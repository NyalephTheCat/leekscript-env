use std::collections::HashMap;

use sipha::tree::ast::{AstNode, AstNodeExt};
use sipha::tree::red::{SyntaxNode, SyntaxToken};
use sipha::tree::walk::WalkOptions;

use crate::Span;
use crate::ast::types::TypeExpr;
use crate::ast::{ForeachStmt, VarDecl};
use crate::parse::Version;
use crate::scope::leek_ty::LeekTy;
use crate::scope::model::{
    ExprTypeKey, Reference, ScopeId, ScopeKind, SemanticCode, SemanticDiagnostic, SemanticSeverity,
    SymbolId, SymbolKind,
};
use crate::syntax::kinds::K;

use super::enter_leave::{sync_enter, sync_leave};
use super::graph::ScopeGraph;
use super::infer::{
    expr_span_ty, infer_binary, infer_interval_ty, set_var_inferred_if_unannotated,
};
use super::narrowing::{accumulated_narrowing_maps, should_track_narrowing};
use super::narrowing_env::NarrowingEnv;
use super::phase::AnalysisPhase;
use super::spans::foreach_bind_spans;

pub(crate) struct Analyzer {
    pub phase: AnalysisPhase,
    pub graph: ScopeGraph,
    pub scope_stack: Vec<ScopeId>,
    scope_push_order: Vec<ScopeId>,
    scope_replay_idx: usize,
    /// `ClassDecl` count waiting for their body `{` block (no extra scope).
    pub(crate) pending_class_body: u32,
    pub skip_leave_block_span: Option<Span>,
    pub references: Vec<Reference>,
    pub expr_types: HashMap<ExprTypeKey, LeekTy>,
    pub diagnostics: Vec<SemanticDiagnostic>,
    pub class_name_stack: Vec<String>,
    pub node_stack: Vec<Option<K>>,
    pub instanceof_type_ctx_depth: u32,
    pub narrowing: NarrowingEnv,
    pub syntax_node_stack: Vec<SyntaxNode>,
    pub(crate) version: Version,
}

impl Analyzer {
    pub(crate) fn new(phase: AnalysisPhase, version: Version) -> Self {
        let mut graph = ScopeGraph::new();
        let root = graph.alloc_scope(None, ScopeKind::Module);
        let mut s = Self {
            phase,
            graph,
            scope_stack: Vec::new(),
            scope_push_order: Vec::new(),
            scope_replay_idx: 0,
            pending_class_body: 0,
            skip_leave_block_span: None,
            references: Vec::new(),
            expr_types: HashMap::new(),
            diagnostics: Vec::new(),
            class_name_stack: Vec::new(),
            node_stack: Vec::new(),
            instanceof_type_ctx_depth: 0,
            narrowing: NarrowingEnv::default(),
            syntax_node_stack: Vec::new(),
            version,
        };
        s.scope_stack.push(root);
        s
    }

    pub(crate) fn run_two_phase(root: &SyntaxNode, version: Version) -> Self {
        let mut a = Analyzer::new(AnalysisPhase::BuildScopes, version);
        let _ = root.walk(&mut a, &WalkOptions::nodes_only());
        let bindings = std::mem::take(&mut a.graph.binding_spans);
        a.phase = AnalysisPhase::ResolveAndInfer;
        a.scope_stack = vec![ScopeId(0)];
        a.scope_replay_idx = 0;
        a.pending_class_body = 0;
        a.skip_leave_block_span = None;
        a.graph.binding_spans = bindings;
        let _ = root.walk(&mut a, &WalkOptions::default());
        a
    }

    pub(crate) fn push_child_scope(&mut self, parent: Option<ScopeId>, kind: ScopeKind) -> ScopeId {
        if self.phase.is_build_scopes() {
            let id = self.graph.alloc_scope(parent, kind);
            self.scope_push_order.push(id);
            self.scope_stack.push(id);
            id
        } else {
            let id = self.scope_push_order[self.scope_replay_idx];
            self.scope_replay_idx += 1;
            self.scope_stack.push(id);
            id
        }
    }

    pub(crate) fn current_scope(&self) -> ScopeId {
        *self.scope_stack.last().expect("scope stack")
    }

    pub(crate) fn sync_enter(&mut self, node: &SyntaxNode) {
        sync_enter(self, node);
    }

    pub(crate) fn sync_leave(&mut self, node: &SyntaxNode) {
        sync_leave(self, node);
    }

    pub(crate) fn resolve_ident(&mut self, token: &SyntaxToken) {
        if token.kind_as::<K>() != Some(K::Ident) {
            return;
        }
        let span = token.text_range();
        let key = ExprTypeKey::from_span(span);
        if self.graph.binding_spans.contains(&key) {
            return;
        }
        if self.phase == AnalysisPhase::ResolveAndInfer
            && matches!(self.node_stack.last(), Some(Some(K::MemberExpr)))
        {
            return;
        }
        let name = token.text().to_string();
        let mut cur = Some(self.current_scope());
        let mut resolved = None;
        while let Some(sid) = cur {
            if let Some(sc) = self.graph.scopes.get(sid.0 as usize) {
                if let Some(sym) = sc.symbols.get(&name) {
                    resolved = Some(*sym);
                    break;
                }
                cur = sc.parent;
            } else {
                break;
            }
        }
        if self.phase == AnalysisPhase::ResolveAndInfer
            && self.instanceof_type_ctx_depth > 0
            && resolved.is_none()
        {
            if let Some(id) = self.ensure_instanceof_class_type(&name, span) {
                resolved = Some(id);
            } else {
                return;
            }
        }
        self.references.push(Reference {
            name: name.clone(),
            span,
            resolved,
        });
        if self.phase == AnalysisPhase::ResolveAndInfer && resolved.is_none() {
            self.diagnostics.push(SemanticDiagnostic {
                severity: SemanticSeverity::Error,
                code: SemanticCode::UndefinedName,
                message: format!("undefined name `{name}`"),
                span,
                related_span: None,
            });
        }
    }

    fn ensure_instanceof_class_type(&mut self, name: &str, name_span: Span) -> Option<SymbolId> {
        if !name.chars().next().is_some_and(|c| c.is_uppercase()) {
            return None;
        }
        let module = ScopeId(0);
        if let Some(sc) = self.graph.scopes.get(module.0 as usize) {
            if let Some(&id) = sc.symbols.get(name) {
                return Some(id);
            }
        }
        Some(self.graph.declare(
            self.phase,
            module,
            name.to_string(),
            name_span,
            SymbolKind::Class,
            Some(LeekTy::Class(name.to_string())),
            None,
        ))
    }

    pub(crate) fn infer_expr_node(&mut self, node: &SyntaxNode) {
        let span = node.text_range();
        let key = ExprTypeKey::from_span(span);
        let ty = match node.kind_as::<K>() {
            Some(K::BinaryExpr) => infer_binary(self, node),
            Some(K::IntervalExpr) => infer_interval_ty(self, node),
            Some(K::Expr | K::ParenExpr | K::UnaryExpr) => expr_span_ty(self, node),
            _ => return,
        };
        self.expr_types.insert(key, ty);
    }

    pub(crate) fn apply_var_inits(&mut self, node: &SyntaxNode) {
        if self.phase != AnalysisPhase::ResolveAndInfer || node.kind_as::<K>() != Some(K::VarDecl) {
            return;
        }
        let vd = VarDecl::cast(node.clone()).expect("vd");
        let Some(name) = vd.first_name() else {
            return;
        };
        let Some(sym_id) = self.resolve_here(&name) else {
            return;
        };
        let rhs_ty = vd
            .syntax()
            .descendant_nodes()
            .filter(|n| n.kind_as::<K>() == Some(K::IntervalExpr))
            .map(|n| expr_span_ty(self, &n))
            .find(|t| *t != LeekTy::Unknown)
            .or_else(|| {
                vd.syntax()
                    .descendant_nodes()
                    .filter(|n| n.kind_as::<K>() == Some(K::Expr))
                    .map(|e| expr_span_ty(self, &e))
                    .filter(|t| *t != LeekTy::Unknown)
                    .last()
            });
        let Some(rhs_ty) = rhs_ty else {
            return;
        };
        let sym = &mut self.graph.symbols[sym_id.0 as usize];
        let type_annotation_span = vd
            .syntax()
            .child::<TypeExpr>()
            .map(|t| t.syntax().text_range());
        if sym.declared_ty.is_none() {
            sym.inferred_ty = Some(rhs_ty);
        } else if let Some(dt) = &sym.declared_ty {
            if !LeekTy::is_assignable_to(&rhs_ty, dt) {
                self.diagnostics.push(SemanticDiagnostic {
                    severity: SemanticSeverity::Error,
                    code: SemanticCode::IncompatibleInitializer,
                    message: format!("initializer type incompatible with `{name}` annotation"),
                    span: vd.syntax().text_range(),
                    related_span: type_annotation_span,
                });
            }
        }
    }

    pub(crate) fn resolve_here(&self, name: &str) -> Option<SymbolId> {
        let mut cur = Some(self.current_scope());
        while let Some(sid) = cur {
            let sc = self.graph.scopes.get(sid.0 as usize)?;
            if let Some(s) = sc.symbols.get(name) {
                return Some(*s);
            }
            cur = sc.parent;
        }
        None
    }

    #[must_use]
    pub(crate) fn symbol_effective_ty(&self, sid: SymbolId) -> LeekTy {
        self.graph.symbols[sid.0 as usize].effective_ty()
    }

    pub(crate) fn push_narrowing_frame_if_needed(&mut self, node: &SyntaxNode) {
        if !should_track_narrowing(self, node) {
            return;
        }
        let map = accumulated_narrowing_maps(self, node);
        self.narrowing.push_frame(map);
    }

    pub(crate) fn syntax_parent_of(&self, node: &SyntaxNode) -> Option<SyntaxNode> {
        let (i, _) = self
            .syntax_node_stack
            .iter()
            .enumerate()
            .rev()
            .find(|(_, n)| n.offset() == node.offset() && n.kind() == node.kind())?;
        i.checked_sub(1)
            .and_then(|j| self.syntax_node_stack.get(j).cloned())
    }

    pub(crate) fn pop_narrowing_frame_if_needed(&mut self, node: &SyntaxNode) {
        if !should_track_narrowing(self, node) {
            return;
        }
        self.narrowing.pop_frame();
    }

    pub(crate) fn apply_foreach_var_inference(&mut self, node: &SyntaxNode) {
        if node.kind_as::<K>() != Some(K::ForeachStmt) {
            return;
        }
        let fe = ForeachStmt::cast(node.clone()).expect("foreach");
        let Some(iter_e) = fe.iterable() else {
            return;
        };
        let iter_ty = expr_span_ty(self, iter_e.syntax());
        let binds = foreach_bind_spans(&fe);
        match (&binds[..], &iter_ty) {
            ([(n, _)], LeekTy::Array(elem)) => {
                if let Some(sid) = self.resolve_here(n) {
                    set_var_inferred_if_unannotated(self, sid, (**elem).clone());
                }
            }
            ([(k, _), (v, _)], LeekTy::Map(ek, ev)) => {
                if let Some(sid) = self.resolve_here(k) {
                    set_var_inferred_if_unannotated(self, sid, (**ek).clone());
                }
                if let Some(sid) = self.resolve_here(v) {
                    set_var_inferred_if_unannotated(self, sid, (**ev).clone());
                }
            }
            _ => {}
        }
    }
}
