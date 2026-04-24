//! Lexical analysis for LeekScript (token order aligned with Java `LexicalParser`).

mod keyword;
mod lexer;
mod token;

pub use keyword::{Kw, WordOp};
pub use lexer::{Lexer, LexerConfig};
pub use token::{LexError, Token, TokenKind};

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> (Vec<Token>, Vec<LexError>) {
        Lexer::new(src, LexerConfig::default()).tokenize()
    }

    fn src_slice<'a>(src: &'a str, span: leekscript_span::Span) -> &'a str {
        &src[span.start as usize..span.end as usize]
    }

    #[test]
    fn var_semicolon() {
        let (t, e) = lex("var x;");
        assert!(e.is_empty());
        assert!(t.iter().any(|x| matches!(x.kind, TokenKind::Kw(Kw::Var))));
        assert!(t
            .iter()
            .any(|x| { x.kind == TokenKind::Ident && src_slice("var x;", x.span) == "x" }));
        assert!(t.iter().any(|x| x.kind == TokenKind::Semicolon));
        assert!(t.last().map(|x| x.kind == TokenKind::Eof).unwrap_or(false));
    }

    #[test]
    fn and_maps_to_word_op() {
        let (t, e) = lex("a and b");
        assert!(e.is_empty());
        assert!(t.iter().any(|x| {
            matches!(x.kind, TokenKind::WordOp(WordOp::And))
                && src_slice("a and b", x.span) == "and"
        }));
    }

    #[test]
    fn unclosed_string() {
        let (t, e) = lex("\"hi");
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].reference, "STRING_NOT_CLOSED");
        assert!(t.iter().any(|x| x.kind == TokenKind::Eof));
    }

    #[test]
    fn hex_binary_numeric_literals() {
        let src = "0xFF 0b1010 0x1_a0";
        let (t, e) = lex(src);
        assert!(e.is_empty());
        let nums: Vec<_> = t
            .iter()
            .filter(|x| x.kind == TokenKind::Number)
            .map(|x| src_slice(src, x.span))
            .collect();
        assert!(nums.contains(&"0xFF"));
        assert!(nums.contains(&"0b1010"));
        assert!(nums.contains(&"0x1_a0"));
    }

    #[test]
    fn operators_longest_match() {
        let (t, e) = lex("a===b");
        assert!(e.is_empty());
        let ops: Vec<_> = t
            .iter()
            .filter(|x| x.kind == TokenKind::Operator)
            .map(|x| &"a===b"[x.span.start as usize..x.span.end as usize])
            .collect();
        assert!(ops.contains(&"==="));
    }
}
