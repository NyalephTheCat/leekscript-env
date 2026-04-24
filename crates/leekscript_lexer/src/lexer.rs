//! Lexer matching `LexicalParser` tokenization order (whitespace → string → comment → number → special id → ident → operator → brackets → comma).

use crate::keyword::classify_word;
use crate::token::{LexError, Token, TokenKind};
use leekscript_span::Span;

#[derive(Clone, Copy)]
pub struct LexerConfig {
    /// Language version 1–4+ (default 4).
    pub version: u8,
}

impl Default for LexerConfig {
    fn default() -> Self {
        Self { version: 4 }
    }
}

pub struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    cfg: LexerConfig,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str, cfg: LexerConfig) -> Self {
        Self { src, pos: 0, cfg }
    }

    pub fn tokenize(mut self) -> (Vec<Token>, Vec<LexError>) {
        let mut tokens = Vec::new();
        let mut errors = Vec::new();
        loop {
            if self.pos >= self.src.len() {
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    span: Span::point(self.pos),
                });
                break;
            }
            if self.try_ws() {
                continue;
            }
            match self.try_string(&mut tokens) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => {
                    errors.push(e);
                    self.pos = self.src.len();
                    continue;
                }
            }
            if self.try_comments() {
                continue;
            }
            if self.try_number(&mut tokens, &mut errors) {
                continue;
            }
            if self.try_special_ident(&mut tokens) {
                continue;
            }
            if self.try_ident(&mut tokens) {
                continue;
            }
            if self.try_operator(&mut tokens) {
                continue;
            }
            if self.try_bracket(&mut tokens) {
                continue;
            }
            if self.try_comma_like(&mut tokens) {
                continue;
            }
            let bad = self.peek_char().unwrap_or('\u{0}');
            let start = self.pos;
            self.advance_char();
            errors.push(LexError {
                reference: "INVALID_CHAR",
                span: Span::new(start..self.pos),
            });
            let _ = bad;
        }
        (tokens, errors)
    }

    fn try_ws(&mut self) -> bool {
        match self.peek_char() {
            Some(' ' | '\r' | '\n' | '\t' | '\u{00A0}') => {
                self.advance_char();
                true
            }
            _ => false,
        }
    }

    /// Returns `Ok(true)` if consumed a string, `Ok(false)` if not a string, `Err` on unclosed.
    fn try_string(&mut self, tokens: &mut Vec<Token>) -> Result<bool, LexError> {
        let Some(q) = self.peek_char() else {
            return Ok(false);
        };
        if q != '"' && q != '\'' {
            return Ok(false);
        }
        let start = self.pos;
        self.advance_char();
        let mut escaped = false;
        let mut closed = false;
        while let Some(c) = self.peek_char() {
            if c == '\\' {
                escaped = !escaped;
                self.advance_char();
                continue;
            }
            if c == q && !escaped {
                closed = true;
                self.advance_char();
                break;
            }
            escaped = false;
            self.advance_char();
        }
        if closed {
            tokens.push(Token {
                kind: TokenKind::String,
                span: Span::new(start..self.pos),
            });
            Ok(true)
        } else {
            Err(LexError {
                reference: "STRING_NOT_CLOSED",
                span: Span::new(start..self.pos),
            })
        }
    }

    fn try_comments(&mut self) -> bool {
        if self.src.len() < self.pos + 2 {
            return false;
        }
        let b = self.src.as_bytes();
        if b[self.pos] == b'/' && b[self.pos + 1] == b'/' {
            while self.pos < self.src.len() && self.peek_char() != Some('\n') {
                self.advance_char();
            }
            if self.peek_char() == Some('\n') {
                self.advance_char();
            }
            return true;
        }
        if b[self.pos] == b'/' && b[self.pos + 1] == b'*' {
            self.pos += 2;
            if self.cfg.version < 2 && self.peek_char() == Some('/') {
                self.advance_char();
                return true;
            }
            while self.pos + 1 < self.src.len() {
                let p = self.src.as_bytes()[self.pos];
                let q = self.src.as_bytes()[self.pos + 1];
                if p == b'*' && q == b'/' {
                    self.pos += 2;
                    return true;
                }
                self.advance_char();
            }
            self.pos = self.src.len();
            return true;
        }
        false
    }

    fn try_number(&mut self, tokens: &mut Vec<Token>, errors: &mut Vec<LexError>) -> bool {
        let Some(c) = self.peek_char() else {
            return false;
        };
        if !c.is_ascii_digit() {
            return false;
        }
        let start = self.pos;
        // Java `tryParseNumber`: `0x` / `0X` hex, `0b` / `0B` binary; `_` separators stripped later.
        if c == '0' {
            let nc = self.peek_char_n(1);
            if matches!(nc, Some('x' | 'X')) {
                self.advance_char();
                self.advance_char();
                let body = self.pos;
                let mut any = false;
                while self.pos < self.src.len() {
                    let ch = self.peek_char().unwrap();
                    if ch == '_' {
                        self.advance_char();
                        continue;
                    }
                    if ch.is_ascii_hexdigit() {
                        any = true;
                        self.advance_char();
                        continue;
                    }
                    break;
                }
                // Hex floating-point: `0x1.ap2` (optional fraction, binary exponent `p`/`P`).
                if self.peek_char() == Some('.') && self.peek_char_n(1) != Some('.') {
                    self.advance_char();
                    while self.pos < self.src.len() {
                        let ch = self.peek_char().unwrap();
                        if ch == '_' {
                            self.advance_char();
                            continue;
                        }
                        if ch.is_ascii_hexdigit() {
                            any = true;
                            self.advance_char();
                            continue;
                        }
                        break;
                    }
                }
                if matches!(self.peek_char(), Some('p' | 'P')) {
                    self.advance_char();
                    if matches!(self.peek_char(), Some('+' | '-')) {
                        self.advance_char();
                    }
                    while self.pos < self.src.len()
                        && self.peek_char().unwrap().is_ascii_digit()
                    {
                        self.advance_char();
                    }
                }
                if !any {
                    errors.push(LexError {
                        reference: "INVALID_CHAR",
                        span: Span::new(body..body),
                    });
                } else if self.peek_char().is_some_and(|c| c == 'x' || c == 'X') {
                    errors.push(LexError {
                        reference: "INVALID_NUMBER",
                        span: Span::new(start..self.pos),
                    });
                    while self.peek_char().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                        self.advance_char();
                    }
                }
                tokens.push(Token {
                    kind: TokenKind::Number,
                    span: Span::new(start..self.pos),
                });
                return true;
            }
            if matches!(nc, Some('b' | 'B')) {
                self.advance_char();
                self.advance_char();
                let body = self.pos;
                let mut any = false;
                while self.pos < self.src.len() {
                    let ch = self.peek_char().unwrap();
                    if ch == '_' {
                        self.advance_char();
                        continue;
                    }
                    if ch == '0' || ch == '1' {
                        any = true;
                        self.advance_char();
                        continue;
                    }
                    break;
                }
                if !any {
                    errors.push(LexError {
                        reference: "INVALID_CHAR",
                        span: Span::new(body..body),
                    });
                } else if self.peek_char().is_some_and(|c| {
                    c.is_ascii_digit() && c != '0' && c != '1'
                }) {
                    errors.push(LexError {
                        reference: "INVALID_NUMBER",
                        span: Span::new(start..self.pos),
                    });
                    while self.peek_char().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                        self.advance_char();
                    }
                }
                tokens.push(Token {
                    kind: TokenKind::Number,
                    span: Span::new(start..self.pos),
                });
                return true;
            }
        }
        self.advance_char();
        let mut saw_dot = false;
        while self.pos < self.src.len() {
            let ch = self.peek_char().unwrap();
            if ch == '_' {
                self.advance_char();
                continue;
            }
            if ch.is_ascii_digit() {
                self.advance_char();
                continue;
            }
            if ch == '.' {
                // Stop before range operator `..`.
                if self.peek_char_n(1) == Some('.') {
                    break;
                }
                // Only a single decimal point allowed.
                if saw_dot {
                    errors.push(LexError {
                        reference: "INVALID_CHAR",
                        span: Span::point(self.pos),
                    });
                    break;
                }
                saw_dot = true;
                self.advance_char();
                continue;
            }
            // Exponent / float suffix scanning: accept + / - only right after marker.
            if ch == '-' || ch == '+' {
                let prev = self.src[..self.pos].chars().next_back();
                if prev == Some('e') || prev == Some('E') || prev == Some('p') || prev == Some('P')
                {
                    self.advance_char();
                    continue;
                }
                break;
            }
            // Java accepts `e`/`E`/`p`/`P` in numeric literals; we allow them here as part of the token.
            if matches!(ch, 'e' | 'E' | 'p' | 'P') {
                self.advance_char();
                continue;
            }
            break;
        }
        if let Some(c) = self.peek_char() {
            if c.is_ascii_alphabetic() && !matches!(c, 'e' | 'E') {
                errors.push(LexError {
                    reference: "INVALID_NUMBER",
                    span: Span::new(start..self.pos),
                });
                while self
                    .peek_char()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                {
                    self.advance_char();
                }
            }
        }
        tokens.push(Token {
            kind: TokenKind::Number,
            span: Span::new(start..self.pos),
        });
        true
    }

    fn try_special_ident(&mut self, tokens: &mut Vec<Token>) -> bool {
        if self.peek_char() == Some('∞') {
            let s = self.pos;
            self.advance_char();
            tokens.push(Token {
                kind: TokenKind::Lemniscate,
                span: Span::new(s..self.pos),
            });
            return true;
        }
        if self.peek_char() == Some('π') {
            let s = self.pos;
            self.advance_char();
            tokens.push(Token {
                kind: TokenKind::Pi,
                span: Span::new(s..self.pos),
            });
            return true;
        }
        false
    }

    fn try_ident(&mut self, tokens: &mut Vec<Token>) -> bool {
        let start = self.pos;
        let Some(c) = self.peek_char() else {
            return false;
        };
        if !is_leek_id_start(c) {
            return false;
        }
        self.advance_char();
        while let Some(c) = self.peek_char() {
            if is_leek_id_continue(c) {
                self.advance_char();
            } else {
                break;
            }
        }
        if start == self.pos {
            return false;
        }
        let word = &self.src[start..self.pos];
        let mut kind = classify_word(word, self.cfg.version);

        // Java suite syntax sugar: `is not` is treated as a single binary operator (like `!=`).
        //
        // We lex it as one token spanning `is<ws>not` so the parser can treat it as a normal infix op.
        // Lowering normalizes internal whitespace when mapping operators.
        if matches!(kind, TokenKind::WordOp(crate::keyword::WordOp::Is)) {
            let mut i = self.pos;
            while i < self.src.len() && self.src.as_bytes()[i].is_ascii_whitespace() {
                i += 1;
            }
            if i + 3 <= self.src.len() {
                let cand = &self.src[i..i + 3];
                let not_match = if self.cfg.version <= 2 {
                    cand.eq_ignore_ascii_case("not")
                } else {
                    cand == "not"
                };
                if not_match {
                    let after = i + 3;
                    let boundary_ok = after >= self.src.len()
                        || !self
                            .src
                            .as_bytes()
                            .get(after)
                            .is_some_and(|b| (*b as char).is_alphanumeric() || *b == b'_');
                    if boundary_ok {
                        self.pos = after;
                        // Keep `WordOp::Is`; the token text is normalized in lowering.
                        kind = TokenKind::WordOp(crate::keyword::WordOp::Is);
                    }
                }
            }
        }
        tokens.push(Token {
            kind,
            span: Span::new(start..self.pos),
        });
        true
    }

    fn try_operator(&mut self, tokens: &mut Vec<Token>) -> bool {
        if self.match_exact("=>") || self.match_exact("->") {
            let s = self.pos - 2;
            tokens.push(Token {
                kind: TokenKind::Arrow,
                span: Span::new(s..self.pos),
            });
            return true;
        }
        if self.match_exact("..") {
            let s = self.pos - 2;
            tokens.push(Token {
                kind: TokenKind::DotDot,
                span: Span::new(s..self.pos),
            });
            return true;
        }
        // `<<` is ambiguous: nested set literals (`<<>>`) need two `<` tokens; `1 << 2` is shift.
        if self.peek_char() == Some('<')
            && self.peek_char_n(1) == Some('<')
            && self.peek_char_n(2) != Some('=')
            && !Self::last_token_can_end_shift_lhs(tokens, self.src)
        {
            let s = self.pos;
            self.advance_char();
            tokens.push(Token {
                kind: TokenKind::Operator,
                span: Span::new(s..self.pos),
            });
            let s2 = self.pos;
            self.advance_char();
            tokens.push(Token {
                kind: TokenKind::Operator,
                span: Span::new(s2..self.pos),
            });
            return true;
        }
        if self.cfg.version >= 2 && self.peek_char() == Some('.') {
            let s = self.pos;
            self.advance_char();
            tokens.push(Token {
                kind: TokenKind::Dot,
                span: Span::new(s..self.pos),
            });
            return true;
        }
        for op in OPERATORS {
            if self.match_exact(op) {
                let len = op.len();
                let s = self.pos - len;
                tokens.push(Token {
                    kind: TokenKind::Operator,
                    span: Span::new(s..self.pos),
                });
                return true;
            }
        }
        false
    }

    /// After these tokens, `<<` is parsed as the left-shift operator; otherwise split into two `<` (set literals).
    fn last_token_can_end_shift_lhs(tokens: &[Token], src: &str) -> bool {
        let Some(t) = tokens.last() else {
            return false;
        };
        use crate::keyword::Kw;
        match t.kind {
            TokenKind::Number
            | TokenKind::Ident
            | TokenKind::String
            | TokenKind::Lemniscate
            | TokenKind::Pi => true,
            TokenKind::ParClose | TokenKind::BracketClose => true,
            TokenKind::Kw(k) => matches!(
                k,
                Kw::True | Kw::False | Kw::Null | Kw::This | Kw::Super
            ),
            TokenKind::Operator => {
                let tx = &src[t.span.start as usize..t.span.end as usize];
                matches!(tx, "++" | "--")
            }
            _ => false,
        }
    }

    fn try_bracket(&mut self, tokens: &mut Vec<Token>) -> bool {
        let map = [
            ('[', TokenKind::BracketOpen),
            (']', TokenKind::BracketClose),
            ('(', TokenKind::ParOpen),
            (')', TokenKind::ParClose),
            ('{', TokenKind::BraceOpen),
            ('}', TokenKind::BraceClose),
        ];
        let Some(c) = self.peek_char() else {
            return false;
        };
        for (ch, kind) in map {
            if c == ch {
                let s = self.pos;
                self.advance_char();
                tokens.push(Token {
                    kind,
                    span: Span::new(s..self.pos),
                });
                return true;
            }
        }
        false
    }

    fn try_comma_like(&mut self, tokens: &mut Vec<Token>) -> bool {
        match self.peek_char() {
            Some(',') => {
                let s = self.pos;
                self.advance_char();
                tokens.push(Token {
                    kind: TokenKind::Comma,
                    span: Span::new(s..self.pos),
                });
                true
            }
            Some(';') => {
                let s = self.pos;
                self.advance_char();
                tokens.push(Token {
                    kind: TokenKind::Semicolon,
                    span: Span::new(s..self.pos),
                });
                true
            }
            _ => false,
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_char_n(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    fn advance_char(&mut self) {
        if let Some(c) = self.peek_char() {
            self.pos += c.len_utf8();
        }
    }

    fn match_exact(&mut self, pat: &str) -> bool {
        if pat.is_empty() {
            return true;
        }
        let mut idx = self.pos;
        for p in pat.chars() {
            let Some(rest) = self.src.get(idx..) else {
                return false;
            };
            let Some(c) = rest.chars().next() else {
                return false;
            };
            let ok = if self.cfg.version <= 2 {
                c.eq_ignore_ascii_case(&p)
            } else {
                c == p
            };
            if !ok {
                return false;
            }
            idx += c.len_utf8();
        }
        self.pos = idx;
        true
    }
}

/// Java `LexicalParser.tryParseOperator` order (longest match via iteration order).
const OPERATORS: &[&str] = &[
    ":", "&&", "&=", "&", "||", "|=", "|", "++", "+=", "+", "--", "-=", "-", "**=", "**", "*=",
    "*", "/=", "/", "\\=", "\\", "%=", "%",     "===", "==", "=", "!==", "!=", "!", "<<<=", "<<<",
    "<<=", "<<", "<=", "<", ">>>=", ">>>", ">>=", ">>", ">=", ">", "^=", "^", "~", "@",
    "??=", "??", "?",
];

fn is_leek_id_start(c: char) -> bool {
    is_leek_id_continue(c) && !c.is_ascii_digit()
}

fn is_leek_id_continue(c: char) -> bool {
    matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '_' | 'ÿ')
        || ('\u{00C0}'..='\u{00D6}').contains(&c)
        || ('\u{00E0}'..='\u{00F6}').contains(&c)
        || ('\u{00D8}'..='\u{00DD}').contains(&c)
        || ('\u{00F8}'..='\u{00FD}').contains(&c)
        || ('\u{0152}'..='\u{0153}').contains(&c)
}
