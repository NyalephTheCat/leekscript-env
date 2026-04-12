//! CST pretty-printer (non-trivia) with block layout and spacing rules.

use crate::ast::ReturnStmt;
use crate::document::LeekDoc;
use crate::format::directives::{
    DirectivePlan, preserve_region_overlapping, span_touches_preserve,
};
use crate::format::options::{BraceStyle, FormatOptions, SemicolonStyle};
use crate::format::spacing::needs_space_between;
use crate::parse::{LanguageOptions, ParseError};
use crate::syntax::kinds::{Lex, Node};
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

/// `ClassMember` fields (`integer x;`, `x = 1`) have no direct [`Node::Block`] child; methods and
/// constructors do.
fn class_member_is_field_like(cm: &SyntaxNode) -> bool {
    cm.kind_as::<Node>() == Some(Node::ClassMember)
        && !cm
            .child_nodes()
            .any(|c| c.kind_as::<Node>() == Some(Node::Block))
}

#[inline]
fn stmt_kind_optional_trailing_semi(k: Node) -> bool {
    matches!(
        k,
        Node::ReturnStmt
            | Node::BreakStmt
            | Node::ContinueStmt
            | Node::IncludeStmt
            | Node::VarDecl
            | Node::GlobalDecl
            | Node::ConstDecl
            | Node::ThrowStmt
            | Node::ImportStmt
            | Node::GotoStmt
            | Node::PackageStmt
            | Node::DoWhileStmt
            | Node::Stmt
    )
}

/// Eligible for [`SemicolonStyle`] on an optional trailing `;` (class fields, not methods).
fn node_has_optional_trailing_semicolon_policy(node: &SyntaxNode) -> bool {
    match node.kind_as::<Node>() {
        Some(Node::ClassMember) => class_member_is_field_like(node),
        Some(k) => stmt_kind_optional_trailing_semi(k),
        None => false,
    }
}

/// `return;` / `break;` / `continue;` — keep or insert `;` in [`SemicolonStyle::OnlyNeeded`].
fn only_needed_requires_trailing_semicolon(node: &SyntaxNode) -> bool {
    match node.kind_as::<Node>() {
        Some(Node::ReturnStmt) => {
            ReturnStmt::cast(node.clone()).is_some_and(|r| r.expr().is_none())
        }
        Some(Node::BreakStmt) | Some(Node::ContinueStmt) => true,
        _ => false,
    }
}

