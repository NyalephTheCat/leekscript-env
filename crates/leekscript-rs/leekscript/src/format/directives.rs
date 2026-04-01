//! `// leekfmt: ...` and `/* leekfmt: ... */` directives.
//!
//! # Syntax
//!
//! - **File / scoped options** (from the directive through the rest of the file, superseded by later
//!   directives at a later byte offset):  
//!   `indent-width=4`, `indent-style=spaces`, `indent-style=tabs`, `brace-style=same-line`,
//!   `brace-style=next-line`, `line-width=100`, `blank-lines-between-top-level=1`,
//!   `space-after-keyword-before-paren=true`, `space-before-function-decl-paren=false`,
//!   `space-inside-parens=false`, `space-around-assign=true`, `space-around-binary-ops=true`,
//!   `newline-before-else-catch-finally=true`, `trailing-newline=true`,
//!   `max-consecutive-blank-lines-in-block=2`, `tab-width=4`
//!
//! - **Verbatim regions**  
//!   `off` … `on` — text between the end of the `off` comment and the start of the `on` comment is
//!   copied unchanged.  
//!   `ignore-next-line` — the entire next source line is preserved.  
//!   `skip-next-line` — alias of `ignore-next-line`.
//!
//! Several assignments can appear on one directive line, separated by `;` or `,`.

use crate::document::LeekDoc;
use crate::format::options::{BraceStyle, FormatPatch, LineEnding};
use crate::syntax::kinds::K;
use sipha::tree::red::SyntaxToken;
use sipha::tree::walk::{Visitor, WalkOptions, walk};
use sipha::types::{Pos, Span};

/// Collected verbatim spans and option patches (sorted by offset).
#[derive(Debug, Default)]
pub struct DirectivePlan {
    pub preserve: Vec<Span>,
    pub patches: Vec<(u32, FormatPatch)>,
}

impl DirectivePlan {
    pub fn merge_overlapping_preserve(&mut self) {
        if self.preserve.is_empty() {
            return;
        }
        self.preserve.sort_by_key(|s| (s.start, s.end));
        let mut out = Vec::with_capacity(self.preserve.len());
        let mut cur = self.preserve[0];
        for s in self.preserve.iter().skip(1) {
            if s.start <= cur.end {
                cur.end = cur.end.max(s.end);
            } else {
                out.push(cur);
                cur = *s;
            }
        }
        out.push(cur);
        self.preserve = out;
    }

    pub fn sort_patches(&mut self) {
        self.patches.sort_by_key(|(o, _)| *o);
    }
}

/// Scan trivia comments in `doc` and build a [`DirectivePlan`].
pub fn scan_directives(doc: &LeekDoc) -> DirectivePlan {
    let mut plan = DirectivePlan::default();
    let mut off_start: Option<u32> = None;

    struct V<'a> {
        plan: &'a mut DirectivePlan,
        off_start: &'a mut Option<u32>,
        source: &'a [u8],
    }

    impl Visitor for V<'_> {
        fn visit_token(&mut self, token: &SyntaxToken) -> sipha::tree::walk::WalkResult {
            let Some(k) = token.kind_as::<K>() else {
                return sipha::tree::walk::WalkResult::Continue(());
            };
            if !matches!(k, K::LineComment | K::BlockComment) {
                return sipha::tree::walk::WalkResult::Continue(());
            }
            let text = token.text();
            let range = token.text_range();
            if let Some(body) = line_comment_body(text) {
                apply_comment_body(body, range, self.plan, self.off_start, self.source);
            } else if let Some(body) = block_comment_body(text) {
                apply_comment_body(body, range, self.plan, self.off_start, self.source);
            }
            sipha::tree::walk::WalkResult::Continue(())
        }
    }

    let mut v = V {
        plan: &mut plan,
        off_start: &mut off_start,
        source: doc.source(),
    };
    let _ = walk(
        doc.root_syntax(),
        &mut v,
        &WalkOptions {
            visit_tokens: true,
            visit_trivia: true,
        },
    );

    if let Some(start) = off_start {
        let end = doc.source().len() as Pos;
        if start < end {
            plan.preserve.push(Span::new(start, end));
        }
    }

    plan.merge_overlapping_preserve();
    plan.sort_patches();
    plan
}

