//! Doxygen-style documentation: sipha tokenization + command lowering.
//!
//! - [`parse_doxygen`] prefers the sipha grammar in [`sipha_parse`], then falls back to the hand-written
//!   scanner in [`scan`] if parsing fails.
//! - Public types live in [`model`].

mod commands;
mod model;
mod scan;
mod sipha_parse;

pub use model::{DoxygenParam, DoxygenRetval, DoxygenThrows, ParsedDoxygen};

use commands::apply_segments;
use scan::split_command_segments;
use sipha_parse::split_via_sipha;

/// Parse a single Doxygen comment body (no `/**` / `*/` delimiters).
#[must_use]
pub fn parse_doxygen(body: &str) -> ParsedDoxygen {
    let raw = body.trim().to_string();
    if raw.is_empty() {
        return ParsedDoxygen::default();
    }

    let segments = split_via_sipha(&raw).unwrap_or_else(|| split_command_segments(&raw));
    apply_segments(segments, raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brief_param_return() {
        let p = parse_doxygen(
            r"\brief Adds two integers.
\param a the first operand
\param b the second operand
\return a + b",
        );
        assert_eq!(p.brief.as_deref(), Some("Adds two integers."));
        assert_eq!(p.params.len(), 2);
        assert_eq!(p.params[0].name, "a");
        assert_eq!(p.params[1].name, "b");
        assert_eq!(p.returns.as_deref(), Some("a + b"));
    }

    #[test]
    fn at_style_commands() {
        let p = parse_doxygen("@brief Short.\n@details Longer text here.");
        assert_eq!(p.brief.as_deref(), Some("Short."));
        assert_eq!(p.details.as_deref(), Some("Longer text here."));
    }

    #[test]
    fn implicit_brief_when_no_command() {
        let p = parse_doxygen("Just a sentence.");
        assert_eq!(p.brief.as_deref(), Some("Just a sentence."));
    }

    #[test]
    fn leading_before_brief_goes_to_details() {
        let p = parse_doxygen("Intro line.\n\\brief Real brief.");
        assert_eq!(p.brief.as_deref(), Some("Real brief."));
        assert_eq!(p.details.as_deref(), Some("Intro line."));
    }

    #[test]
    fn param_direction_brackets() {
        let p = parse_doxygen(r"\param[in] dst target buffer \param[out] err optional error");
        assert_eq!(p.params.len(), 2);
        assert_eq!(p.params[0].direction.as_deref(), Some("in"));
        assert_eq!(p.params[0].name, "dst");
        assert_eq!(p.params[1].direction.as_deref(), Some("out"));
        assert_eq!(p.params[1].name, "err");
    }

    #[test]
    fn tparam_retval_and_unknown() {
        let p = parse_doxygen(
            r"\tparam T type arg
\retval 0 success
\retval -1 failure
\fn void f()
\file x.leek",
        );
        assert_eq!(p.template_params.len(), 1);
        assert_eq!(p.template_params[0].name, "T");
        assert_eq!(p.retvals.len(), 2);
        assert_eq!(p.retvals[0].value, "0");
        assert!(p.unknown.iter().any(|(n, _)| n == "fn" || n == "file"));
    }
}
