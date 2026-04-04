//! CST pretty-printer (non-trivia) with block layout and spacing rules.

use crate::ast::ReturnStmt;
use crate::document::LeekDoc;
use crate::format::directives::{
    DirectivePlan, preserve_region_overlapping, span_touches_preserve,
};
use crate::format::options::{BraceStyle, FormatOptions, SemicolonStyle};
use crate::format::spacing::needs_space_between;
use crate::parse::{LanguageOptions, ParseError};
use crate::syntax::kinds::K;
use crate::syntax::syntax_el_is_trivia;
use sipha::prelude::AstNode;
use sipha::tree::red::{SyntaxElement, SyntaxNode, SyntaxToken};
use sipha::types::{Pos, Span};

/// Previous sibling [`SyntaxNode`] among `children` before index `i` (skips tokens).
fn prev_child_node(children: &[SyntaxElement], i: usize) -> Option<&SyntaxNode> {
    let mut j = i;
    while j > 0 {
        j -= 1;
        if let SyntaxElement::Node(n) = &children[j] {
            return Some(n);
        }
    }
    None
}

/// `ClassMember` fields (`integer x;`, `x = 1`) have no direct [`K::Block`] child; methods and
/// constructors do.
fn class_member_is_field_like(cm: &SyntaxNode) -> bool {
    cm.kind_as::<K>() == Some(K::ClassMember)
        && !cm.child_nodes().any(|c| c.kind_as::<K>() == Some(K::Block))
}

#[inline]
fn stmt_kind_optional_trailing_semi(k: K) -> bool {
    matches!(
        k,
        K::ReturnStmt
            | K::BreakStmt
            | K::ContinueStmt
            | K::IncludeStmt
            | K::VarDecl
            | K::GlobalDecl
            | K::ConstDecl
            | K::ThrowStmt
            | K::ImportStmt
            | K::GotoStmt
            | K::PackageStmt
            | K::DoWhileStmt
            | K::Stmt
    )
}

/// Eligible for [`SemicolonStyle`] on an optional trailing `;` (class fields, not methods).
fn node_has_optional_trailing_semicolon_policy(node: &SyntaxNode) -> bool {
    match node.kind_as::<K>() {
        Some(K::ClassMember) => class_member_is_field_like(node),
        Some(k) => stmt_kind_optional_trailing_semi(k),
        None => false,
    }
}

/// `return;` / `break;` / `continue;` — keep or insert `;` in [`SemicolonStyle::OnlyNeeded`].
fn only_needed_requires_trailing_semicolon(node: &SyntaxNode) -> bool {
    match node.kind_as::<K>() {
        Some(K::ReturnStmt) => ReturnStmt::cast(node.clone()).is_some_and(|r| r.expr().is_none()),
        Some(K::BreakStmt) | Some(K::ContinueStmt) => true,
        _ => false,
    }
}

/// No extra blank between consecutive class **fields**; still separate methods and field↔method.
fn skip_class_body_member_gap(prev: Option<&SyntaxNode>, curr: &SyntaxNode) -> bool {
    if curr.kind_as::<K>() != Some(K::ClassMember) {
        return false;
    }
    let Some(p) = prev else {
        return false;
    };
    if p.kind_as::<K>() != Some(K::ClassMember) {
        return false;
    }
    class_member_is_field_like(p) && class_member_is_field_like(curr)
}

#[inline]
fn str_display_width(s: &str, tab_width: usize) -> usize {
    s.chars()
        .map(|c| if c == '\t' { tab_width.max(1) } else { 1 })
        .sum()
}

/// Parse and format a LeekScript document.
pub fn format_document(
    src: &str,
    lang: impl Into<LanguageOptions>,
    base: &FormatOptions,
) -> Result<String, ParseError> {
    let doc = LeekDoc::parse(src, lang)?;
    Ok(format_leek_doc(&doc, base))
}

/// Format an already-parsed document (re-scans `leekfmt:` directives from trivia).
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
        line_col: 0,
        type_syntax_depth: 0,
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
    /// Visual column after the last character on the current line (0 right of newline).
    line_col: usize,
    /// Nesting of type-syntax nodes ([`K::TypeExpr`], unions, generics, …) for compact `|`, `<`, `>`.
    type_syntax_depth: u32,
}

