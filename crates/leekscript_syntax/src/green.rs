//! Shared helpers for building rowan green trees from lexer output + source text.

use crate::kind::LeekSyntaxKind;
use crate::trivia::split_trivia;
use leekscript_lexer::{Token, TokenKind};
use rowan::GreenNodeBuilder;

/// Map a lexer token to a concrete [`LeekSyntaxKind`] leaf.
#[must_use]
pub fn syntax_kind_for_token(kind: &TokenKind) -> LeekSyntaxKind {
    match kind {
        TokenKind::Ident => LeekSyntaxKind::Ident,
        TokenKind::Kw(_) => LeekSyntaxKind::Kw,
        TokenKind::WordOp(_) => LeekSyntaxKind::WordOp,
        TokenKind::Number => LeekSyntaxKind::Number,
        TokenKind::String => LeekSyntaxKind::String,
        TokenKind::Lemniscate => LeekSyntaxKind::Lemniscate,
        TokenKind::Pi => LeekSyntaxKind::Pi,
        TokenKind::Operator => LeekSyntaxKind::Operator,
        TokenKind::Semicolon => LeekSyntaxKind::Semicolon,
        TokenKind::Comma => LeekSyntaxKind::Comma,
        TokenKind::ParOpen => LeekSyntaxKind::ParenOpen,
        TokenKind::ParClose => LeekSyntaxKind::ParenClose,
        TokenKind::BracketOpen => LeekSyntaxKind::BracketOpen,
        TokenKind::BracketClose => LeekSyntaxKind::BracketClose,
        TokenKind::BraceOpen => LeekSyntaxKind::BraceOpen,
        TokenKind::BraceClose => LeekSyntaxKind::BraceClose,
        TokenKind::Dot => LeekSyntaxKind::Dot,
        TokenKind::DotDot => LeekSyntaxKind::DotDot,
        TokenKind::Arrow => LeekSyntaxKind::Arrow,
        TokenKind::Eof => LeekSyntaxKind::Tombstone,
    }
}

pub fn push_trivia(builder: &mut GreenNodeBuilder<'_>, gap: &str) {
    for (k, slice) in split_trivia(gap) {
        builder.token(rowan::SyntaxKind(k as u16), slice);
    }
}

pub fn push_lex_token(builder: &mut GreenNodeBuilder<'_>, src: &str, t: &Token) {
    let k = syntax_kind_for_token(&t.kind);
    debug_assert!(!matches!(t.kind, TokenKind::Eof));
    let slice = &src[t.span.start as usize..t.span.end as usize];
    builder.token(rowan::SyntaxKind(k as u16), slice);
}

/// Emit trivia `src[last_end..token.start]` then the lexical token; advance `last_end` to token end.
pub fn emit_token_with_trivia(
    builder: &mut GreenNodeBuilder<'_>,
    src: &str,
    last_end: &mut usize,
    t: &Token,
) {
    let s = t.span.start as usize;
    let e = t.span.end as usize;
    push_trivia(builder, &src[*last_end..s]);
    push_lex_token(builder, src, t);
    *last_end = e;
}
