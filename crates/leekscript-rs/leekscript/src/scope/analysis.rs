//! Visitor-based scope construction, reference resolution, and light type inference.

use std::collections::{HashMap, HashSet};

use sipha::tree::ast::{AstNode, AstNodeExt};
use sipha::tree::red::{SyntaxNode, SyntaxToken};
use sipha::tree::walk::{Visitor, WalkOptions, WalkResult};

use crate::ast::types::TypeExpr;
use crate::ast::{
    CatchClause, ClassDecl, ForeachStmt, FunctionDecl, GlobalDecl, VarDecl,
};
use crate::syntax::kinds::K;
use crate::Span;

use super::extract::{
    extract_function_params, leek_ty_from_type_expr, try_extract_class_field,
    try_extract_class_method,
};
use super::leek_ty::LeekTy;
use super::model::{
    Reference, Scope, ScopeId, ScopeKind, SemanticDiagnostic, Symbol, SymbolId, SymbolKind,
};

/// Full result of [`run_semantic_analysis`].
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub scopes: Vec<Scope>,
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    pub expr_types: HashMap<(u32, u32), LeekTy>,
    pub diagnostics: Vec<SemanticDiagnostic>,
}

impl AnalysisResult {
    #[must_use]
    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id.0 as usize)
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
#[must_use]
pub fn run_semantic_analysis(root: &SyntaxNode) -> AnalysisResult {
    let mut a = Analyzer::new(1);
    let _ = root.walk(&mut a, &WalkOptions::nodes_only());
    let bindings = std::mem::take(&mut a.binding_spans);
    a.phase = 2;
    a.scope_stack = vec![ScopeId(0)];
    a.scope_replay_idx = 0;
    a.pending_class_body = 0;
    a.skip_leave_block_span = None;
    a.binding_spans = bindings.clone();
    let _ = root.walk(&mut a, &WalkOptions::default());
    AnalysisResult {
        scopes: a.scopes,
        symbols: a.symbols,
        references: a.references,
        expr_types: a.expr_types,
        diagnostics: a.diagnostics,
    }
}

struct Analyzer {
    phase: u8,
    scopes: Vec<Scope>,
    symbols: Vec<Symbol>,
    scope_stack: Vec<ScopeId>,
    next_scope_id: u32,
    next_symbol_id: u32,
    /// `ClassDecl` count waiting for their body `{` block (no extra scope).
    pending_class_body: u32,
    /// Block we entered without pushing (class body).
    skip_leave_block_span: Option<Span>,
    binding_spans: HashSet<(u32, u32)>,
    references: Vec<Reference>,
    expr_types: HashMap<(u32, u32), LeekTy>,
    diagnostics: Vec<SemanticDiagnostic>,
    /// For `constructor` / method heads: innermost class name on the stack.
    class_name_stack: Vec<String>,
    /// Phase 1 records every child scope push; phase 2 replays the same [`ScopeId`] sequence.
    scope_push_order: Vec<ScopeId>,
    scope_replay_idx: usize,
    /// Walk stack (CST node kinds) for context-sensitive identifier handling (e.g. skip `.field` names).
    node_stack: Vec<Option<K>>,
    /// Nesting depth inside `BinaryExpr` nodes whose **direct** children include `instanceof` (RHS is a type).
    instanceof_type_ctx_depth: u32,
}

impl Analyzer {
    fn new(phase: u8) -> Self {
        let mut s = Self {
            phase,
            scopes: Vec::new(),
            symbols: Vec::new(),
            scope_stack: Vec::new(),
            next_scope_id: 0,
            next_symbol_id: 0,
            pending_class_body: 0,
            skip_leave_block_span: None,
            binding_spans: HashSet::new(),
            references: Vec::new(),
            expr_types: HashMap::new(),
            diagnostics: Vec::new(),
            class_name_stack: Vec::new(),
            scope_push_order: Vec::new(),
            scope_replay_idx: 0,
            node_stack: Vec::new(),
            instanceof_type_ctx_depth: 0,
        };
        let root = s.alloc_scope(None, ScopeKind::Module);
        s.scope_stack.push(root);
        s
    }

