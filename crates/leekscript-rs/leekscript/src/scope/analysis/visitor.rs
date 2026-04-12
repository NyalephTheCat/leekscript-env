use sipha::tree::red::{SyntaxNode, SyntaxToken};
use sipha::tree::walk::{Visitor, WalkResult};

use crate::scope::leek_ty::LeekTy;
use crate::scope::model::{ExprTypeKey, SymbolKind};
use crate::syntax::kinds::{Lex, Node};

use super::analyzer::Analyzer;
use super::infer::binary_expr_is_instanceof;
use super::phase::AnalysisPhase;

impl Visitor for Analyzer {
    fn enter_node(&mut self, node: &SyntaxNode) -> WalkResult {
        if self.phase == AnalysisPhase::ResolveAndInfer {
            self.syntax_node_stack.push(node.clone());
        }
        self.node_stack.push(node.kind_as::<Node>());
        if self.phase == AnalysisPhase::ResolveAndInfer && binary_expr_is_instanceof(node) {
            self.instanceof_type_ctx_depth += 1;
        }
        self.sync_enter(node);
        if self.phase == AnalysisPhase::ResolveAndInfer {
            self.push_narrowing_frame_if_needed(node);
            self.push_short_circuit_or_rhs_narrowing_if_needed(node);
        }
        WalkResult::Continue(())
    }

    fn leave_node(&mut self, node: &SyntaxNode) -> WalkResult {
        if self.phase == AnalysisPhase::ResolveAndInfer {
            self.check_bare_return_semicolon(node);
            self.infer_expr_node(node);
            self.apply_var_inits(node);
            self.apply_foreach_var_inference(node);
            self.on_leave_node_deprecations(node);
            self.pop_short_circuit_or_rhs_narrowing_if_needed(node);
            self.pop_narrowing_frame_if_needed(node);
        }
        self.sync_leave(node);
        if self.phase == AnalysisPhase::ResolveAndInfer && binary_expr_is_instanceof(node) {
            self.instanceof_type_ctx_depth = self.instanceof_type_ctx_depth.saturating_sub(1);
        }
        let _ = self.node_stack.pop();
        if self.phase == AnalysisPhase::ResolveAndInfer {
            let _ = self.syntax_node_stack.pop();
        }
        WalkResult::Continue(())
    }

    fn visit_token(&mut self, token: &SyntaxToken) -> WalkResult {
        if self.phase == AnalysisPhase::ResolveAndInfer {
            let span = token.text_range();
            let key = ExprTypeKey::from_span(span);
            let in_member_expr = matches!(self.node_stack.last(), Some(Some(Node::MemberExpr)));
            match token.kind_as::<Lex>() {
                Some(Lex::Number) => {
                    let t = LeekTy::from_number_literal_text(token.text());
                    self.expr_types.insert(key, t);
                }
                Some(Lex::String) => {
                    self.expr_types.insert(key, LeekTy::String);
                }
                Some(Lex::TrueKw | Lex::FalseKw) => {
                    self.expr_types.insert(key, LeekTy::Boolean);
                }
                Some(Lex::NullKw) => {
                    self.expr_types.insert(key, LeekTy::Null);
                }
                Some(Lex::Pi) => {
                    self.expr_types.insert(key, LeekTy::Real);
                }
                Some(Lex::Infinity) => {
                    self.expr_types.insert(key, LeekTy::Real);
                }
                Some(Lex::ThisKw) => {
                    if let Some(ty) = self.implicit_this_ty() {
                        self.expr_types.insert(key, ty);
                    }
                }
                _ => {}
            }
            self.resolve_ident(token);
            if token.kind_as::<Lex>() == Some(Lex::Ident) && !in_member_expr {
                if let Some(sid) = self.resolve_here(token.text()) {
                    let sym = &self.graph.symbols[sid.0 as usize];
                    let binding_skip_expr_ty =
                        self.graph.binding_spans.contains(&key) && sym.kind != SymbolKind::Class;
                    if !binding_skip_expr_ty {
                        let base = sym.effective_ty();
                        let t = self.narrowing.with_narrowing(sid, base);
                        self.expr_types.insert(key, t);
                    }
                }
            }
        }
        WalkResult::Continue(())
    }
}
