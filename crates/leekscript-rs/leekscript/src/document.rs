//! Mutable parse result: syntax root plus source buffer kept in sync for pipelines
//! (desugar, format, reparse).
//!
//! [`crate::parse::parse_doc`] returns a sipha [`ParsedDoc`] whose root cannot be replaced.
//! Use [`LeekDoc`] when you need to run [`LeekDoc::apply_transform`], [`LeekDoc::replace_span`],
//! round-trip through emit + parse, or otherwise replace the tree while keeping [`LeekDoc::source`]
//! aligned.

use crate::ast::Root;
use crate::parse::{ParseError, Version, parse_doc, parse_doc_with_recovery};
use sipha::diagnostics::error::SemanticDiagnostic;
use sipha::diagnostics::line_index::LineIndex;
use sipha::diagnostics::parsed_doc::ParsedDoc;
use sipha::tree::ast::AstNode;
use sipha::tree::red::SyntaxNode;
use sipha::types::{Pos, Span};

#[cfg(feature = "emit")]
use sipha::tree::emit::{EmitOptions, syntax_root_to_string};

#[cfg(feature = "walk")]
use sipha::tree::walk::{Visitor, WalkOptions, WalkResult};

#[cfg(feature = "partial-reparse")]
use crate::syntax::kinds::K;
#[cfg(feature = "partial-reparse")]
use sipha::parse::engine::Engine;
#[cfg(feature = "partial-reparse")]
use sipha::parse::incremental::TextEdit;
#[cfg(feature = "partial-reparse")]
use sipha::parse::partial_reparse::{PartialReparseConfig, reparse_partial_or_fallback};
#[cfg(feature = "partial-reparse")]
use sipha::types::SyntaxKind;

/// Kinds that can appear as a direct child of [`K::Root`] from the `stmt` rule — boundaries for
/// [`reparse_partial_or_fallback`](sipha::parse::partial_reparse::reparse_partial_or_fallback).
#[cfg(feature = "partial-reparse")]
const STMT_BOUNDARY_KINDS: &[SyntaxKind] = &[
    K::IncludeStmt as SyntaxKind,
    K::ReturnStmt as SyntaxKind,
    K::BreakStmt as SyntaxKind,
    K::ContinueStmt as SyntaxKind,
    K::GlobalDecl as SyntaxKind,
    K::ElseStmt as SyntaxKind,
    K::SwitchStmt as SyntaxKind,
    K::VarDecl as SyntaxKind,
    K::FunctionDecl as SyntaxKind,
    K::ClassDecl as SyntaxKind,
    K::IfStmt as SyntaxKind,
    K::ForStmt as SyntaxKind,
    K::ForeachStmt as SyntaxKind,
    K::DoWhileStmt as SyntaxKind,
    K::TryStmt as SyntaxKind,
    K::ThrowStmt as SyntaxKind,
    K::ImportStmt as SyntaxKind,
    K::ExportStmt as SyntaxKind,
    K::GotoStmt as SyntaxKind,
    K::PackageStmt as SyntaxKind,
    K::ConstDecl as SyntaxKind,
    K::WhileStmt as SyntaxKind,
    K::MatchStmt as SyntaxKind,
    K::Stmt as SyntaxKind,
    K::EmptyStmt as SyntaxKind,
    K::ErrorStmt as SyntaxKind,
];

/// Failure when splicing source or re-parsing after an edit.
#[derive(Debug)]
pub enum DocEditError {
    /// Span not inside the current buffer or `start > end`.
    InvalidSpan,
    /// Resulting buffer is not valid UTF-8 (unlikely if the original source and replacement are UTF-8).
    InvalidUtf8,
    /// Full-document re-parse failed after the replacement.
    Reparse(ParseError),
}

/// Parsed LeekScript document with a root you can replace or transform.
#[derive(Debug, Clone)]
pub struct LeekDoc {
    source: Vec<u8>,
    root: SyntaxNode,
    line_index: LineIndex,
}