    fn push_child_scope(&mut self, parent: Option<ScopeId>, kind: ScopeKind) -> ScopeId {
        if self.phase == 1 {
            let id = self.alloc_scope(parent, kind);
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

    fn alloc_scope(&mut self, parent: Option<ScopeId>, kind: ScopeKind) -> ScopeId {
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

    fn current_scope(&self) -> ScopeId {
        *self.scope_stack.last().expect("scope stack")
    }

    fn declare(
        &mut self,
        scope_id: ScopeId,
        name: String,
        name_span: Span,
        kind: SymbolKind,
        declared_ty: Option<LeekTy>,
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
                },
            );
        }
        self.symbols[sid] = sym;
        if let Some(sc) = self.scopes.get_mut(scope_id.0 as usize) {
            sc.symbols.insert(name, id);
        }
        self.binding_spans.insert((name_span.start, name_span.end));
        id
    }

    fn sync_enter(&mut self, node: &SyntaxNode) {
        match node.kind_as::<K>() {
            Some(K::FunctionDecl) => {
                let outer = self.current_scope();
                let fd = FunctionDecl::cast(node.clone()).expect("fd");
                let name = fd.name().unwrap_or_default();
                let name_span = function_name_span(&fd).unwrap_or_else(|| node.text_range());
                if self.phase == 1 {
                    self.declare(
                        outer,
                        name,
                        name_span,
                        SymbolKind::Function,
                        fd.return_type().map(|t| leek_ty_from_type_expr(&t)),
                    );
                }
                let fn_sc = self.push_child_scope(Some(outer), ScopeKind::Function);
                if self.phase == 1 {
                    for (ty, pname, pspan) in extract_function_params(&fd) {
                        let dt = ty.as_ref().map(leek_ty_from_type_expr);
                        self.declare(fn_sc, pname, pspan, SymbolKind::Parameter, dt);
                    }
                }
            }
            Some(K::Block) => {
                if self.pending_class_body > 0 {
                    self.pending_class_body -= 1;
                    self.skip_leave_block_span = Some(node.text_range());
                    return;
                }
                let p = self.current_scope();
                self.push_child_scope(Some(p), ScopeKind::Block);
            }
            Some(K::ClassDecl) => {
                let cd = ClassDecl::cast(node.clone()).expect("cd");
                let outer = self.current_scope();
                let cname = cd.name().unwrap_or_default();
                let cspan = class_name_span(&cd).unwrap_or_else(|| node.text_range());
                if self.phase == 1 {
                    self.declare(
                        outer,
                        cname.clone(),
                        cspan,
                        SymbolKind::Class,
                        Some(LeekTy::Class(cname.clone())),
                    );
                }
                self.class_name_stack.push(cname);
                self.push_child_scope(Some(outer), ScopeKind::Class);
                self.pending_class_body += 1;
            }
            Some(K::ClassMember) => {
                let cn = self.class_name_stack.last().cloned().unwrap_or_default();
                if let Some(m) = try_extract_class_method(node, &cn) {
                    let class_sc = self.current_scope();
                    if self.phase == 1 {
                        let sk = if m.is_constructor {
                            SymbolKind::Constructor
                        } else {
                            SymbolKind::Method
                        };
                        self.declare(class_sc, m.name, m.name_span, sk, None);
                    }
                    let msc = self.push_child_scope(Some(class_sc), ScopeKind::Method);
                    if self.phase == 1 {
                        for (ty, pname, pspan) in m.params {
                            let dt = ty.as_ref().map(leek_ty_from_type_expr);
                            self.declare(msc, pname, pspan, SymbolKind::Parameter, dt);
                        }
                    }
                } else if self.phase == 1 {
                    if let Some((fname, fspan, fty)) = try_extract_class_field(node) {
                        let class_sc = self.current_scope();
                        self.declare(class_sc, fname, fspan, SymbolKind::Field, Some(fty));
                    }
                }
            }
            Some(K::VarDecl) => {
                if self.phase != 1 {
                    return;
                }
                let vd = VarDecl::cast(node.clone()).expect("vd");
                let sc = self.current_scope();
                if let (Some(n), Some(sp)) = (vd.first_name(), var_decl_name_span(&vd)) {
                    let dt = vd.syntax().child::<TypeExpr>().map(|t| leek_ty_from_type_expr(&t));
                    self.declare(sc, n, sp, SymbolKind::Variable, dt);
                }
            }
            Some(K::GlobalDecl) => {
                if self.phase != 1 {
                    return;
                }
                let g = GlobalDecl::cast(node.clone()).expect("g");
                let module = ScopeId(0);
                if let (Some(n), Some(sp)) = (g.first_name(), global_name_span(&g)) {
                    let dt = g.type_expr().map(|t| leek_ty_from_type_expr(&t));
                    self.declare(module, n, sp, SymbolKind::Global, dt);
                }
            }
            Some(K::ForeachStmt) => {
                if self.phase != 1 {
                    return;
                }
                let fe = ForeachStmt::cast(node.clone()).expect("fe");
                let sc = self.current_scope();
                for (n, sp) in foreach_bind_spans(&fe) {
                    self.declare(sc, n, sp, SymbolKind::Variable, None);
                }
            }
            Some(K::CatchClause) => {
                if self.phase != 1 {
                    return;
                }
                let cc = CatchClause::cast(node.clone()).expect("cc");
                let sc = self.current_scope();
                if let (Some(n), Some(sp)) = (cc.param_name(), catch_param_span(&cc)) {
                    let dt = cc.param_type().map(|t| leek_ty_from_type_expr(&t));
                    self.declare(sc, n, sp, SymbolKind::Variable, dt);
                }
            }
            _ => {}
        }
    }

