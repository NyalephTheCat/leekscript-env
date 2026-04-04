mod error;
mod lang_directive;
mod recovery;
pub(crate) mod version;

pub use error::{ParseError, ParseErrorInner};
#[cfg(feature = "partial-reparse")]
pub use recovery::parse_rule_at_offset;
pub use recovery::{
    ParsedWithRecovery, parse_doc_or_recover, parse_doc_with_recovery,
    parse_doc_with_recovery_limited, parse_signature_doc_with_recovery,
    parse_signature_doc_with_recovery_limited,
};
pub use lang_directive::language_options_with_source_directives;
pub use version::{
    ExperimentalFeatures, LanguageOptions, Version, FLAG_PARSE_RECOVERY, FLAG_SIGNATURE_MODE,
};

use std::cell::RefCell;

use crate::grammar;
use sipha::prelude::*;

thread_local! {
    /// Reuse allocation-heavy parse buffers between calls on the same thread (Criterion, LSP, CLI).
    /// [`RefCell::try_borrow_mut`] avoids panics if parsing is re-entered on the same thread.
    /// Larger initial capacities than [`Engine::new`] reduce reallocations on big inputs (e.g. std stubs).
    static REUSABLE_PARSE_ENGINE: RefCell<Engine> = RefCell::new(Engine::with_capacity(8192, 8192));
}

/// Run `f` with a thread-local [`Engine`] when possible (see [`REUSABLE_PARSE_ENGINE`]).
pub(crate) fn with_reusable_engine<R>(f: impl FnOnce(&mut Engine) -> R) -> R {
    REUSABLE_PARSE_ENGINE.with(|cell| {
        if let Ok(mut engine) = cell.try_borrow_mut() {
            f(&mut *engine)
        } else {
            f(&mut Engine::with_capacity(8192, 8192))
        }
    })
}

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
    let bytes = src.as_bytes();

    let out = with_reusable_engine(|engine| {
        engine
            .parse_with_context(&graph, bytes, &ctx)
            .map_err(ParseError::from)
    })?;

    ParsedDoc::from_slice(bytes, &out).ok_or(ParseError::NoSyntaxRoot)
}

/// Parse with `buf` as the working copy of the source: on success the bytes are moved into
/// [`ParsedDoc`] and `buf` is left empty (default-constructed). Return the buffer with
/// [`ParsedDoc::into_bytes`](sipha::diagnostics::parsed_doc::ParsedDoc::into_bytes) after use so
/// `buf` keeps its capacity for the next call (see crate benchmarks).
#[must_use]
pub fn parse_doc_reusing_vec(
    src: &str,
    lang: impl Into<LanguageOptions>,
    buf: &mut Vec<u8>,
) -> Result<ParsedDoc, ParseError> {
    let opts = language_options_with_source_directives(src, lang);
    buf.clear();
    buf.extend_from_slice(src.as_bytes());
    parse_doc_buffer_with_context(buf, opts.parse_context())
}

/// Like [`parse_signature_doc`], but uses the same buffer reuse contract as [`parse_doc_reusing_vec`].
#[must_use]
pub fn parse_signature_doc_reusing_vec(
    src: &str,
    lang: impl Into<LanguageOptions>,
    buf: &mut Vec<u8>,
) -> Result<ParsedDoc, ParseError> {
    let opts = language_options_with_source_directives(src, lang);
    buf.clear();
    buf.extend_from_slice(src.as_bytes());
    parse_doc_buffer_with_context(buf, opts.signature_parse_context())
}

fn parse_doc_buffer_with_context(buf: &mut Vec<u8>, ctx: ParseContext) -> Result<ParsedDoc, ParseError> {
    let built = grammar::built_graph();
    let graph = built.as_graph();
    let out = with_reusable_engine(|engine| {
        engine
            .parse_with_context(&graph, buf, &ctx)
            .map_err(ParseError::from)
    })?;
    let source = std::mem::replace(buf, Vec::new());
    ParsedDoc::new(source, &out).ok_or(ParseError::NoSyntaxRoot)
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
