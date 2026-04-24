//! Token kinds produced by the lexer.

use crate::keyword::{Kw, WordOp};
use leekscript_span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Eof,
    /// User-defined identifier (`TokenType.STRING` in Java).
    Ident,
    /// Reserved word (`Kw`).
    Kw(Kw),
    /// `and` / `or` / `xor` / `instanceof` → operator in Java.
    WordOp(WordOp),
    Number,
    /// String literal including quotes (`'` or `"`).
    String,
    /// `∞`
    Lemniscate,
    /// `π`
    Pi,
    /// Punctuation operators (`TokenType.OPERATOR`).
    Operator,
    Semicolon,
    Comma,
    ParOpen,
    ParClose,
    BracketOpen,
    BracketClose,
    BraceOpen,
    BraceClose,
    Dot,
    DotDot,
    Arrow,
}

#[derive(Debug, Clone)]
pub struct LexError {
    /// Java `Error` enum name for registry lookup (`INVALID_CHAR`, …).
    pub reference: &'static str,
    pub span: Span,
}
