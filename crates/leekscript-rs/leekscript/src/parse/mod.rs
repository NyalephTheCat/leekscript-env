mod error;
mod lang_directive;
mod recovery;
pub(crate) mod version;

pub use error::ParseError;
#[cfg(feature = "partial-reparse")]
pub use recovery::parse_rule_at_offset;
pub use recovery::{
    ParsedWithRecovery, parse_doc_or_recover, parse_doc_with_recovery,
    parse_doc_with_recovery_limited,
};
pub use lang_directive::language_options_with_source_directives;
pub use version::{
    ExperimentalFeatures, LanguageOptions, Version, FLAG_PARSE_RECOVERY, FLAG_SIGNATURE_MODE,
};

use crate::grammar;
use sipha::prelude::*;

/// `true` when `path` looks like a generated API / stdlib stub (e.g. `std.sig.leek`, `std.sig.en.leek`).
#[must_use]
pub fn is_signature_stub_path(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if !name.ends_with(".leek") {
        return false;
    }
    name.ends_with(".sig.leek") || name.contains(".sig.")
}

fn parse_doc_with_context(src: &str, ctx: ParseContext) -> Result<ParsedDoc, ParseError> {
    let built = grammar::built_graph();
    let graph = built.as_graph();

    let mut engine = Engine::new();
    let out = engine
        .parse_with_context(&graph, src.as_bytes(), &ctx)
        .map_err(ParseError::from)?;

    ParsedDoc::from_slice(src.as_bytes(), &out).ok_or(ParseError::NoSyntaxRoot)
}

/// Parse a full document.
///
/// `lang` is usually a [`LanguageOptions`] or a [`Version`] (which implies no experimental flags).
/// Leading `leeklang:` comments are merged on top of `lang` (see [`language_options_with_source_directives`]).
///
/// If the `grammar-v4-only` Cargo feature is enabled, only [`Version::V4`] is supported as the
/// base dialect; older [`Version`] values will not match the lexer/parser graph correctly.
pub fn parse_doc(src: &str, lang: impl Into<LanguageOptions>) -> Result<ParsedDoc, ParseError> {
    let opts = language_options_with_source_directives(src, lang);
    parse_doc_with_context(src, opts.parse_context())
}

/// Parse a signature / stub document: top-level `function` may end with `;` instead of a block.
///
/// Use [`is_signature_stub_path`] for filename heuristics. When prepending `--signatures` to a
/// check buffer, use this mode for the combined source so the prelude and project parse together.
/// Leading `leeklang:` comments are applied the same way as for [`parse_doc`].
pub fn parse_signature_doc(
    src: &str,
    lang: impl Into<LanguageOptions>,
) -> Result<ParsedDoc, ParseError> {
    let opts = language_options_with_source_directives(src, lang);
    parse_doc_with_context(src, opts.signature_parse_context())
}

pub fn parse_syntax_root(
    src: &str,
    lang: impl Into<LanguageOptions>,
) -> Result<SyntaxNode, ParseError> {
    Ok(parse_doc(src, lang)?.root().clone())
}
