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
    scan_apply_leading_leeklang(src, &mut opts);
    opts
}

fn trim_ascii_start(mut s: &[u8]) -> &[u8] {
    while let Some((&b, rest)) = s.split_first() {
        if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
            s = rest;
        } else {
            break;
        }
    }
    s
}

/// True if `body` (bytes after `//`) might start a `leeklang:` directive after ASCII trim (matches
/// the common case; non-ASCII whitespace before the marker still falls through when we re-check
/// with full `str::trim`).
fn line_body_might_be_leeklang(body: &[u8]) -> bool {
    let b = trim_ascii_start(body);
    if b.is_empty() {
        return false;
    }
    if b[0] == b'!' {
        let b = trim_ascii_start(&b[1..]);
        return starts_with_leeklang_marker(b);
    }
    starts_with_leeklang_marker(b)
}

fn starts_with_leeklang_marker(b: &[u8]) -> bool {
    b.len() >= 9 && b[..9].eq_ignore_ascii_case(b"leeklang:")
}

/// True if `prefix` of `s` may contain UTF-8 that affects trimming (e.g. Unicode space before `leeklang:`).
fn prefix_might_contain_non_ascii(s: &[u8], prefix: usize) -> bool {
    s.iter().take(prefix.min(s.len())).copied().any(|b| b > 127)
}

fn scan_apply_leading_leeklang(src: &str, opts: &mut LanguageOptions) {
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
            let body = &bytes[i + 2..end];
            if line_body_might_be_leeklang(body) || prefix_might_contain_non_ascii(body, body.len())
            {
                if let Some(line) = src.get(start..end)
                    && let Some(body_str) = line.strip_prefix("//")
                {
                    let trimmed = body_str.trim();
                    if let Some((rest, _file_wide)) = leeklang_directive_rest(trimmed) {
                        apply_leeklang_rest(opts, rest);
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
            let inner = &bytes[start + 2..end];
            if line_body_might_be_leeklang(inner) || prefix_might_contain_non_ascii(inner, 256) {
                if let Some(block) = src.get(start..block_end)
                    && let Some(inner_str) =
                        block.strip_prefix("/*").and_then(|s| s.strip_suffix("*/"))
                {
                    let trimmed = inner_str.trim();
                    if let Some((rest, _file_wide)) = leeklang_directive_rest(trimmed) {
                        apply_leeklang_rest(opts, rest);
                    }
                }
            }
            i = block_end;
            continue;
        }
        break;
    }
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

fn write_normalized_leeklang_key(key: &str, out: &mut [u8; 96]) -> Option<usize> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    if key.len() > out.len() {
        return None;
    }
    for (i, &b) in key.as_bytes().iter().enumerate() {
        out[i] = match b {
            b'A'..=b'Z' => b + 32,
            b'-' => b'_',
            _ => b,
        };
    }
    Some(key.len())
}

fn apply_leeklang_rest(opts: &mut LanguageOptions, rest: &str) {
    let mut key_buf = [0u8; 96];
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
            let Some(n) = write_normalized_leeklang_key(key, &mut key_buf) else {
                continue;
            };
            let key_norm = std::str::from_utf8(&key_buf[..n]).expect("leeklang keys are ASCII");
            apply_kv(opts, key_norm, value.trim());
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
            let v = value.trim();
            if v.eq_ignore_ascii_case("all")
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("yes")
                || v.eq_ignore_ascii_case("on")
                || v == "1"
            {
                opts.experimental = ExperimentalFeatures::ALL;
            } else if v.eq_ignore_ascii_case("none")
                || v.eq_ignore_ascii_case("false")
                || v.eq_ignore_ascii_case("no")
                || v.eq_ignore_ascii_case("off")
                || v == "0"
            {
                opts.experimental = ExperimentalFeatures::NONE;
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
    let s = s.trim();
    if s.eq_ignore_ascii_case("1")
        || s.eq_ignore_ascii_case("true")
        || s.eq_ignore_ascii_case("yes")
        || s.eq_ignore_ascii_case("on")
    {
        Some(true)
    } else if s.eq_ignore_ascii_case("0")
        || s.eq_ignore_ascii_case("false")
        || s.eq_ignore_ascii_case("no")
        || s.eq_ignore_ascii_case("off")
    {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_then_line_directive_version() {
        let src = "\u{FEFF}// leeklang: version=v2\nVAR x = 1\n";
        let o = language_options_with_source_directives(
            src,
            LanguageOptions::new(Version::V4, ExperimentalFeatures::NONE),
        );
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
        let o = language_options_with_source_directives(
            src,
            LanguageOptions::new(Version::V4, ExperimentalFeatures::NONE),
        );
        assert_eq!(o.version, Version::V3);
    }

    #[test]
    fn stops_at_first_non_comment() {
        let src = "// leeklang: version=v1\nnot_a_comment // leeklang: version=v4\n";
        let o = language_options_with_source_directives(
            src,
            LanguageOptions::new(Version::V4, ExperimentalFeatures::NONE),
        );
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

    #[test]
    fn unicode_space_before_leeklang_on_line_comment() {
        let src = format!("//\u{00A0}leeklang: version=v2\nvar x = 1;\n");
        let o = language_options_with_source_directives(
            &src,
            LanguageOptions::new(Version::V4, ExperimentalFeatures::NONE),
        );
        assert_eq!(o.version, Version::V2);
    }
}
