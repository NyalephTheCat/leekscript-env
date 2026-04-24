//! Span trivia, diagnostics, and source slicing for lowering.

use super::HirLoweringDiagnostic;
use leekscript_span::Span;
use leekscript_syntax::LeekLanguage;
use rowan::{NodeOrToken, SyntaxElement, SyntaxNode, SyntaxToken, TextRange};

/// Source text and language version for HIR lowering (mirrors lexer `LexerConfig.version`).
#[derive(Clone, Copy)]
pub(super) struct LowerCtx<'a> {
    pub src: &'a str,
    pub language_version: u8,
}

pub(super) fn span_of_range(r: TextRange) -> Span {
    let s: usize = r.start().into();
    let e: usize = r.end().into();
    Span::new(s..e)
}

pub(super) fn span_of_node(n: &SyntaxNode<LeekLanguage>) -> Span {
    span_of_range(n.text_range())
}

/// Byte offset of the first non-trivia token in `node`'s subtree (pre-order).
///
/// Rowan `CallExpr` / callee nodes often include **leading** trivia in `SyntaxNode::text_range()`
/// (for example blank lines before `debug` in `\\n\\ndebug(...)`). Farmer log positions and
/// diagnostics should anchor to the real callee token instead.
pub(super) fn span_start_of_first_non_trivia_token(n: &SyntaxNode<LeekLanguage>) -> u32 {
    for el in n.descendants_with_tokens() {
        if let NodeOrToken::Token(t) = el {
            if !t.kind().is_trivia() {
                return span_of_range(t.text_range()).start;
            }
        }
    }
    span_of_node(n).start
}

pub(super) fn non_trivia(
    node: &SyntaxNode<LeekLanguage>,
) -> impl Iterator<Item = SyntaxElement<LeekLanguage>> + '_ {
    node.children_with_tokens().filter(|el| match el {
        NodeOrToken::Token(t) => !t.kind().is_trivia(),
        NodeOrToken::Node(_) => true,
    })
}

pub(super) fn diag(
    reference: &'static str,
    span: Span,
    msg: impl Into<String>,
) -> HirLoweringDiagnostic {
    HirLoweringDiagnostic {
        reference,
        span,
        message: msg.into(),
    }
}
pub(super) fn token_text<'a>(t: &SyntaxToken<LeekLanguage>, src: &'a str) -> &'a str {
    let r = t.text_range();
    let s: usize = r.start().into();
    let e: usize = r.end().into();
    &src[s..e]
}

pub(super) fn unquote_string(raw: &str) -> Result<String, String> {
    let mut chars = raw.chars();
    let Some(q) = chars.next() else {
        return Err("empty string literal".into());
    };
    if q != '\'' && q != '"' {
        return Err("string literal must start with a quote".into());
    }
    let mut out = String::new();
    for c in chars {
        if c == q {
            break;
        }
        out.push(c);
    }
    Ok(out)
}
