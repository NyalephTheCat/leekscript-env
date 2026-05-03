//! Build a rowan green tree from lexer output + source text (lossless: full `src` round-trips).

use crate::green::{push_lex_token, push_trivia};
use crate::kind::LeekSyntaxKind;
use crate::language::LeekLanguage;
use leekscript_lexer::{Token, TokenKind};
use rowan::{GreenNodeBuilder, SyntaxNode};

/// Lexical tokens only (no `Eof`), in order.
#[must_use]
pub fn non_eof_tokens(tokens: &[Token]) -> Vec<&Token> {
    tokens.iter().filter(|t| t.kind != TokenKind::Eof).collect()
}

/// Lossless green tree: `SOURCE_FILE` → trivia + lexical tokens, covering all of `src`.
#[must_use]
pub fn build_source_file_tree(src: &str, tokens: &[Token]) -> SyntaxNode<LeekLanguage> {
    let lex = non_eof_tokens(tokens);
    let mut builder = GreenNodeBuilder::new();
    builder.start_node(rowan::SyntaxKind(LeekSyntaxKind::SourceFile as u16));

    if lex.is_empty() {
        push_trivia(&mut builder, src);
        builder.finish_node();
        let green = builder.finish();
        return SyntaxNode::new_root(green);
    }

    let first = lex[0];
    push_trivia(&mut builder, &src[0..first.span.start as usize]);

    for i in 0..lex.len() {
        if i > 0 {
            let gap = &src[lex[i - 1].span.end as usize..lex[i].span.start as usize];
            push_trivia(&mut builder, gap);
        }
        push_lex_token(&mut builder, src, lex[i]);
    }

    let last_end = lex.last().unwrap().span.end as usize;
    push_trivia(&mut builder, &src[last_end..src.len()]);

    builder.finish_node();
    let green = builder.finish();
    SyntaxNode::new_root(green)
}

/// Extract prefix, between-token gaps, and suffix from a flat `SOURCE_FILE` (same layout as [`build_source_file_tree`]).
#[must_use]
pub fn gaps_from_source_file(
    root: &SyntaxNode<LeekLanguage>,
) -> Option<(String, Vec<String>, String)> {
    if root.kind() != LeekSyntaxKind::SourceFile {
        return None;
    }
    let mut prefix = String::new();
    let mut between: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut seen_token = 0u32;

    for el in root.children_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t) => {
                let k = t.kind();
                if k.is_trivia() {
                    current.push_str(t.text());
                } else {
                    if seen_token == 0 {
                        prefix = current;
                    } else {
                        between.push(current);
                    }
                    current = String::new();
                    seen_token += 1;
                }
            }
            rowan::NodeOrToken::Node(_) => return None,
        }
    }
    Some((prefix, between, current))
}

#[cfg(test)]
mod tests {
    use super::*;
    use leekscript_lexer::{Lexer, LexerConfig};

    #[test]
    fn full_roundtrip_text() {
        let src = "// hi\nvar  x=1;\n";
        let (tokens, errs) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(errs.is_empty());
        let root = build_source_file_tree(src, &tokens);
        assert_eq!(root.text().to_string(), src);
    }

    #[test]
    fn gaps_roundtrip() {
        let src = "var a=1;";
        let (tokens, errs) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(errs.is_empty());
        let root = build_source_file_tree(src, &tokens);
        let (pre, bet, suf) = gaps_from_source_file(&root).unwrap();
        assert_eq!(pre, "");
        assert_eq!(bet.len(), 4);
        assert_eq!(suf, "");
    }
}