    fn sync_leave(&mut self, node: &SyntaxNode) {
        match node.kind_as::<K>() {
            Some(K::Block) => {
                if self.skip_leave_block_span == Some(node.text_range()) {
                    self.skip_leave_block_span = None;
                    return;
                }
                self.scope_stack.pop();
            }
            Some(K::FunctionDecl) => {
                self.scope_stack.pop();
            }
            Some(K::ClassMember) => {
                let cn = self.class_name_stack.last().cloned().unwrap_or_default();
                if try_extract_class_method(node, &cn).is_some() {
                    self.scope_stack.pop();
                }
            }
            Some(K::ClassDecl) => {
                self.class_name_stack.pop();
                self.scope_stack.pop();
            }
            _ => {}
        }
    }

    fn resolve_ident(&mut self, token: &SyntaxToken) {
        if token.kind_as::<K>() != Some(K::Ident) {
            return;
        }
        let span = token.text_range();
        let key = (span.start, span.end);
        if self.binding_spans.contains(&key) {
            return;
        }
        // Property name in `base.field` — not a lexical reference to a top-level/local symbol.
        if self.phase == 2 && matches!(self.node_stack.last(), Some(Some(K::MemberExpr))) {
            return;
        }
        let name = token.text().to_string();
        let mut cur = Some(self.current_scope());
        let mut resolved = None;
        while let Some(sid) = cur {
            if let Some(sc) = self.scopes.get(sid.0 as usize) {
                if let Some(sym) = sc.symbols.get(&name) {
                    resolved = Some(*sym);
                    break;
                }
                cur = sc.parent;
            } else {
                break;
            }
        }
        // RHS of `instanceof`: uppercase names are class types — register at module scope if new.
        // Lowercase segments (e.g. `pkg` in `pkg.Type`) stay out of the value namespace.
        if self.phase == 2 && self.instanceof_type_ctx_depth > 0 && resolved.is_none() {
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
        if self.phase == 2 && resolved.is_none() {
            self.diagnostics.push(SemanticDiagnostic {
                message: format!("undefined name `{name}`"),
                span,
            });
        }
    }

    /// `instanceof` RHS: ensure `Name` is a [`SymbolKind::Class`] at module scope (Java-style type).
    /// Returns `None` for lowercase identifiers (package / value segments we do not model).
    fn ensure_instanceof_class_type(&mut self, name: &str, name_span: Span) -> Option<SymbolId> {
        if !name.chars().next().is_some_and(|c| c.is_uppercase()) {
            return None;
        }
        let module = ScopeId(0);
        if let Some(sc) = self.scopes.get(module.0 as usize) {
            if let Some(&id) = sc.symbols.get(name) {
                return Some(id);
            }
        }
        Some(self.declare(
            module,
            name.to_string(),
            name_span,
            SymbolKind::Class,
            Some(LeekTy::Class(name.to_string())),
        ))
    }

    fn infer_expr_node(&mut self, node: &SyntaxNode) {
        let span = node.text_range();
        let key = (span.start, span.end);
        let ty = match node.kind_as::<K>() {
            Some(K::BinaryExpr) => infer_binary(self, node),
            Some(K::IntervalExpr) => infer_interval_ty(self, node),
            Some(K::Expr | K::ParenExpr | K::UnaryExpr) => expr_span_ty(self, node),
            _ => return,
        };
        self.expr_types.insert(key, ty);
    }

    fn apply_var_inits(&mut self, node: &SyntaxNode) {
        if self.phase != 2 || node.kind_as::<K>() != Some(K::VarDecl) {
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
        let sym = &mut self.symbols[sym_id.0 as usize];
        if sym.declared_ty.is_none() {
            sym.inferred_ty = Some(rhs_ty);
        } else if let Some(dt) = &sym.declared_ty {
            if !LeekTy::is_assignable_to(&rhs_ty, dt) {
                self.diagnostics.push(SemanticDiagnostic {
                    message: format!("initializer type incompatible with `{name}` annotation"),
                    span: vd.syntax().text_range(),
                });
            }
        }
    }

    fn resolve_here(&self, name: &str) -> Option<SymbolId> {
        let mut cur = Some(self.current_scope());
        while let Some(sid) = cur {
            let sc = self.scopes.get(sid.0 as usize)?;
            if let Some(s) = sc.symbols.get(name) {
                return Some(*s);
            }
            cur = sc.parent;
        }
        None
    }
}

fn infer_interval_ty(a: &Analyzer, node: &SyntaxNode) -> LeekTy {
    let mut acc = None::<LeekTy>;
    for tok in node.descendant_semantic_tokens() {
        if tok.kind_as::<K>() == Some(K::Number) {
            let nt = LeekTy::from_number_literal_text(tok.text());
            acc = Some(match acc {
                None => nt,
                Some(prev) => LeekTy::unify_binary_numeric(&prev, &nt),
            });
        }
    }
    if let Some(t) = acc {
        return LeekTy::Interval(Box::new(LeekTy::interval_inner(t)));
    }
    for n in node.descendant_nodes() {
        if n.kind_as::<K>() != Some(K::Expr) {
            continue;
        }
        let et = expr_span_ty(a, &n);
        if !LeekTy::is_interval_element(&et) {
            continue;
        }
        acc = Some(match acc {
            None => et,
            Some(prev) => LeekTy::unify_binary_numeric(&prev, &et),
        });
    }
    let inner = acc.map(LeekTy::interval_inner).unwrap_or(LeekTy::Unknown);
    LeekTy::Interval(Box::new(inner))
}

fn infer_binary(a: &Analyzer, node: &SyntaxNode) -> LeekTy {
    let kids: Vec<_> = node.child_nodes().collect();
    if kids.len() < 2 {
        return LeekTy::Unknown;
    }
    let l = &kids[0];
    let r = &kids[kids.len() - 1];
    let lk = expr_span_ty(a, l);
    let rk = expr_span_ty(a, r);
    LeekTy::coerce_binary_op(&lk, &rk)
}

fn ty_from_semantic_tokens(a: &Analyzer, node: &SyntaxNode) -> LeekTy {
    if node
        .descendant_nodes()
        .any(|n| n.kind_as::<K>() == Some(K::BinaryExpr))
    {
        return LeekTy::Unknown;
    }
    for tok in node.descendant_semantic_tokens() {
        let k = (tok.text_range().start, tok.text_range().end);
        if let Some(t) = a.expr_types.get(&k) {
            if *t != LeekTy::Unknown {
                return t.clone();
            }
        }
    }
    LeekTy::Unknown
}

fn expr_span_ty(a: &Analyzer, node: &SyntaxNode) -> LeekTy {
    let r = node.text_range();
    let key = (r.start, r.end);
    if let Some(t) = a.expr_types.get(&key) {
        if *t != LeekTy::Unknown {
            return t.clone();
        }
    }

    match node.kind_as::<K>() {
        Some(K::BinaryExpr) => infer_binary(a, node),
        Some(K::IntervalExpr) => infer_interval_ty(a, node),
        Some(K::Expr) => {
            if let Some(ch) = node.child_nodes().next() {
                let t = expr_span_ty(a, &ch);
                if t != LeekTy::Unknown {
                    return t;
                }
            }
            ty_from_semantic_tokens(a, node)
        }
        Some(K::UnaryExpr) => node
            .child_nodes()
            .last()
            .map(|c| expr_span_ty(a, &c))
            .unwrap_or(LeekTy::Unknown),
        Some(K::ParenExpr) => node
            .child_nodes()
            .find(|c| c.kind_as::<K>() == Some(K::Expr))
            .map(|c| expr_span_ty(a, &c))
            .unwrap_or(LeekTy::Unknown),
        _ => {
            if let Some(ch) = node.child_nodes().next() {
                let t = expr_span_ty(a, &ch);
                if t != LeekTy::Unknown {
                    return t;
                }
            }
            for ch in node.child_nodes().skip(1) {
                let t = expr_span_ty(a, &ch);
                if t != LeekTy::Unknown {
                    return t;
                }
            }
            ty_from_semantic_tokens(a, node)
        }
    }
}

fn binary_expr_is_instanceof(node: &SyntaxNode) -> bool {
    node.kind_as::<K>() == Some(K::BinaryExpr)
        && node
            .child_tokens()
            .any(|t| t.kind_as::<K>() == Some(K::InstanceofKw))
}

impl Visitor for Analyzer {
    fn enter_node(&mut self, node: &SyntaxNode) -> WalkResult {
        self.node_stack.push(node.kind_as::<K>());
        if self.phase == 2 && binary_expr_is_instanceof(node) {
            self.instanceof_type_ctx_depth += 1;
        }
        self.sync_enter(node);
        WalkResult::Continue(())
    }

    fn leave_node(&mut self, node: &SyntaxNode) -> WalkResult {
        if self.phase == 2 {
            self.infer_expr_node(node);
            self.apply_var_inits(node);
        }
        self.sync_leave(node);
        if self.phase == 2 && binary_expr_is_instanceof(node) {
            self.instanceof_type_ctx_depth = self.instanceof_type_ctx_depth.saturating_sub(1);
        }
        let _ = self.node_stack.pop();
        WalkResult::Continue(())
    }

    fn visit_token(&mut self, token: &SyntaxToken) -> WalkResult {
        if self.phase == 2 {
            let span = token.text_range();
            let key = (span.start, span.end);
            let in_member_expr = matches!(self.node_stack.last(), Some(Some(K::MemberExpr)));
            match token.kind_as::<K>() {
                Some(K::Number) => {
                    let t = LeekTy::from_number_literal_text(token.text());
                    self.expr_types.insert(key, t);
                }
                Some(K::String) => {
                    self.expr_types.insert(key, LeekTy::String);
                }
                Some(K::TrueKw | K::FalseKw) => {
                    self.expr_types.insert(key, LeekTy::Boolean);
                }
                Some(K::NullKw) => {
                    self.expr_types.insert(key, LeekTy::Null);
                }
                Some(K::Pi) => {
                    self.expr_types.insert(key, LeekTy::Real);
                }
                Some(K::Infinity) => {
                    self.expr_types.insert(key, LeekTy::Real);
                }
                _ => {}
            }
            self.resolve_ident(token);
            if token.kind_as::<K>() == Some(K::Ident) && !in_member_expr {
                if let Some(sid) = self.resolve_here(token.text()) {
                    let sym = &self.symbols[sid.0 as usize];
                    // Skip declaration binding sites for vars/functions (types come from RHS / elsewhere).
                    // Keep `Class` (including `instanceof String` synthetic classes): the name is the type.
                    let binding_skip_expr_ty = self.binding_spans.contains(&key)
                        && sym.kind != SymbolKind::Class;
                    if !binding_skip_expr_ty {
                        let t = sym
                            .declared_ty
                            .clone()
                            .or_else(|| sym.inferred_ty.clone())
                            .unwrap_or(LeekTy::Unknown);
                        self.expr_types.insert(key, t);
                    }
                }
            }
        }
        WalkResult::Continue(())
    }
}

fn function_name_span(fd: &FunctionDecl) -> Option<Span> {
    fd.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<K>() == Some(K::Ident))
        .map(|t| t.text_range())
}

fn class_name_span(cd: &ClassDecl) -> Option<Span> {
    cd.syntax()
        .child_tokens()
        .take_while(|t| t.kind_as::<K>() != Some(K::ExtendsKw))
        .find(|t| t.kind_as::<K>() == Some(K::Ident))
        .map(|t| t.text_range())
}

fn var_decl_name_span(vd: &VarDecl) -> Option<Span> {
    vd.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<K>() == Some(K::Ident))
        .map(|t| t.text_range())
}

fn global_name_span(g: &GlobalDecl) -> Option<Span> {
    g.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<K>() == Some(K::Ident))
        .map(|t| t.text_range())
}

fn catch_param_span(cc: &CatchClause) -> Option<Span> {
    cc.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<K>() == Some(K::Ident))
        .map(|t| t.text_range())
}

fn foreach_bind_spans(fe: &ForeachStmt) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    let mut after_for = false;
    for t in fe.syntax().child_tokens() {
        match t.kind_as::<K>() {
            Some(K::ForKw) => after_for = true,
            Some(K::InKw) => break,
            Some(K::Ident) if after_for => out.push((t.text().to_string(), t.text_range())),
            _ => {}
        }
    }
    out
}