impl LeekDoc {
    /// Parse and wrap as an editable document.
    ///
    /// Uses [`crate::parse::parse_doc_with_recovery`] so formatting and directive scanning can
    /// proceed past local errors (same as a typical IDE buffer). Use [`crate::parse::parse_doc`]
    /// when you need a strict parse.
    pub fn parse(src: &str, version: Version) -> Result<Self, ParseError> {
        parse_doc_with_recovery(src, version).map(|r| Self::from_parsed(&r.doc))
    }

    /// Take ownership of a parse result as an editable document (clones the source buffer).
    pub fn from_parsed(doc: &ParsedDoc) -> Self {
        let source = doc.source().to_vec();
        let line_index = LineIndex::new(&source);
        Self {
            source,
            root: doc.root().clone(),
            line_index,
        }
    }

    /// Syntax tree root (sipha red tree).
    #[inline]
    pub fn root_syntax(&self) -> &SyntaxNode {
        &self.root
    }

    /// Typed CST root, if this node is still a LeekScript `Root`.
    pub fn root_ast(&self) -> Option<Root> {
        Root::cast(self.root.clone())
    }

    /// Raw source bytes (kept aligned with the tree by [`Self::refresh_source_from_tree`],
    /// [`Self::apply_transform`], [`Self::reparse`], etc.).
    #[inline]
    pub fn source(&self) -> &[u8] {
        &self.source
    }

    /// Source as UTF-8, or empty if invalid UTF-8.
    #[inline]
    pub fn source_str(&self) -> &str {
        std::str::from_utf8(&self.source).unwrap_or("")
    }

    #[inline]
    pub fn line_index(&self) -> &LineIndex {
        &self.line_index
    }

    #[inline]
    pub fn offset_to_line_col(&self, offset: Pos) -> (u32, u32) {
        self.line_index.line_col(offset)
    }

    #[inline]
    pub fn offset_to_line_col_1based(&self, offset: Pos) -> (u32, u32) {
        self.line_index.line_col_1based(offset)
    }

    #[inline]
    pub fn span_slice(&self, span: Span) -> &[u8] {
        span.as_slice(&self.source)
    }

    #[inline]
    pub fn snippet_at(&self, offset: Pos) -> String {
        self.line_index.snippet_at(&self.source, offset)
    }

    /// Format a semantic diagnostic using this document’s source and line index.
    pub fn format_semantic_diagnostic(&self, diagnostic: &SemanticDiagnostic) -> String {
        diagnostic.format_with_source(&self.source, &self.line_index)
    }

    /// Smallest [`SyntaxNode`] whose range contains `offset`, if any.
    #[inline]
    #[must_use]
    pub fn node_at_offset(&self, offset: Pos) -> Option<SyntaxNode> {
        self.root.node_at_offset(offset)
    }

    /// Typed CST wrapper covering `offset`: deepest node first, then ancestors until `N::cast` works.
    ///
    /// See [`crate::visit::typed_at_offset`].
    #[inline]
    #[must_use]
    pub fn typed_at_offset<N: AstNode>(&self, offset: Pos) -> Option<N> {
        crate::visit::typed_at_offset(&self.root, offset)
    }

    /// Walk this document’s root. Requires the `walk` feature (enabled by default).
    #[cfg(feature = "walk")]
    #[inline]
    pub fn walk(&self, visitor: &mut impl Visitor, options: &WalkOptions) -> WalkResult {
        self.root.walk(visitor, options)
    }

