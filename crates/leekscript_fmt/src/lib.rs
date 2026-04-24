//! Opinionated formatter for LeekScript sources.
//!
//! Strategy: re-lex, validate delimiters, build a lossless [rowan](https://github.com/rust-analyzer/rowan)
//! [`SOURCE_FILE`](leekscript_syntax::LeekSyntaxKind::SourceFile) tree ([`leekscript_syntax::build_source_file_tree`]),
//! then walk the typed [`SourceFile::file_segments`](leekscript_syntax::SourceFile::file_segments) layout:
//! [`TriviaPiece`](leekscript_syntax::TriviaPiece) (whitespace vs line/block comment) and lexical tokens.
//! Comments are preserved per piece (`\r\n` normalized inside comment text); whitespace-only gaps are
//! collapsed using the same spacing rules as before.

use leekscript_directives::FmtPreamble;
use leekscript_lexer::LexError;
use leekscript_lexer::{Kw, Lexer, LexerConfig, Token, TokenKind};
use leekscript_parser::{validate_delimiters, ParseDiagnostic};
use leekscript_syntax::{build_source_file_tree, FileSegments, SourceFile, TriviaPiece};
use std::path::PathBuf;

/// Formatter options (see `Leek.toml` `[fmt]` and `// leek-fmt:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FmtConfig {
    pub width: u32,
    pub indent: u32,
    pub tab_width: u32,
    pub use_tabs: bool,
}

impl Default for FmtConfig {
    fn default() -> Self {
        Self {
            width: 100,
            indent: 4,
            tab_width: 4,
            use_tabs: false,
        }
    }
}

impl FmtConfig {
    /// Merge `[fmt]` table from a parsed manifest (unknown keys ignored).
    pub fn merge_manifest(self, fmt: Option<&toml::Table>) -> Self {
        let Some(t) = fmt else {
            return self;
        };
        let mut c = self;
        if let Some(w) = t.get("width").and_then(|v| v.as_integer()) {
            c.width = w as u32;
        }
        if let Some(i) = t.get("indent").and_then(|v| v.as_integer()) {
            c.indent = i as u32;
        }
        if let Some(tw) = t.get("tab_width").and_then(|v| v.as_integer()) {
            c.tab_width = tw as u32;
        }
        if let Some(b) = t.get("use_tabs").and_then(|v| v.as_bool()) {
            c.use_tabs = b;
        }
        c
    }

    /// File preamble `// leek-fmt:` overrides (narrower fields only).
    pub fn merge_preamble(self, preamble: Option<&FmtPreamble>) -> Self {
        let Some(p) = preamble else {
            return self;
        };
        let mut c = self;
        if let Some(w) = p.width {
            c.width = w;
        }
        if let Some(i) = p.indent {
            c.indent = i;
        }
        if let Some(tw) = p.tab_width {
            c.tab_width = tw;
        }
        if let Some(b) = p.use_tabs {
            c.use_tabs = b;
        }
        c
    }
}

/// Load `[fmt]` from `Leek.toml` discovered like other tools (optional path, else cwd walk).
pub fn fmt_config_from_workspace(manifest: Option<&PathBuf>) -> FmtConfig {
    let defaults = FmtConfig::default();
    let path = manifest.cloned().or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(leekscript_config::find_manifest)
    });
    let Some(p) = path else {
        return defaults;
    };
    let Ok(m) = leekscript_config::LeekManifest::load_path(&p) else {
        return defaults;
    };
    defaults.merge_manifest(m.fmt.as_ref())
}

#[derive(Debug, Clone)]
pub enum FormatError {
    Lex(Vec<LexError>),
    Parse(Vec<ParseDiagnostic>),
}

