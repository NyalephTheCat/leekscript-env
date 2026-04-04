//! Leading `// leeklang: …`, `//! leeklang: …`, and `/* leeklang: … */` directives (marker is
//! case-insensitive `leeklang:` or `LEEKLANG:`).
//!
//! Parsed **before** the main parse pass (unlike `leekfmt:` directives, which are read from trivia
//! after a successful parse). Only a **prefix** of the file is scanned: UTF-8 BOM, then any mix of
//! blank lines, line comments, and block comments, until the first byte that is not whitespace and
//! does not start a `//` or `/*` comment.
//!
//! Directives are merged **on top of** the caller-supplied [`LanguageOptions`] (e.g. CLI flags) in
//! source order; later assignments override earlier ones. Separate settings with `;` or `,`, or
//! spaces between `key=value` pairs (same spirit as `leekfmt:`).
//!
//! # Keys
//!
//! - **`version`** / **`dialect`** — `v1` … `v4`, or `1` … `4`, or `ls1` … `ls4`.
//! - **`experimental`** — `all` / `true` / `on` / `yes` / `1` (enable every experimental flag), or
//!   `none` / `false` / `off` / `no` / `0` (clear all experimental flags).
//! - **Per-flag** (booleans as `true`/`false`/`on`/`off`/`yes`/`no`/`1`/`0`):
//!   `experimental-let`, `experimental-const` (alias `experimental-lexical-const`),
//!   `experimental-match`, `experimental-modules`, `experimental-exceptions`, `experimental-goto`,
//!   `experimental-loop-levels`.
//!
//! Example:
//! `//! leeklang: version=v4 experimental=none experimental-let=true`
//!
//! **Merged buffers** (signatures + project, or `merge` output): only comments at the **very
//! beginning** of the combined string are read; inner files’ `leeklang:` lines are not scanned.

use super::version::{ExperimentalFeatures, LanguageOptions, Version};

/// Apply leading `leeklang:` comments in `src` on top of `base` (CLI / default options).
#[must_use]
pub fn language_options_with_source_directives(
    src: &str,
    base: impl Into<LanguageOptions>,
) -> LanguageOptions {
    let mut opts = base.into();
    for rest in scan_leading_leeklang_rests(src) {
        apply_leeklang_rest(&mut opts, &rest);
    }
    opts
}

fn scan_leading_leeklang_rests(src: &str) -> Vec<String> {
    let mut rests = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0usize;
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        i = 3;
    }
    loop {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            let start = i;
            let mut end = i + 2;
            while end < bytes.len() && bytes[end] != b'\n' {
                end += 1;
            }
            if let Some(line) = src.get(start..end) {
                if let Some(body) = line.strip_prefix("//") {
                    let trimmed = body.trim();
                    if let Some((rest, _file_wide)) = leeklang_directive_rest(trimmed) {
                        rests.push(rest.to_string());
                    }
                }
            }
            i = if end < bytes.len() { end + 1 } else { end };
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let start = i;
            let mut end = i + 2;
            while end + 1 < bytes.len() && !(bytes[end] == b'*' && bytes[end + 1] == b'/') {
                end += 1;
            }
            if end + 1 >= bytes.len() {
                break;
            }
            let block_end = end + 2;
            if let Some(block) = src.get(start..block_end) {
                if let Some(inner) = block
                    .strip_prefix("/*")
                    .and_then(|s| s.strip_suffix("*/"))
                {
                    let trimmed = inner.trim();
                    if let Some((rest, _file_wide)) = leeklang_directive_rest(trimmed) {
                        rests.push(rest.to_string());
                    }
                }
            }
            i = block_end;
            continue;
        }
        break;
    }
    rests
}

/// `rest` is the payload after the `leeklang:` marker. The `bool` matches `leekfmt`’s file-wide `!`
/// form (reserved for future use).
fn leeklang_directive_rest(trimmed: &str) -> Option<(&str, bool)> {
    if let Some(rest) = trimmed
        .strip_prefix("leeklang:")
        .or_else(|| trimmed.strip_prefix("LEEKLANG:"))
    {
        return Some((rest.trim_start(), false));
    }
    let after_bang = trimmed.strip_prefix('!')?.trim_start();
    let rest = after_bang
        .strip_prefix("leeklang:")
        .or_else(|| after_bang.strip_prefix("LEEKLANG:"))?;
    Some((rest.trim_start(), true))
}

