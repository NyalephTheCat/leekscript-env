pub mod doxygen;
pub mod kinds;

pub use doxygen::{
    parse_doxygen, DoxygenParam, DoxygenRetval, DoxygenThrows, ParsedDoxygen,
};

use crate::syntax::kinds::K;
use sipha::tree::red::{SyntaxElement, SyntaxNode, SyntaxToken};

/// True for whitespace / comment tokens and for the lexer’s grouped [`K::Trivia`] node (it only
/// contains trivia token leaves).
#[inline]
pub(crate) fn syntax_el_is_trivia(el: &SyntaxElement) -> bool {
    el.is_trivia() || el.kind_as::<K>() == Some(K::Trivia)
}

/// Collect raw Doxygen comment text attached to `node` (see [`attached_docstring`]).
fn collect_raw_doxygen_body(node: &SyntaxNode) -> Option<String> {
    let first = node.first_token()?;
    let cutoff = first.offset();
    let node_start = node.offset();
    let mut toks: Vec<SyntaxToken> = node
        .descendant_tokens()
        .into_iter()
        .filter(|t| matches!(t.kind_as::<K>(), Some(K::LineComment | K::BlockComment)))
        .filter(|t| {
            let r = t.text_range();
            r.start >= node_start && r.end <= cutoff
        })
        .collect();
    toks.sort_by_key(SyntaxToken::offset);
    let mut pieces: Vec<String> = Vec::new();
    for tok in toks {
        if let Some(p) = doxygen_from_comment_token(&tok) {
            if !p.is_empty() {
                pieces.push(p);
            }
        }
    }
    if pieces.is_empty() {
        return None;
    }
    let joined = pieces.join("\n");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Leading Doxygen-style documentation before a declaration: `/** … */`, `/*! … */`, and/or
/// consecutive `/// …` line comments (plain `//` and `/* … */` are ignored).
///
/// For structured commands (`\brief`, `@param`, …), use [`attached_parsed_doxygen`] or
/// [`parse_doxygen`] on the returned string.
#[must_use]
pub fn attached_docstring(node: &SyntaxNode) -> Option<String> {
    collect_raw_doxygen_body(node)
}

/// Like [`attached_docstring`], but parses Doxygen commands into [`ParsedDoxygen`].
#[must_use]
pub fn attached_parsed_doxygen(node: &SyntaxNode) -> Option<ParsedDoxygen> {
    let raw = collect_raw_doxygen_body(node)?;
    Some(doxygen::parse_doxygen(&raw))
}

fn doxygen_from_comment_token(tok: &SyntaxToken) -> Option<String> {
    match tok.kind_as::<K>()? {
        K::LineComment => doxygen_from_line_comment(tok.text()),
        K::BlockComment => doxygen_from_block_comment(tok.text()),
        _ => None,
    }
}

/// `///` doc lines; `////` and `//!` are not treated as declaration docs.
fn doxygen_from_line_comment(text: &str) -> Option<String> {
    let t = text.strip_prefix("//")?;
    if t.starts_with("//!") {
        return None;
    }
    if t.starts_with("////") {
        return None;
    }
    let body = t.strip_prefix("//")?;
    Some(body.trim_start_matches(' ').to_string())
}

/// `/** … */` or `/*! … */` only (not plain `/* … */`).
fn doxygen_from_block_comment(text: &str) -> Option<String> {
    let inner = text.strip_prefix("/*")?.strip_suffix("*/")?;
    let inner = inner.trim();
    let rest = if let Some(s) = inner.strip_prefix("**") {
        s.trim_start()
    } else if let Some(s) = inner.strip_prefix('*') {
        s.trim_start()
    } else if let Some(s) = inner.strip_prefix('!') {
        s.trim_start()
    } else {
        return None;
    };
    let normalized = normalize_doxygen_block_body(rest);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

/// Strip optional leading `*` on each line (Javadoc-style `/**` … `*/` bodies).
fn normalize_doxygen_block_body(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let out: Vec<String> = lines
        .iter()
        .map(|line| {
            let t = line.trim();
            t.strip_prefix('*').map_or(t, str::trim).to_string()
        })
        .collect();
    out.join("\n").trim().to_string()
}