/// Format `src` using lexer `version` (should match check / manifest resolution).
pub fn format_source(
    src: &str,
    cfg: &FmtConfig,
    lexer_cfg: LexerConfig,
) -> Result<String, FormatError> {
    let (tokens, lex_errors) = Lexer::new(src, lexer_cfg).tokenize();
    if !lex_errors.is_empty() {
        return Err(FormatError::Lex(lex_errors));
    }
    let parse_errors = validate_delimiters(&tokens);
    if !parse_errors.is_empty() {
        return Err(FormatError::Parse(parse_errors));
    }
    if tokens.is_empty() {
        return Ok(String::new());
    }
    let n = tokens.len();
    if tokens[n - 1].kind != TokenKind::Eof {
        return Ok(src.to_string());
    }
    let last_real = n - 2;

    let root = build_source_file_tree(src, &tokens);
    debug_assert_eq!(root.text().to_string(), src);
    let source_file = SourceFile::cast(root).expect("SOURCE_FILE root");
    let seg = source_file
        .file_segments()
        .expect("flat SOURCE_FILE (no nested grammar nodes yet)");
    debug_assert_eq!(
        seg.lexicals.len(),
        last_real + 1,
        "rowan lexicals align with lexer tokens"
    );
    for i in 0..=last_real {
        debug_assert_eq!(seg.lexicals[i].text(), token_slice(src, &tokens[i]));
    }
    debug_assert_eq!(seg.between.len(), last_real);

    let mut out = String::with_capacity(src.len() + src.len() / 8);
    out.push_str(&FileSegments::join_pieces_normalize_comments(&seg.prefix));

    let mut depth: usize = 0;

    for i in 0..=last_real {
        if i > 0 {
            let prev_prev = if i >= 2 { Some(&tokens[i - 2]) } else { None };
            let g = format_trivia_gap(
                &seg.between[i - 1],
                prev_prev,
                &tokens[i - 1],
                &tokens[i],
                depth,
                cfg,
                src,
            );
            out.push_str(&g);
        }
        let s = token_slice(src, &tokens[i]);
        out.push_str(s);
        match tokens[i].kind {
            TokenKind::BraceOpen => depth += 1,
            TokenKind::BraceClose => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            _ => {}
        }
    }

    out.push_str(&normalize_trailing_trivia(&seg.suffix));

    Ok(out)
}

fn token_slice<'a>(src: &'a str, t: &Token) -> &'a str {
    &src[t.span.start as usize..t.span.end as usize]
}

fn pieces_whitespace_only(pieces: &[TriviaPiece]) -> bool {
    pieces.iter().all(|p| {
        matches!(p, TriviaPiece::Whitespace(_)) && p.text().chars().all(|c| c.is_whitespace())
    })
}

fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn indent_line(levels: usize, cfg: &FmtConfig) -> String {
    if levels == 0 {
        return String::new();
    }
    if cfg.use_tabs {
        "\t".repeat(levels)
    } else {
        let n = (cfg.indent as usize).saturating_mul(levels);
        " ".repeat(n)
    }
}

fn format_trivia_gap(
    pieces: &[TriviaPiece],
    prev_prev: Option<&Token>,
    prev: &Token,
    next: &Token,
    depth_after_prev: usize,
    cfg: &FmtConfig,
    src: &str,
) -> String {
    if pieces.iter().any(|p| p.is_comment()) {
        return FileSegments::join_pieces_normalize_comments(pieces);
    }
    if !pieces.is_empty() && !pieces_whitespace_only(pieces) {
        return FileSegments::join_pieces(pieces);
    }
    format_gap_whitespace_only(prev_prev, prev, next, depth_after_prev, cfg, src)
}

/// Whitespace-only gap (or empty): apply brace / spacing rules.
fn format_gap_whitespace_only(
    prev_prev: Option<&Token>,
    prev: &Token,
    next: &Token,
    depth_after_prev: usize,
    cfg: &FmtConfig,
    src: &str,
) -> String {
    // `{}` empty block — keep compact.
    if prev.kind == TokenKind::BraceOpen && next.kind == TokenKind::BraceClose {
        return String::new();
    }

    if prev.kind == TokenKind::BraceOpen && next.kind != TokenKind::BraceClose {
        return format!("\n{}", indent_line(depth_after_prev, cfg));
    }

    if next.kind == TokenKind::BraceClose && prev.kind != TokenKind::BraceOpen {
        let col = depth_after_prev.saturating_sub(1);
        return format!("\n{}", indent_line(col, cfg));
    }

    if needs_space(prev_prev, prev, next, src) {
        " ".to_string()
    } else {
        String::new()
    }
}

fn normalize_trailing_trivia(pieces: &[TriviaPiece]) -> String {
    let joined = FileSegments::join_pieces_normalize_comments(pieces);
    if pieces.iter().any(|p| p.is_comment()) {
        return normalize_newlines(&joined);
    }
    if joined.is_empty() {
        return String::new();
    }
    if joined.chars().all(|c| c.is_whitespace()) {
        return if joined.contains('\n') {
            "\n".into()
        } else {
            String::new()
        };
    }
    joined
}

fn minus_plus_is_binary_after(left: &Token) -> bool {
    match &left.kind {
        TokenKind::Ident
        | TokenKind::Number
        | TokenKind::String
        | TokenKind::Lemniscate
        | TokenKind::Pi => true,
        TokenKind::WordOp(_) => true,
        TokenKind::ParClose | TokenKind::BracketClose | TokenKind::BraceClose => true,
        TokenKind::Kw(k) => matches!(
            k,
            Kw::True | Kw::False | Kw::Null | Kw::This | Kw::Class | Kw::Super
        ),
        _ => false,
    }
}