/// No extra blank between consecutive class **fields**; still separate methods and field↔method.
fn skip_class_body_member_gap(prev: Option<&SyntaxNode>, curr: &SyntaxNode) -> bool {
    if curr.kind_as::<Node>() != Some(Node::ClassMember) {
        return false;
    }
    let Some(p) = prev else {
        return false;
    };
    if p.kind_as::<Node>() != Some(Node::ClassMember) {
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
    prev_kind: Option<Lex>,
    indent_level: u32,
    /// Visual column after the last character on the current line (0 right of newline).
    line_col: usize,
    /// Nesting of type-syntax nodes ([`Node::TypeExpr`], unions, generics, …) for compact `|`, `<`, `>`.
    type_syntax_depth: u32,
}

/// How many **extra** empty lines appear in source trivia between `prev_end` and `next_start`
/// (half-open byte range into UTF-8 source): one `\n` closes the previous statement; each further
/// `\n` opens another visual line (blank if only whitespace follows before the next stmt).
#[inline]
fn extra_blank_lines_in_source_gap(source: &[u8], prev_end: u32, next_start: u32) -> usize {
    let (a, b) = if prev_end <= next_start {
        (prev_end as usize, next_start as usize)
    } else {
        (next_start as usize, prev_end as usize)
    };
    if a >= b || b > source.len() {
        return 0;
    }
    let newlines = source[a..b].iter().filter(|&&ch| ch == b'\n').count();
    newlines.saturating_sub(1)
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
            .filter(|n| n.kind_as::<Node>() != Some(Node::Trivia))
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
                    if stmts[i - 1].kind_as::<Node>() == Some(Node::ClassDecl) {
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
                if stmts[i - 1].kind_as::<Node>() == Some(Node::ClassDecl) {
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
        if node.kind_as::<Node>() == Some(Node::Block) {
            self.format_block(node, parent);
            return;
        }
        let bump_type = matches!(
            node.kind_as::<Node>(),
            Some(
                Node::TypeExpr
                    | Node::TypeUnionType
                    | Node::TypeNullableType
                    | Node::TypePrimaryType
                    | Node::BuiltinTypeNameExpr
                    | Node::TemplateParams
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
        let is_class_body =
            parent_of_block.is_some_and(|p| p.kind_as::<Node>() == Some(Node::ClassDecl));
        let children: Vec<SyntaxElement> = block
            .children()
            .filter(|e| !syntax_el_is_trivia(e))
            .collect();

        let mut i = 0usize;
        let mut need_newline_before_stmt = false;
        let mut prev_stmt_node: Option<SyntaxNode> = None;

        while i < children.len() {
            match &children[i] {
                SyntaxElement::Token(t) => {
                    let Some(k) = t.kind_as::<Lex>() else {
                        i += 1;
                        continue;
                    };
                    if k == Lex::LBrace {
                        let off = t.text_range().start;
                        self.write_semantic_token(t);
                        self.emit_newline(off);
                        self.indent_level = self.indent_level.saturating_add(1);
                        need_newline_before_stmt = false;
                        i += 1;
                        continue;
                    }
                    if k == Lex::RBrace {
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
                        let skip_gap = is_class_body
                            && skip_class_body_member_gap(prev_child_node(&children, i), n);
                        let mut extra = if skip_gap {
                            0
                        } else if is_class_body {
                            o.blank_lines_between_class_members
                        } else {
                            o.blank_lines_between_block_statements
                        };
                        if o.max_consecutive_blank_lines_in_block > 0 {
                            extra = extra.min(o.max_consecutive_blank_lines_in_block);
                        }
                        let cap = if o.max_consecutive_blank_lines_in_block > 0 {
                            o.max_consecutive_blank_lines_in_block
                        } else {
                            10usize
                        };
                        if !skip_gap {
                            if let Some(ref prev) = prev_stmt_node {
                                // Leading whitespace/newlines before the next stmt live as trivia on its
                                // first semantic token, not in `[prev.end, n.offset())` (often empty).
                                let gap_end = n
                                    .descendant_semantic_tokens()
                                    .first()
                                    .map(|t| t.text_range().start)
                                    .unwrap_or(n.offset());
                                let src_extra = extra_blank_lines_in_source_gap(
                                    self.source,
                                    prev.text_range().end,
                                    gap_end,
                                )
                                .min(cap);
                                extra = extra.max(src_extra);
                            }
                        }
                        if o.max_consecutive_blank_lines_in_block > 0 {
                            extra = extra.min(o.max_consecutive_blank_lines_in_block);
                        } else {
                            extra = extra.min(10);
                        }
                        for _ in 0..extra {
                            self.emit_newline(off);
                        }
                    }
                    need_newline_before_stmt = true;
                    self.emit_indent(off);
                    self.format_node(n, Some(block));
                    prev_stmt_node = Some(n.clone());
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
                    let Some(k) = t.kind_as::<Lex>() else {
                        self.write_semantic_token(t);
                        continue;
                    };
                    if k != Lex::Semi {
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
        if needs_space_between(self.prev_kind, Lex::Semi, &o, in_type) {
            self.push_str_no_nl(" ", tab_w);
        }
        self.push_str_no_nl(";", tab_w);
        self.prev_kind = Some(Lex::Semi);
    }

    fn write_semantic_token(&mut self, t: &SyntaxToken) {
        let off = t.text_range().start;
        let o = self.opts_at(off);
        let tab_w = o.tab_width.max(1);

        let Some(k) = t.kind_as::<Lex>() else {
            self.push_str_no_nl(t.text(), tab_w);
            self.prev_kind = None;
            return;
        };

        if k == Lex::ElseKw
            && o.newline_before_else_catch_finally
            && self.prev_kind == Some(Lex::RBrace)
        {
            self.emit_newline(off);
            self.emit_indent(off);
        }
        if matches!(k, Lex::CatchKw | Lex::FinallyKw)
            && o.newline_before_else_catch_finally
            && self.prev_kind == Some(Lex::RBrace)
        {
            self.emit_newline(off);
            self.emit_indent(off);
        }

        let in_type = self.type_syntax_depth > 0;
        let mut comma_wrapped = false;
        if self.prev_kind == Some(Lex::Comma) && o.line_width > 0 {
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

        if k == Lex::LBrace
            && o.brace_style == BraceStyle::NextLine
            && self.prev_kind == Some(Lex::RParen)
        {
            self.emit_newline(off);
            self.emit_indent(off);
        }

        self.push_str_no_nl(t.text(), tab_w);
        self.prev_kind = Some(k);
    }

    /// Range-format entry: format `node` without extra leading indent (outer layout/trivia is preserved separately).
    fn finish_range_format_subtree(&mut self, node: &SyntaxNode, parent: Option<&SyntaxNode>) {
        self.format_node(node, parent);
    }
}

#[inline]
fn same_syntax_node(a: &SyntaxNode, b: &SyntaxNode) -> bool {
    a.offset() == b.offset() && a.text_len() == b.text_len() && a.kind() == b.kind()
}

/// Immediate parent via DFS — [`SyntaxNode::ancestors`] can return empty when `find_path` misses
/// (e.g. some recovery shapes); this keeps range selection walk-up reliable.
fn immediate_parent_of_node(root: &SyntaxNode, target: &SyntaxNode) -> Option<SyntaxNode> {
    fn walk(here: &SyntaxNode, target: &SyntaxNode) -> Option<SyntaxNode> {
        for c in here.child_nodes() {
            if same_syntax_node(&c, target) {
                return Some(here.clone());
            }
            if let Some(p) = walk(&c, target) {
                return Some(p);
            }
        }
        None
    }
    walk(root, target)
}

#[inline]
fn parent_of_node(root: &SyntaxNode, node: &SyntaxNode) -> Option<SyntaxNode> {
    if same_syntax_node(node, root) {
        return None;
    }
    node.ancestors(root)
        .into_iter()
        .next()
        .or_else(|| immediate_parent_of_node(root, node))
}

fn normalize_trivia_leaf(mut n: SyntaxNode, root: &SyntaxNode) -> Option<SyntaxNode> {
    while n.kind_as::<Node>() == Some(Node::Trivia) {
        n = parent_of_node(root, &n)?;
    }
    Some(n)
}

/// Smallest non-trivia [`SyntaxNode`] whose span contains `[start, end)` (UTF-8 bytes).
fn minimal_covering_node(root: &SyntaxNode, sel: Span) -> Option<SyntaxNode> {
    let start = sel.start.min(sel.end);
    let end = sel.start.max(sel.end);
    if start >= end {
        return None;
    }
    let rr = root.text_range();
    if start < rr.start || end > rr.end {
        return None;
    }
    let seed = root.node_at_offset(start)?;
    let mut n = normalize_trivia_leaf(seed, root)?;
    while n.text_range().start > start || n.text_range().end < end {
        let p = parent_of_node(root, &n)?;
        n = p;
    }
    loop {
        let candidates: Vec<SyntaxNode> = n
            .child_nodes()
            .filter(|c| c.kind_as::<Node>() != Some(Node::Trivia))
            .filter(|c| {
                let r = c.text_range();
                r.start <= start && r.end >= end
            })
            .collect();
        let Some(best) = candidates.into_iter().min_by_key(SyntaxNode::text_len) else {
            break;
        };
        if best.kind_as::<Node>() == Some(Node::Trivia) {
            break;
        }
        if same_syntax_node(&best, &n) {
            break;
        }
        n = best;
    }
    if n.kind_as::<Node>() == Some(Node::Trivia) {
        return None;
    }
    Some(n)
}

/// Printer [`indent_level`](Printer::indent_level) before formatting `node`, matching [`format_block`](Printer::format_block).
fn block_body_indent_depth(root: &SyntaxNode, node: &SyntaxNode) -> u32 {
    let mut count = 0u32;
    let mut cur = node.clone();
    loop {
        if same_syntax_node(&cur, root) {
            break;
        }
        let Some(p) = parent_of_node(root, &cur) else {
            break;
        };
        if p.kind_as::<Node>() == Some(Node::Block)
            && p.child_nodes().any(|c| same_syntax_node(&c, &cur))
        {
            count += 1;
        }
        cur = p;
    }
    count
}

fn format_leek_subtree_node(
    doc: &LeekDoc,
    root: &SyntaxNode,
    node: &SyntaxNode,
    base: &FormatOptions,
) -> Option<(Span, String)> {
    if node.kind_as::<Node>() == Some(Node::Root) {
        return None;
    }
    let parent = parent_of_node(root, node);
    // Match [`Printer::format_root`]: top-level statements use `parent: None`, not `Some(root)`.
    let parent_for_printer = parent.as_ref().filter(|p| !same_syntax_node(p, root));
    let toks = node.descendant_semantic_tokens();
    if toks.is_empty() {
        return None;
    }
    let first_tok = &toks[0];
    let last_tok = toks.last().expect("non-empty");
    let nr = node.text_range();
    let lead = std::str::from_utf8(
        &doc.source()[nr.start as usize..first_tok.text_range().start as usize],
    )
    .ok()?;
    let trail =
        std::str::from_utf8(&doc.source()[last_tok.text_range().end as usize..nr.end as usize])
            .ok()?;
    let plan = crate::format::directives::scan_directives(doc);
    let mut p = Printer {
        source: doc.source(),
        base,
        plan: &plan,
        out: String::with_capacity(node.text_len() as usize * 2),
        prev_kind: None,
        indent_level: block_body_indent_depth(root, node),
        line_col: 0,
        type_syntax_depth: 0,
    };
    p.finish_range_format_subtree(node, parent_for_printer);
    let mut out = String::with_capacity(lead.len() + p.out.len() + trail.len());
    out.push_str(lead);
    out.push_str(&p.out);
    out.push_str(trail);
    let span = node.text_range();
    let old = std::str::from_utf8(span.as_slice(doc.source())).unwrap_or("");
    if old == out {
        return None;
    }
    Some((span, out))
}

/// Format the smallest syntax subtree (or several top-level subtrees) that cover `selection`.
///
/// Returns one or more `(span, text)` replacements (sorted by descending `span.start`). [`None`] when
/// nothing changes or the selection cannot be resolved.
#[must_use]
pub fn format_leek_doc_range(
    doc: &LeekDoc,
    selection: Span,
    base: &FormatOptions,
) -> Option<Vec<(Span, String)>> {
    let root = doc.root_syntax();
    let node = minimal_covering_node(root, selection)?;
    let rr = root.text_range();
    if node.kind_as::<Node>() == Some(Node::Root) {
        let covers_whole_file = selection.start <= rr.start && selection.end >= rr.end;
        if covers_whole_file {
            let formatted = format_leek_doc(doc, base);
            if formatted == doc.source_str() {
                return None;
            }
            return Some(vec![(Span::new(0, doc.source().len() as u32), formatted)]);
        }
        let mut out: Vec<(Span, String)> = Vec::new();
        for c in root
            .child_nodes()
            .filter(|x| x.kind_as::<Node>() != Some(Node::Trivia))
        {
            let r = c.text_range();
            if r.end <= selection.start || r.start >= selection.end {
                continue;
            }
            let sub = format_leek_subtree_node(doc, root, &c, base);
            if let Some(pair) = sub {
                out.push(pair);
            }
        }
        if out.is_empty() {
            return None;
        }
        out.sort_by_key(|(s, _)| std::cmp::Reverse(s.start));
        return Some(out);
    }
    let one = format_leek_subtree_node(doc, root, &node, base)?;
    Some(vec![one])
}