impl Printer<'_> {
    fn opts_at(&self, offset: u32) -> FormatOptions {
        FormatOptions::effective_at(self.base, &self.plan.patches, offset)
    }

    fn push_str_no_nl(&mut self, s: &str, tab_width: usize) {
        self.line_col += str_display_width(s, tab_width);
        self.out.push_str(s);
    }

    fn emit_verbatim_span(&mut self, span: Span) {
        let len = self.source.len() as Pos;
        let end = span.end.min(len);
        let start = span.start.min(end);
        let s = Span::new(start, end).as_slice(self.source);
        let text = std::str::from_utf8(s).unwrap_or("");
        self.out.push_str(text);
        self.prev_kind = None;
        let tab_width = self.opts_at(start).tab_width.max(1);
        if text.contains('\n') {
            let tail = text.rsplit_once('\n').map(|(_, l)| l).unwrap_or(text);
            self.line_col = str_display_width(tail.trim_end_matches('\r'), tab_width);
        } else {
            self.line_col += str_display_width(text, tab_width);
        }
    }

    fn emit_newline(&mut self, offset: u32) {
        self.out.push_str(self.opts_at(offset).line_ending.as_str());
        self.line_col = 0;
    }

    fn emit_indent(&mut self, offset: u32) {
        let o = self.opts_at(offset);
        let unit = o.indent_unit();
        let tw = o.tab_width.max(1);
        for _ in 0..self.indent_level {
            self.push_str_no_nl(&unit, tw);
        }
    }

    /// One extra indent unit for wrapped continuation lines (after a comma break).
    fn emit_indent_with_continuation(&mut self, offset: u32) {
        let o = self.opts_at(offset);
        let unit = o.indent_unit();
        let tw = o.tab_width.max(1);
        let total = self.indent_level.saturating_add(1);
        for _ in 0..total {
            self.push_str_no_nl(&unit, tw);
        }
    }

    fn format_root(&mut self, root: &SyntaxNode) {
        let stmts: Vec<SyntaxNode> = root
            .child_nodes()
            .filter(|n| n.kind_as::<K>() != Some(K::Trivia))
            .collect();
        let mut i = 0usize;
        while i < stmts.len() {
            let node = &stmts[i];
            let span = node.text_range();
            if let Some(pv) = preserve_region_overlapping(span, &self.plan.preserve) {
                if i > 0 {
                    let next_off = stmts[i].offset();
                    self.emit_newline(next_off);
                    let o = self.opts_at(next_off);
                    for _ in 0..o.blank_lines_between_top_level {
                        self.emit_newline(next_off);
                    }
                    if stmts[i - 1].kind_as::<K>() == Some(K::ClassDecl) {
                        for _ in 0..o.blank_lines_after_class {
                            self.emit_newline(next_off);
                        }
                    }
                    self.prev_kind = None;
                }
                self.emit_verbatim_span(pv);
                // Skip every top-level node that intersects `pv`. Using only `end <= pv.end` can
                // stall forever when a node's span overlaps the preserve region but extends past
                // `pv.end` (e.g. leading line comment trivia on the first preserved statement).
                while i < stmts.len() {
                    let s = stmts[i].text_range();
                    if s.start >= pv.end || s.end <= pv.start {
                        break;
                    }
                    i += 1;
                }
                self.prev_kind = None;
                continue;
            }
            if i > 0 {
                let next_off = stmts[i].offset();
                self.emit_newline(next_off);
                let o = self.opts_at(next_off);
                for _ in 0..o.blank_lines_between_top_level {
                    self.emit_newline(next_off);
                }
                if stmts[i - 1].kind_as::<K>() == Some(K::ClassDecl) {
                    for _ in 0..o.blank_lines_after_class {
                        self.emit_newline(next_off);
                    }
                }
                self.prev_kind = None;
            }
            self.format_node(node, None);
            i += 1;
        }
    }

    fn format_node(&mut self, node: &SyntaxNode, parent: Option<&SyntaxNode>) {
        let span = node.text_range();
        if span_touches_preserve(span, &self.plan.preserve) {
            self.emit_verbatim_span(span);
            return;
        }
        if node.kind_as::<K>() == Some(K::Block) {
            self.format_block(node, parent);
            return;
        }
        let bump_type = matches!(
            node.kind_as::<K>(),
            Some(
                K::TypeExpr
                    | K::TypeUnionType
                    | K::TypeNullableType
                    | K::TypePrimaryType
                    | K::BuiltinTypeNameExpr
                    | K::TemplateParams
            )
        );
        if bump_type {
            self.type_syntax_depth += 1;
        }
        if node_has_optional_trailing_semicolon_policy(node) {
            self.format_node_optional_trailing_semi(node);
        } else {
            self.format_flat_node(node);
        }
        if bump_type {
            self.type_syntax_depth -= 1;
        }
    }

    fn format_block(&mut self, block: &SyntaxNode, parent_of_block: Option<&SyntaxNode>) {
        let is_class_body = parent_of_block.is_some_and(|p| p.kind_as::<K>() == Some(K::ClassDecl));
        let children: Vec<SyntaxElement> = block
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
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
                        // Statement starts on a new line — reset so `)`/`,` from the previous
                        // stmt does not insert a leading space on this line (e.g. after `);`).
                        self.prev_kind = None;
                        let o = self.opts_at(off);
                        let mut extra = if is_class_body {
                            if skip_class_body_member_gap(prev_child_node(&children, i), n) {
                                0
                            } else {
                                o.blank_lines_between_class_members
                            }
                        } else {
                            o.blank_lines_between_block_statements
                        };
                        if o.max_consecutive_blank_lines_in_block > 0 {
                            extra = extra.min(o.max_consecutive_blank_lines_in_block);
                        }
                        for _ in 0..extra {
                            self.emit_newline(off);
                        }
                    }
                    need_newline_before_stmt = true;
                    self.emit_indent(off);
                    self.format_node(n, Some(block));
                    i += 1;
                }
            }
        }
    }

    fn format_flat_node(&mut self, node: &SyntaxNode) {
        for el in node.children() {
            if syntax_el_is_trivia(&el) {
                continue;
            }
            match el {
                SyntaxElement::Node(n) => self.format_node(&n, Some(node)),
                SyntaxElement::Token(t) => self.write_semantic_token(&t),
            }
        }
    }

    /// Optional trailing `;` on statements / class fields — see [`SemicolonStyle`].
    fn format_node_optional_trailing_semi(&mut self, node: &SyntaxNode) {
        let children: Vec<SyntaxElement> = node
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();
        let mut emitted_semi = false;
        for el in &children {
            match el {
                SyntaxElement::Node(n) => self.format_node(n, Some(node)),
                SyntaxElement::Token(t) => {
                    let Some(k) = t.kind_as::<K>() else {
                        self.write_semantic_token(t);
                        continue;
                    };
                    if k != K::Semi {
                        self.write_semantic_token(t);
                        continue;
                    }
                    let off = t.text_range().start;
                    let style = self.opts_at(off).semicolon_style;
                    let write = match style {
                        SemicolonStyle::Preserve => true,
                        SemicolonStyle::Always => true,
                        SemicolonStyle::OnlyNeeded => only_needed_requires_trailing_semicolon(node),
                    };
                    if write {
                        self.write_semantic_token(t);
                        emitted_semi = true;
                    }
                }
            }
        }

        let tail_off = node.text_range().end.saturating_sub(1);
        let style = self.opts_at(tail_off).semicolon_style;
        match style {
            SemicolonStyle::Always if !emitted_semi => self.emit_synthetic_semicolon(tail_off),
            SemicolonStyle::OnlyNeeded
                if !emitted_semi && only_needed_requires_trailing_semicolon(node) =>
            {
                self.emit_synthetic_semicolon(tail_off);
            }
            _ => {}
        }
    }

    fn emit_synthetic_semicolon(&mut self, byte_offset: u32) {
        let o = self.opts_at(byte_offset);
        let in_type = self.type_syntax_depth > 0;
        let tab_w = o.tab_width.max(1);
        if needs_space_between(self.prev_kind, K::Semi, &o, in_type) {
            self.push_str_no_nl(" ", tab_w);
        }
        self.push_str_no_nl(";", tab_w);
        self.prev_kind = Some(K::Semi);
    }

    fn write_semantic_token(&mut self, t: &SyntaxToken) {
        let off = t.text_range().start;
        let o = self.opts_at(off);
        let tab_w = o.tab_width.max(1);

        let Some(k) = t.kind_as::<K>() else {
            self.push_str_no_nl(t.text(), tab_w);
            self.prev_kind = None;
            return;
        };

        if k == K::ElseKw
            && o.newline_before_else_catch_finally
            && self.prev_kind == Some(K::RBrace)
        {
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

        let in_type = self.type_syntax_depth > 0;
        let mut comma_wrapped = false;
        if self.prev_kind == Some(K::Comma) && o.line_width > 0 {
            let need_space = needs_space_between(self.prev_kind, k, &o, in_type);
            let gap = usize::from(need_space);
            let token_w = str_display_width(t.text(), tab_w);
            if self.line_col.saturating_add(gap).saturating_add(token_w) > o.line_width {
                self.emit_newline(off);
                self.emit_indent_with_continuation(off);
                comma_wrapped = true;
            }
        }

        if !comma_wrapped && needs_space_between(self.prev_kind, k, &o, in_type) {
            self.push_str_no_nl(" ", tab_w);
        }

        if k == K::LBrace
            && o.brace_style == BraceStyle::NextLine
            && self.prev_kind == Some(K::RParen)
        {
            self.emit_newline(off);
            self.emit_indent(off);
        }

        self.push_str_no_nl(t.text(), tab_w);
        self.prev_kind = Some(k);
    }
}
