//! CST pretty-printer (non-trivia) with block layout and spacing rules.

use crate::document::LeekDoc;
use crate::format::directives::{span_touches_preserve, DirectivePlan};
use crate::format::options::{BraceStyle, FormatOptions};
use crate::format::spacing::needs_space_between;
use crate::parse::{ParseError, Version};
use crate::syntax::kinds::K;
use sipha::tree::red::{SyntaxElement, SyntaxNode, SyntaxToken};
use sipha::types::{Pos, Span};

/// Parse and format a LeekScript document.
pub fn format_document(src: &str, version: Version, base: &FormatOptions) -> Result<String, ParseError> {
    let doc = LeekDoc::parse(src, version)?;
    Ok(format_leek_doc(&doc, base))
}

/// Format an already-parsed document (re-scans `// leekfmt:` directives from trivia).
#[must_use]
pub fn format_leek_doc(doc: &LeekDoc, base: &FormatOptions) -> String {
    let plan = crate::format::directives::scan_directives(doc);
    let mut p = Printer {
        source: doc.source(),
        base,
        plan: &plan,
        out: String::with_capacity(doc.source().len().saturating_mul(2)),
        prev_kind: None,
        indent_level: 0,
    };
    p.format_root(doc.root_syntax());
    let end_opts = FormatOptions::effective_at(base, &plan.patches, u32::MAX);
    if end_opts.trailing_newline && !p.out.ends_with(['\n', '\r']) {
        p.out.push_str(end_opts.line_ending.as_str());
    }
    p.out
}

struct Printer<'a> {
    source: &'a [u8],
    base: &'a FormatOptions,
    plan: &'a DirectivePlan,
    out: String,
    prev_kind: Option<K>,
    indent_level: u32,
}

impl Printer<'_> {
    fn opts_at(&self, offset: u32) -> FormatOptions {
        FormatOptions::effective_at(self.base, &self.plan.patches, offset)
    }

    fn emit_verbatim_span(&mut self, span: Span) {
        let len = self.source.len() as Pos;
        let end = span.end.min(len);
        let start = span.start.min(end);
        let s = Span::new(start, end).as_slice(self.source);
        self.out.push_str(std::str::from_utf8(s).unwrap_or(""));
        self.prev_kind = None;
    }

    fn emit_newline(&mut self, offset: u32) {
        self.out.push_str(self.opts_at(offset).line_ending.as_str());
    }

    fn emit_indent(&mut self, offset: u32) {
        let o = self.opts_at(offset);
        let unit = o.indent_unit();
        for _ in 0..self.indent_level {
            self.out.push_str(&unit);
        }
    }

    fn format_root(&mut self, root: &SyntaxNode) {
        let stmts: Vec<SyntaxNode> = root.child_nodes().collect();
        for (i, node) in stmts.iter().enumerate() {
            let span = node.text_range();
            if span_touches_preserve(span, &self.plan.preserve) {
                self.emit_verbatim_span(span);
            } else {
                self.format_node(node);
            }
            if i + 1 < stmts.len() {
                let next_off = stmts[i + 1].offset();
                self.emit_newline(next_off);
                let o = self.opts_at(next_off);
                for _ in 0..o.blank_lines_between_top_level {
                    self.emit_newline(next_off);
                }
            }
        }
    }

    fn format_node(&mut self, node: &SyntaxNode) {
        let span = node.text_range();
        if span_touches_preserve(span, &self.plan.preserve) {
            self.emit_verbatim_span(span);
            return;
        }
        if node.kind_as::<K>() == Some(K::Block) {
            self.format_block(node);
            return;
        }
        self.format_flat_node(node);
    }

    fn format_block(&mut self, block: &SyntaxNode) {
        let children: Vec<SyntaxElement> = block
            .children()
            .filter(|e| !e.is_trivia())
            .collect();

        let mut i = 0usize;
        let mut need_newline_before_stmt = false;

        while i < children.len() {
            match &children[i] {
                SyntaxElement::Token(t) => {
                    let Some(k) = t.kind_as::<K>() else {
                        i += 1;
                        continue;
                    };
                    if k == K::LBrace {
                        let off = t.text_range().start;
                        self.write_semantic_token(t);
                        self.emit_newline(off);
                        self.indent_level = self.indent_level.saturating_add(1);
                        need_newline_before_stmt = false;
                        i += 1;
                        continue;
                    }
                    if k == K::RBrace {
                        let off = t.text_range().start;
                        self.indent_level = self.indent_level.saturating_sub(1);
                        self.emit_newline(off);
                        self.emit_indent(off);
                        self.write_semantic_token(t);
                        i += 1;
                        continue;
                    }
                    self.write_semantic_token(t);
                    i += 1;
                }
                SyntaxElement::Node(n) => {
                    let off = n.offset();
                    if need_newline_before_stmt {
                        self.emit_newline(off);
                    }
                    need_newline_before_stmt = true;
                    self.emit_indent(off);
                    self.format_node(n);
                    i += 1;
                }
            }
        }
    }

    fn format_flat_node(&mut self, node: &SyntaxNode) {
        for el in node.children() {
            if el.is_trivia() {
                continue;
            }
            match el {
                SyntaxElement::Node(n) => self.format_node(&n),
                SyntaxElement::Token(t) => self.write_semantic_token(&t),
            }
        }
    }

    fn write_semantic_token(&mut self, t: &SyntaxToken) {
        let Some(k) = t.kind_as::<K>() else {
            self.out.push_str(t.text());
            return;
        };
        let off = t.text_range().start;
        let o = self.opts_at(off);

        if k == K::ElseKw && o.newline_before_else_catch_finally && self.prev_kind == Some(K::RBrace) {
            self.emit_newline(off);
            self.emit_indent(off);
        }
        if matches!(k, K::CatchKw | K::FinallyKw)
            && o.newline_before_else_catch_finally
            && self.prev_kind == Some(K::RBrace)
        {
            self.emit_newline(off);
            self.emit_indent(off);
        }

        if needs_space_between(self.prev_kind, k, &o) {
            self.out.push(' ');
        }

        if k == K::LBrace && o.brace_style == BraceStyle::NextLine && self.prev_kind == Some(K::RParen) {
            self.emit_newline(off);
            self.emit_indent(off);
        }

        self.out.push_str(t.text());
        self.prev_kind = Some(k);
    }
}