    /// Replace the half-open byte range `span` with `replacement`, then align the CST.
    ///
    /// With the **`partial-reparse`** feature (enabled in default features), this first attempts
    /// sipha’s partial reparse at a statement boundary using the correct dialect [`Version`]. If
    /// that does not apply, or sipha falls back to incremental reuse (which does not carry
    /// dialect flags), this performs a full [`parse_doc`] on the spliced text.
    ///
    /// For in-place green-tree edits without any parse pass, use [`Self::apply_transform`] or
    /// [`Self::set_syntax_root`].
    pub fn replace_span(
        &mut self,
        span: Span,
        replacement: &str,
        version: Version,
    ) -> Result<(), DocEditError> {
        let start = span.start as usize;
        let end = span.end as usize;
        if start > end || end > self.source.len() {
            return Err(DocEditError::InvalidSpan);
        }
        let mut out =
            Vec::with_capacity(self.source.len().saturating_sub(end - start) + replacement.len());
        out.extend_from_slice(&self.source[..start]);
        out.extend_from_slice(replacement.as_bytes());
        out.extend_from_slice(&self.source[end..]);
        let text = String::from_utf8(out).map_err(|_| DocEditError::InvalidUtf8)?;

        #[cfg(feature = "partial-reparse")]
        {
            let edit = TextEdit {
                start: span.start,
                end: span.end,
                new_text: replacement.as_bytes().to_vec(),
            };
            let edits = [edit];
            let built = crate::grammar::built_graph();
            let graph = built.as_graph();
            if let Some(stmt_rule) = graph.rule_id("stmt") {
                let config = PartialReparseConfig {
                    boundary_kinds: STMT_BOUNDARY_KINDS,
                    boundary_rule: stmt_rule,
                    context: version.to_parse_context(),
                };
                let mut engine = Engine::new();
                if let Ok(outcome) = reparse_partial_or_fallback(
                    &mut engine,
                    &graph,
                    &self.source,
                    &self.root,
                    &edits,
                    &config,
                ) {
                    if outcome.used_partial {
                        if let Some(root) = outcome.root {
                            self.source = TextEdit::apply_edits(&self.source, &edits);
                            self.root = root;
                            self.line_index = LineIndex::new(&self.source);
                            return Ok(());
                        }
                    }
                }
            }
        }

        let doc = parse_doc(&text, version).map_err(DocEditError::Reparse)?;
        self.source = doc.source().to_vec();
        self.root = doc.root().clone();
        self.line_index = LineIndex::new(&self.source);
        Ok(())
    }

    /// Replace the syntax root (for example after building a new green tree) and refresh
    /// [`Self::source`] from [`SyntaxNode::collect_text`].
    pub fn set_syntax_root(&mut self, root: SyntaxNode) {
        self.root = root;
        self.refresh_source_from_tree();
    }

    /// Set [`Self::source`] from the concatenation of all token texts in the current tree.
    ///
    /// Offsets on the red tree are only meaningful relative to this buffer. Call this after
    /// manual edits to the green layer so diagnostics and slices match.
    pub fn refresh_source_from_tree(&mut self) {
        self.source = self.root.collect_text().into_bytes();
        self.line_index = LineIndex::new(&self.source);
    }

    /// Run a sipha [`Transformer`] top-down and refresh source from the new tree.
    ///
    /// Requires the `transform` Cargo feature (enabled by default).
    #[cfg(feature = "transform")]
    pub fn apply_transform(&mut self, transformer: &mut impl sipha::tree::transform::Transformer) {
        self.root = sipha::tree::transform::transform(&self.root, transformer);
        self.refresh_source_from_tree();
    }

    /// Parse [`Self::source_str`] again with the given dialect version.
    ///
    /// Use after external edits to the buffer, or to re-validate after a transform.
    pub fn reparse(&mut self, version: Version) -> Result<(), ParseError> {
        let doc = parse_doc(self.source_str(), version)?;
        self.source = doc.source().to_vec();
        self.root = doc.root().clone();
        self.line_index = LineIndex::new(&self.source);
        Ok(())
    }

    /// Emit the tree to text (optionally stripping trivia), then parse it back.
    ///
    /// Typical use: formatting / pretty-print pipelines. Fails if the emitted text is not
    /// valid LeekScript. Requires the `emit` Cargo feature (enabled by default).
    #[cfg(feature = "emit")]
    pub fn reparse_after_emit(
        &mut self,
        version: Version,
        options: &EmitOptions,
    ) -> Result<(), ParseError> {
        let text = syntax_root_to_string(&self.root, options);
        self.source = text.into_bytes();
        self.line_index = LineIndex::new(&self.source);
        let doc = parse_doc(self.source_str(), version)?;
        self.root = doc.root().clone();
        Ok(())
    }
}

#[cfg(feature = "transform")]
pub use sipha::tree::transform::{TransformResult, Transformer, transform};