fn needs_space(prev_prev: Option<&Token>, prev: &Token, next: &Token, src: &str) -> bool {
    let (a, b) = (prev.kind, next.kind);

    if matches!(a, TokenKind::ParOpen)
        || matches!(
            b,
            TokenKind::ParClose
                | TokenKind::BracketClose
                | TokenKind::BraceClose
                | TokenKind::Semicolon
                | TokenKind::Comma
        )
    {
        return false;
    }
    if matches!((a, b), (TokenKind::Comma, TokenKind::ParClose)) {
        return false;
    }
    if matches!(a, TokenKind::Dot | TokenKind::DotDot)
        || matches!(b, TokenKind::Dot | TokenKind::DotDot)
    {
        return false;
    }
    if matches!(a, TokenKind::BraceOpen) || matches!(b, TokenKind::BraceClose) {
        return false;
    }

    if matches!(
        (a, b),
        (
            TokenKind::Kw(Kw::If | Kw::For | Kw::While | Kw::Switch | Kw::Catch),
            TokenKind::ParOpen,
        )
    ) || matches!((a, b), (TokenKind::Kw(Kw::Return), TokenKind::ParOpen))
        || matches!((a, b), (TokenKind::BraceClose, TokenKind::Kw(Kw::Else)))
    {
        return true;
    }
    if matches!((a, b), (TokenKind::Ident, TokenKind::ParOpen)) {
        return false;
    }

    let prev_txt = token_slice(src, prev);
    let next_txt = token_slice(src, next);

    if a == TokenKind::Operator && (prev_txt == "++" || prev_txt == "--") {
        return false;
    }
    if b == TokenKind::Operator && (next_txt == "++" || next_txt == "--") {
        return false;
    }

    if a == TokenKind::Operator && (prev_txt == "-" || prev_txt == "+") && b == TokenKind::Number {
        if let Some(pp) = prev_prev {
            return minus_plus_is_binary_after(pp);
        }
        return false;
    }

    fn wordish(k: TokenKind) -> bool {
        matches!(
            k,
            TokenKind::Ident
                | TokenKind::Number
                | TokenKind::String
                | TokenKind::Lemniscate
                | TokenKind::Pi
                | TokenKind::WordOp(_)
        ) || matches!(k, TokenKind::Kw(_))
    }

    if wordish(a) && wordish(b) {
        return true;
    }

    if a == TokenKind::Comma || a == TokenKind::Semicolon {
        return wordish(b)
            || matches!(
                b,
                TokenKind::ParOpen | TokenKind::BracketOpen | TokenKind::BraceOpen
            );
    }

    if wordish(a) && b == TokenKind::Operator {
        return true;
    }
    if a == TokenKind::Operator && wordish(b) {
        return true;
    }

    if wordish(a)
        && matches!(
            b,
            TokenKind::ParOpen | TokenKind::BracketOpen | TokenKind::BraceOpen
        )
    {
        return true;
    }

    if matches!(
        a,
        TokenKind::ParClose | TokenKind::BracketClose | TokenKind::BraceClose
    ) && wordish(b)
    {
        return true;
    }

    if matches!(
        a,
        TokenKind::ParClose | TokenKind::BracketClose | TokenKind::BraceClose
    ) && matches!(
        b,
        TokenKind::ParOpen | TokenKind::BracketOpen | TokenKind::BraceOpen
    ) {
        return true;
    }

    if a == TokenKind::Arrow {
        return true;
    }
    if wordish(a) && b == TokenKind::Arrow {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(src: &str) -> String {
        format_source(src, &FmtConfig::default(), LexerConfig { version: 4 }).unwrap()
    }

    #[test]
    fn normalizes_spaces_around_equals() {
        let out = fmt("var  x  =  1  ;");
        assert_eq!(out, "var x = 1;");
    }

    #[test]
    fn preserves_line_comment_in_prefix() {
        let src = "// preamble\nvar x=1;";
        let out = fmt(src);
        assert!(out.contains("// preamble"));
        assert!(out.contains("var x = 1;"));
    }

    #[test]
    fn brace_block_inserts_indents() {
        let src = "if (true) { return 1; }";
        let out = fmt(src);
        assert!(out.contains("{\n"));
        assert!(out.contains("return 1;"));
        assert!(out.contains("\n}"));
    }

    #[test]
    fn rejects_unclosed_paren() {
        let err =
            format_source("(1", &FmtConfig::default(), LexerConfig { version: 4 }).unwrap_err();
        assert!(matches!(err, FormatError::Parse(_)));
    }
}
