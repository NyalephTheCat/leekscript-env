//! LeekScript **concrete syntax tree** using [rowan](https://github.com/rust-analyzer/rowan)'s
//! red–green trees: immutable, cheap to clone, ready for incremental reparsing and LSP.
//!
//! Today the tree is a **flat** `SOURCE_FILE` whose children alternate **trivia** (whitespace,
//! `//`, `/* */`) and **lexical tokens**. As the real parser lands, nested nodes (`EXPR`, `STMT`,
//! …) will wrap the same leaf tokens without changing trivia handling.
//!
//! ## Typed AST
//!
//! Use [`ast::SourceFile`] as the root typed wrapper; deeper `Expr` / `Stmt` types will be added
//! alongside grammar production nodes.

pub mod ast;
mod cst;
pub mod green;
mod kind;
mod language;
mod trivia;

pub use ast::{AstNode, FileSegments, SourceFile, TriviaPiece};
pub use cst::{build_source_file_tree, gaps_from_source_file, non_eof_tokens};
pub use green::{emit_token_with_trivia, push_lex_token, push_trivia, syntax_kind_for_token};
pub use kind::LeekSyntaxKind;
pub use language::LeekLanguage;

use leekscript_lexer::{Lexer, LexerConfig, Token};
use rowan::SyntaxNode;

/// Cursor aliases (rowan) for LeekScript.
pub type SyntaxNodePtr = SyntaxNode<LeekLanguage>;
pub type SyntaxTokenPtr = rowan::SyntaxToken<LeekLanguage>;
pub type SyntaxElementPtr = rowan::SyntaxElement<LeekLanguage>;

/// Lex, then build a lossless rowan tree (no delimiter validation — call [`leekscript_parser::validate_delimiters`] in the driver).
pub fn parse_source_file_tree(src: &str, lexer_cfg: LexerConfig) -> (SyntaxNodePtr, Vec<Token>) {
    let (tokens, _errs) = Lexer::new(src, lexer_cfg).tokenize();
    let root = build_source_file_tree(src, &tokens);
    (root, tokens)
}

pub use rowan::TextRange;
