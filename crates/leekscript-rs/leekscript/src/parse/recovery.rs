//! Recovering and partial-rule parsing on top of sipha.

use crate::grammar;
use crate::syntax::kinds::K;
use sipha::prelude::ParsedDoc;
use sipha::prelude::*;

use super::{LanguageOptions, ParseError, language_options_with_source_directives};

/// Successful parse with optional recovery diagnostics.
///
/// When [`Self::errors`] is non-empty, the CST may contain [`K::ErrorStmt`] placeholders (zero-width
/// nodes) from sipha’s error-node insertion. Valid siblings are still parsed normally.
#[derive(Debug)]
pub struct ParsedWithRecovery {
    pub doc: ParsedDoc,
    pub errors: Vec<ParseError>,
}

const DEFAULT_MAX_RECOVERY_ERRORS: usize = 64;

/// Parse a full document, recovering after top-level statement failures when possible.
///
/// Uses [`GrammarBuilder::recover_until`](sipha::prelude::GrammarBuilder::recover_until) at module
/// scope (sync on `;` and common statement keywords). Multiple errors are collected until
/// `max_errors` or end of input.
///
/// **Note:** sipha’s [`Engine::parse_recovering_multi_with_context`] only returns structured errors
/// in the `Err` path. If the parser recovers and reaches end-of-input successfully, `errors` may
/// be empty even though recovery ran.
#[must_use]
pub fn parse_doc_with_recovery(
    src: &str,
    lang: impl Into<LanguageOptions>,
) -> Result<ParsedWithRecovery, ParseError> {
    parse_doc_with_recovery_limited(src, lang, DEFAULT_MAX_RECOVERY_ERRORS)
}

/// Like [`parse_doc_with_recovery`], with an explicit cap on collected parse errors.
pub fn parse_doc_with_recovery_limited(
    src: &str,
    lang: impl Into<LanguageOptions>,
    max_errors: usize,
) -> Result<ParsedWithRecovery, ParseError> {
    let built = grammar::built_graph();
    let graph = built.as_graph();
    let mut engine = Engine::new();
    let opts = language_options_with_source_directives(src, lang);
    let ctx = opts
        .parse_context()
        .with_set(super::version::FLAG_PARSE_RECOVERY)
        .with_error_node_kind(K::ErrorStmt as sipha::types::SyntaxKind);

    let bytes = src.as_bytes();
    match engine.parse_recovering_multi_with_context(&graph, bytes, &ctx, max_errors.max(1)) {
        Ok(out) => {
            let doc = ParsedDoc::from_slice(bytes, &out).ok_or(ParseError::NoSyntaxRoot)?;
            Ok(ParsedWithRecovery {
                doc,
                errors: vec![],
            })
        }
        Err(multi) => {
            let doc =
                ParsedDoc::from_slice(bytes, &multi.partial).ok_or(ParseError::NoSyntaxRoot)?;
            Ok(ParsedWithRecovery {
                doc,
                errors: multi.errors.into_iter().map(ParseError::from).collect(),
            })
        }
    }
}

/// Parse a signature / stub document with recovery (like [`parse_doc_with_recovery`], but
/// [`LanguageOptions::signature_parse_context`] so `function … => T;` stubs parse).
#[must_use]
pub fn parse_signature_doc_with_recovery(
    src: &str,
    lang: impl Into<LanguageOptions>,
) -> Result<ParsedWithRecovery, ParseError> {
    parse_signature_doc_with_recovery_limited(src, lang, DEFAULT_MAX_RECOVERY_ERRORS)
}

/// Like [`parse_signature_doc_with_recovery`], with an explicit cap on collected parse errors.
pub fn parse_signature_doc_with_recovery_limited(
    src: &str,
    lang: impl Into<LanguageOptions>,
    max_errors: usize,
) -> Result<ParsedWithRecovery, ParseError> {
    let built = grammar::built_graph();
    let graph = built.as_graph();
    let mut engine = Engine::new();
    let opts = language_options_with_source_directives(src, lang);
    let ctx = opts
        .signature_parse_context()
        .with_set(super::version::FLAG_PARSE_RECOVERY)
        .with_error_node_kind(K::ErrorStmt as sipha::types::SyntaxKind);

    let bytes = src.as_bytes();
    match engine.parse_recovering_multi_with_context(&graph, bytes, &ctx, max_errors.max(1)) {
        Ok(out) => {
            let doc = ParsedDoc::from_slice(bytes, &out).ok_or(ParseError::NoSyntaxRoot)?;
            Ok(ParsedWithRecovery {
                doc,
                errors: vec![],
            })
        }
        Err(multi) => {
            let doc =
                ParsedDoc::from_slice(bytes, &multi.partial).ok_or(ParseError::NoSyntaxRoot)?;
            Ok(ParsedWithRecovery {
                doc,
                errors: multi.errors.into_iter().map(ParseError::from).collect(),
            })
        }
    }
}

/// Parse a grammar rule starting at `byte_offset` in `src` (fragment / partial input).
///
/// The rule name must exist in the built graph (e.g. `"stmt"`, `"expr"`). Requires the
/// `partial-reparse` crate feature (enabled in `leekscript` by default).
///
/// Returns the parsed [`ParsedDoc`] and the number of bytes consumed by the rule.
#[cfg(feature = "partial-reparse")]
pub fn parse_rule_at_offset(
    src: &str,
    lang: impl Into<LanguageOptions>,
    rule_name: &str,
    byte_offset: u32,
) -> Result<(ParsedDoc, u32), ParseError> {
    let built = grammar::built_graph();
    let graph = built.as_graph();
    let rule = graph.rule_id(rule_name).ok_or_else(|| {
        ParseError::from(sipha::parse::engine::ParseError::UnknownRuleName(
            rule_name.to_string(),
        ))
    })?;

    let mut engine = Engine::new();
    let ctx = lang.into().parse_context();
    let bytes = src.as_bytes();
    let start = byte_offset.min(bytes.len() as u32);
    let out = engine
        .parse_rule_at_with_context(&graph, bytes, rule, start, &ctx)
        .map_err(ParseError::from)?;
    let doc = ParsedDoc::from_slice(bytes, &out).ok_or(ParseError::NoSyntaxRoot)?;
    Ok((doc, out.consumed))
}

/// Best-effort parse: use recovery when the strict parse fails.
#[must_use]
pub fn parse_doc_or_recover(
    src: &str,
    lang: impl Into<LanguageOptions>,
) -> Result<ParsedWithRecovery, ParseError> {
    let opts = lang.into();
    match super::parse_doc(src, opts) {
        Ok(doc) => Ok(ParsedWithRecovery {
            doc,
            errors: vec![],
        }),
        Err(_) => parse_doc_with_recovery(src, opts),
    }
}