fn apply_leeklang_rest(opts: &mut LanguageOptions, rest: &str) {
    for part in rest.split([';', ',']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        for segment in part.split_whitespace() {
            let segment = segment.trim();
            if segment.is_empty() {
                continue;
            }
            let (key, value) = segment
                .split_once('=')
                .or_else(|| segment.split_once(':'))
                .unwrap_or((segment, ""));
            let key = key.trim().to_ascii_lowercase().replace('-', "_");
            let value = value.trim();
            apply_kv(opts, &key, value);
        }
    }
}

fn apply_kv(opts: &mut LanguageOptions, key: &str, value: &str) {
    match key {
        "version" | "dialect" => {
            if let Some(v) = Version::parse_dialect_label(value) {
                opts.version = v;
            }
        }
        "experimental" => {
            let v = value.to_ascii_lowercase();
            match v.as_str() {
                "all" | "true" | "yes" | "on" | "1" => {
                    opts.experimental = ExperimentalFeatures::ALL;
                }
                "none" | "false" | "no" | "off" | "0" => {
                    opts.experimental = ExperimentalFeatures::NONE;
                }
                _ => {}
            }
        }
        "experimental_let" => {
            if let Some(b) = parse_bool_loose(value) {
                opts.experimental.let_bindings = b;
            }
        }
        "experimental_const" | "experimental_lexical_const" => {
            if let Some(b) = parse_bool_loose(value) {
                opts.experimental.lexical_const = b;
            }
        }
        "experimental_match" => {
            if let Some(b) = parse_bool_loose(value) {
                opts.experimental.match_stmt = b;
            }
        }
        "experimental_modules" => {
            if let Some(b) = parse_bool_loose(value) {
                opts.experimental.modules = b;
            }
        }
        "experimental_exceptions" => {
            if let Some(b) = parse_bool_loose(value) {
                opts.experimental.exceptions = b;
            }
        }
        "experimental_goto" => {
            if let Some(b) = parse_bool_loose(value) {
                opts.experimental.goto = b;
            }
        }
        "experimental_loop_levels" => {
            if let Some(b) = parse_bool_loose(value) {
                opts.experimental.loop_levels = b;
            }
        }
        "experimental_fn_optional_params" | "experimental_function_optional_params" => {
            if let Some(b) = parse_bool_loose(value) {
                opts.experimental.fn_optional_params = b;
            }
        }
        "experimental_templates" | "experimental_generics" => {
            if let Some(b) = parse_bool_loose(value) {
                opts.experimental.templates = b;
            }
        }
        _ => {}
    }
}

fn parse_bool_loose(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_then_line_directive_version() {
        let src = "\u{FEFF}// leeklang: version=v2\nVAR x = 1\n";
        let o = language_options_with_source_directives(src, LanguageOptions::new(Version::V4, ExperimentalFeatures::NONE));
        assert_eq!(o.version, Version::V2);
    }

    #[test]
    fn bang_line_sets_experimental_subset() {
        let src = "//! leeklang: experimental=all experimental-let=false\nlet x = 1;\n";
        let o = language_options_with_source_directives(src, LanguageOptions::default());
        assert!(o.experimental.match_stmt);
        assert!(!o.experimental.let_bindings);
    }

    #[test]
    fn block_comment_directive() {
        let src = "/* leeklang: dialect=v3 */\nvar x = 1;\n";
        let o = language_options_with_source_directives(src, LanguageOptions::new(Version::V4, ExperimentalFeatures::NONE));
        assert_eq!(o.version, Version::V3);
    }

    #[test]
    fn stops_at_first_non_comment() {
        let src = "// leeklang: version=v1\nnot_a_comment // leeklang: version=v4\n";
        let o = language_options_with_source_directives(src, LanguageOptions::new(Version::V4, ExperimentalFeatures::NONE));
        assert_eq!(o.version, Version::V1);
    }

    #[test]
    fn only_experimental_let_true_enables_flag() {
        let src = "//! leeklang: experimental-let=true\nlet x = 1;\n";
        let o = language_options_with_source_directives(
            src,
            LanguageOptions::new(Version::V4, ExperimentalFeatures::NONE),
        );
        assert!(o.experimental.let_bindings, "{o:?}");
    }
}
