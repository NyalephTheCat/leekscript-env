//! Deprecation warnings aligned with the reference LeekScript compiler (Java).

use sipha::tree::red::{SyntaxElement, SyntaxNode, SyntaxToken};

use crate::parse::Version;
use crate::scope::model::{SemanticCode, SemanticDiagnostic, SemanticSeverity, SymbolKind};
use crate::syntax::kinds::K;

use super::analyzer::Analyzer;
use super::phase::AnalysisPhase;

impl Analyzer {
    pub(crate) fn on_leave_node_deprecations(&mut self, node: &SyntaxNode) {
        if self.phase != AnalysisPhase::ResolveAndInfer {
            return;
        }
        self.check_strict_equality_deprecated(node);
        self.check_deprecated_call(node);
    }

    fn ls4_or_later(&self) -> bool {
        matches!(self.version, Version::V4)
    }

    /// `===` / `!==` — deprecated for LS 4+ (`LeekExpression.java` in leekscript-java).
    fn check_strict_equality_deprecated(&mut self, node: &SyntaxNode) {
        if !self.ls4_or_later() || node.kind_as::<K>() != Some(K::BinaryExpr) {
            return;
        }
        for t in node.non_trivia_tokens() {
            let k = t.kind_as::<K>();
            if !matches!(k, Some(K::EqEqEq | K::NotEqEq)) {
                continue;
            }
            let op = t.text();
            let alt = if op == "===" { "==" } else { "!=" };
            self.diagnostics.push(SemanticDiagnostic {
                severity: SemanticSeverity::Warning,
                code: SemanticCode::DeprecatedStrictEquality,
                message: format!("`{op}` is deprecated since LeekScript 4; use `{alt}` instead"),
                span: t.text_range(),
                related_span: None,
            });
        }
    }

    fn check_deprecated_call(&mut self, node: &SyntaxNode) {
        if node.kind_as::<K>() != Some(K::CallExpr) {
            return;
        }
        if call_expr_starts_with_eval_kw(node) {
            return;
        }
        let Some(parent) = self.syntax_parent_of(node) else {
            return;
        };
        let Some(tok) = callee_ident_token_for_call(node, &parent) else {
            return;
        };
        let span = tok.text_range();
        let callee_name = tok.text();
        let Some(sym_id) = self
            .references
            .iter()
            .find(|r| r.span == span && r.name == callee_name)
            .and_then(|r| r.resolved)
        else {
            return;
        };
        let sym = &self.graph.symbols[sym_id.0 as usize];
        if !matches!(
            sym.kind,
            SymbolKind::Function | SymbolKind::Global | SymbolKind::Method
        ) {
            return;
        }
        let Some(dep_note) = sym.doc.as_ref().and_then(|d| d.deprecated.as_deref()) else {
            return;
        };
        let hint = {
            let s = dep_note.trim();
            if s.is_empty() {
                String::new()
            } else {
                format!(" ({s})")
            }
        };
        self.diagnostics.push(SemanticDiagnostic {
            severity: SemanticSeverity::Warning,
            code: SemanticCode::DeprecatedCallable,
            message: format!("call to deprecated function `{callee_name}`{hint}"),
            span,
            related_span: Some(sym.name_span),
        });
    }
}

fn call_expr_starts_with_eval_kw(call: &SyntaxNode) -> bool {
    call.non_trivia_tokens()
        .next()
        .and_then(|t| t.kind_as::<K>())
        == Some(K::EvalKw)
}

/// Direct call `name(...)` or parenthesized `(name)(...)` with a single identifier in the callee.
///
/// `parent` must be the immediate CST parent of `call` (see [`Analyzer::syntax_parent_of`]).
fn callee_ident_token_for_call(call: &SyntaxNode, parent: &SyntaxNode) -> Option<SyntaxToken> {
    let children: Vec<SyntaxElement> = parent.children().collect();
    let idx = children.iter().position(|el| match el {
        SyntaxElement::Node(n) => {
            n.offset() == call.offset()
                && n.kind() == call.kind()
                && n.text_len() == call.text_len()
        }
        _ => false,
    })?;
    if idx == 0 {
        return None;
    }
    match &children[idx - 1] {
        SyntaxElement::Token(t) if t.kind_as::<K>() == Some(K::Ident) => Some(t.clone()),
        SyntaxElement::Node(n) => {
            let idents: Vec<SyntaxToken> = n
                .descendant_semantic_tokens()
                .into_iter()
                .filter(|t| t.kind_as::<K>() == Some(K::Ident))
                .collect();
            if idents.len() == 1 {
                Some(idents[0].clone())
            } else {
                None
            }
        }
        _ => None,
    }
}