fn line_comment_body(text: &str) -> Option<&str> {
    let t = text.strip_prefix("//")?;
    Some(t.trim_end_matches(['\r', '\n']))
}

fn block_comment_body(text: &str) -> Option<&str> {
    let t = text.strip_prefix("/*")?.strip_suffix("*/")?;
    Some(t)
}

fn apply_comment_body(
    body: &str,
    comment_range: Span,
    plan: &mut DirectivePlan,
    off_start: &mut Option<u32>,
    source: &[u8],
) {
    let comment_start = comment_range.start;
    let comment_end = comment_range.end;
    let trimmed = body.trim();
    let Some(rest) = trimmed
        .strip_prefix("leekfmt:")
        .or_else(|| trimmed.strip_prefix("LEEKFMT:"))
    else {
        return;
    };

    for part in split_directive_parts(rest) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let pl = part.to_ascii_lowercase();
        match pl.as_str() {
            "off" => {
                *off_start = Some(comment_end);
            }
            "on" => {
                if let Some(start) = off_start.take() {
                    let preserve_end = comment_start;
                    if start < preserve_end {
                        plan.preserve.push(clamp_span(
                            Span::new(start, preserve_end),
                            source.len() as Pos,
                        ));
                    }
                }
            }
            "ignore-next-line" | "skip-next-line" | "ignore-next" | "skip-next" => {
                if let Some(span) = span_next_line(source, comment_end) {
                    plan.preserve.push(clamp_span(span, source.len() as Pos));
                }
            }
            _ => {
                if let Some(patch) = parse_option_assignments(part) {
                    plan.patches.push((comment_end, patch));
                }
            }
        }
    }
}

fn split_directive_parts(rest: &str) -> Vec<&str> {
    rest.split([';', ','])
        .filter(|s| !s.trim().is_empty())
        .collect()
}

fn clamp_span(span: Span, source_len: Pos) -> Span {
    let end = span.end.min(source_len);
    let start = span.start.min(end);
    Span::new(start, end)
}

fn span_next_line(source: &[u8], after_comment_end: u32) -> Option<Span> {
    let mut i = after_comment_end as usize;
    if i > source.len() {
        return None;
    }
    while i < source.len() && source[i] != b'\n' {
        i += 1;
    }
    if i < source.len() && source[i] == b'\n' {
        i += 1;
    }
    if i >= source.len() {
        return None;
    }
    let start = i as u32;
    while i < source.len() && source[i] != b'\n' {
        i += 1;
    }
    if i < source.len() && source[i] == b'\n' {
        i += 1;
    }
    Some(clamp_span(
        Span::new(start, i as u32),
        source.len() as Pos,
    ))
}

fn parse_option_assignments(part: &str) -> Option<FormatPatch> {
    let mut patch = FormatPatch::default();
    let mut any = false;

    for segment in part.split_whitespace() {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let (key, value) = segment
            .split_once('=')
            .or_else(|| segment.split_once(':'))
            .unwrap_or((segment, ""));
        let key = key.trim().to_ascii_lowercase().replace('-', "_");
        let value = value.trim();
        if value.is_empty() && key != "use_tabs" && key != "indent_style" {
            continue;
        }
        any |= apply_kv(&mut patch, &key, value);
    }

    any.then_some(patch)
}

