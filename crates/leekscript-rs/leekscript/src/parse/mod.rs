mod error;
mod recovery;
pub(crate) mod version;

pub use error::ParseError;
pub use recovery::{
    ParsedWithRecovery, parse_doc_or_recover, parse_doc_with_recovery, parse_doc_with_recovery_limited,
};
#[cfg(feature = "partial-reparse")]
pub use recovery::parse_rule_at_offset;
pub use version::{Version, FLAG_PARSE_RECOVERY};

use crate::grammar;
use sipha::prelude::*;

/// Parse a full document.
///
/// If the `grammar-v4-only` Cargo feature is enabled, only [`Version::V4`] and [`Version::VNext`]
/// are supported; older [`Version`] values will not match the lexer/parser graph correctly.
pub fn parse_doc(src: &str, version: Version) -> Result<ParsedDoc, ParseError> {
    let built = grammar::built_graph();
    let graph = built.as_graph();

    let mut engine = Engine::new();
    let ctx = version.to_parse_context();

    let out = engine
        .parse_with_context(&graph, src.as_bytes(), &ctx)
        .map_err(ParseError::from)?;

    ParsedDoc::from_slice(src.as_bytes(), &out).ok_or(ParseError::NoSyntaxRoot)
}

pub fn parse_syntax_root(src: &str, version: Version) -> Result<SyntaxNode, ParseError> {
    Ok(parse_doc(src, version)?.root().clone())
}