fn apply_kv(patch: &mut FormatPatch, key: &str, value: &str) -> bool {
    match key {
        "indent_width" => {
            if let Ok(n) = value.parse::<usize>() {
                patch.indent_width = Some(n.clamp(1, 32));
                return true;
            }
        }
        "tab_width" => {
            if let Ok(n) = value.parse::<usize>() {
                patch.tab_width = Some(n.clamp(1, 32));
                return true;
            }
        }
        "line_width" => {
            if let Ok(n) = value.parse::<usize>() {
                patch.line_width = Some(n.clamp(20, 500));
                return true;
            }
        }
        "indent_style" => {
            let v = value.to_ascii_lowercase();
            if v == "tabs" || v == "tab" {
                patch.use_tabs = Some(true);
                return true;
            }
            if v == "spaces" || v == "space" {
                patch.use_tabs = Some(false);
                return true;
            }
        }
        "use_tabs" => {
            if let Some(b) = parse_bool(value) {
                patch.use_tabs = Some(b);
                return true;
            }
        }
        "brace_style" => {
            let v = value.to_ascii_lowercase();
            if v == "same_line" || v == "sameline" || v == "kr" {
                patch.brace_style = Some(BraceStyle::SameLine);
                return true;
            }
            if v == "next_line" || v == "nextline" || v == "allman" {
                patch.brace_style = Some(BraceStyle::NextLine);
                return true;
            }
        }
        "blank_lines_between_top_level" => {
            if let Ok(n) = value.parse::<usize>() {
                patch.blank_lines_between_top_level = Some(n.min(10));
                return true;
            }
        }
        "max_consecutive_blank_lines_in_block" => {
            if let Ok(n) = value.parse::<usize>() {
                patch.max_consecutive_blank_lines_in_block = Some(n.min(10));
                return true;
            }
        }
        "space_after_keyword_before_paren" => {
            if let Some(b) = parse_bool(value) {
                patch.space_after_keyword_before_paren = Some(b);
                return true;
            }
        }
        "space_before_function_decl_paren" => {
            if let Some(b) = parse_bool(value) {
                patch.space_before_function_decl_paren = Some(b);
                return true;
            }
        }
        "space_inside_parens" => {
            if let Some(b) = parse_bool(value) {
                patch.space_inside_parens = Some(b);
                return true;
            }
        }
        "space_around_assign" => {
            if let Some(b) = parse_bool(value) {
                patch.space_around_assign = Some(b);
                return true;
            }
        }
        "space_around_binary_ops" => {
            if let Some(b) = parse_bool(value) {
                patch.space_around_binary_ops = Some(b);
                return true;
            }
        }
        "newline_before_else_catch_finally" => {
            if let Some(b) = parse_bool(value) {
                patch.newline_before_else_catch_finally = Some(b);
                return true;
            }
        }
        "trailing_newline" => {
            if let Some(b) = parse_bool(value) {
                patch.trailing_newline = Some(b);
                return true;
            }
        }
        "line_ending" => {
            let v = value.to_ascii_lowercase();
            if matches!(v.as_str(), "lf" | "unix" | "\n") {
                patch.line_ending = Some(LineEnding::Lf);
                return true;
            }
            if matches!(v.as_str(), "crlf" | "windows" | "\r\n") {
                patch.line_ending = Some(LineEnding::Crlf);
                return true;
            }
        }
        _ => {}
    }
    false
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// True if `span` lies fully inside some preserve region.
pub fn span_is_preserved(span: Span, preserve: &[Span]) -> bool {
    preserve
        .iter()
        .any(|p| p.start <= span.start && p.end >= span.end)
}

/// True if `span` intersects any preserve region (conservative “do not touch”).
pub fn span_touches_preserve(span: Span, preserve: &[Span]) -> bool {
    preserve
        .iter()
        .any(|p| p.start < span.end && p.end > span.start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Version;

    #[test]
    fn directive_patch_indent() {
        let doc = LeekDoc::parse("// leekfmt: indent-width=2\nlet x=1;", Version::V4).unwrap();
        let plan = scan_directives(&doc);
        assert_eq!(plan.patches.len(), 1);
        assert_eq!(plan.patches[0].1.indent_width, Some(2));
    }

    #[test]
    fn off_on_region() {
        let src = "a;\n// leekfmt: off\nmangled{}\n// leekfmt: on\nb;\n";
        let doc = LeekDoc::parse(src, Version::V4).unwrap();
        let plan = scan_directives(&doc);
        assert_eq!(plan.preserve.len(), 1);
        assert!(!plan.preserve[0].is_empty());
    }
}
