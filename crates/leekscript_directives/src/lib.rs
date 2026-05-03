//! File preamble `// leek-*` directives — see `docs/reference/directives.md`.
//!
//! Scans up to `max_lines` physical lines (default 64), stopping early at the first line that is not
//! blank, a line comment, or a block comment.

#![warn(clippy::pedantic)]

use leekscript_span::Span;
use serde::Serialize;

/// Registry `id` for [`DirectiveDiagnostic::registry_id`].
pub const UNKNOWN_LEEK_DIRECTIVE: &str = "unknown_leek_directive";
pub const LEEK_DIRECTIVE_INVALID_VALUE: &str = "leek_directive_invalid_value";

/// Parsed `// leek-fmt: width=…, indent=…` hints (for `lek fmt` / LSP; does not affect `lek check` lexing).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FmtPreamble {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_tabs: Option<bool>,
}

/// Effective settings from the file preamble (narrower than `Leek.toml` per precedence docs).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilePreamble {
    pub language_version: Option<u8>,
    pub strict: Option<bool>,
    pub fmt: Option<FmtPreamble>,
    /// Feature names from `// leek-experimental: a, b` (stored for tooling; lexing unchanged).
    pub experimental: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveDiagnostic {
    /// Registry toolchain `id` (e.g. [`UNKNOWN_LEEK_DIRECTIVE`]).
    pub registry_id: &'static str,
    pub span: Span,
    pub message: String,
}

/// Parse leading comments; `max_lines` caps how many physical lines are examined.
#[must_use]
pub fn parse_file_preamble(
    src: &str,
    max_lines: usize,
) -> (FilePreamble, Vec<DirectiveDiagnostic>) {
    let mut preamble = FilePreamble::default();
    let mut diags = Vec::new();
    let mut offset = 0usize;
    let mut line_no = 0usize;
    let mut block_comment = false;
    while line_no < max_lines && offset <= src.len() {
        let line_end = src[offset..].find('\n').map_or(src.len(), |p| offset + p);
        let line = &src[offset..line_end];
        let line_start = offset;

        if block_comment {
            if line.contains("*/") {
                block_comment = false;
            }
        } else {
            let t = line.trim();
            if t.is_empty() {
                // still preamble
            } else if t.starts_with("//") {
                parse_line(line, line_start, &mut preamble, &mut diags);
            } else if t.starts_with("/*") {
                if !t.contains("*/") {
                    block_comment = true;
                }
            } else {
                break;
            }
        }

        if line_end >= src.len() {
            break;
        }
        offset = line_end + 1;
        line_no += 1;
    }
    (preamble, diags)
}

fn parse_line(
    line: &str,
    line_start: usize,
    preamble: &mut FilePreamble,
    diags: &mut Vec<DirectiveDiagnostic>,
) {
    let trimmed = line.trim_start();
    let Some(after_slashes) = trimmed.strip_prefix("//") else {
        return;
    };
    let after_slashes = after_slashes.trim_start();
    let Some(rest) = after_slashes.strip_prefix("leek-") else {
        return;
    };

    let Some(leek_rel) = line.find("leek-") else {
        return;
    };
    let leek_start = line_start + leek_rel;
    let after_leek = leek_start + "leek-".len();
    let line_end = line_start + line.len();

    let (name, value) = split_key_value(rest);
    let name = name.trim();
    if name.is_empty() {
        return;
    }

    match name {
        "version" => match parse_version(value) {
            Ok(v) => preamble.language_version = Some(v),
            Err(msg) => diags.push(DirectiveDiagnostic {
                registry_id: LEEK_DIRECTIVE_INVALID_VALUE,
                span: Span::new(after_leek..line_end),
                message: msg,
            }),
        },
        "strict" => match parse_strict(value) {
            Ok(v) => preamble.strict = Some(v),
            Err(msg) => diags.push(DirectiveDiagnostic {
                registry_id: LEEK_DIRECTIVE_INVALID_VALUE,
                span: Span::new(after_leek..line_end),
                message: msg,
            }),
        },
        "fmt" => match parse_fmt(value) {
            Ok(f) => preamble.fmt = Some(f),
            Err(msg) => diags.push(DirectiveDiagnostic {
                registry_id: LEEK_DIRECTIVE_INVALID_VALUE,
                span: Span::new(after_leek..line_end),
                message: msg,
            }),
        },
        "experimental" => match parse_experimental(value) {
            Ok(v) => preamble.experimental = Some(v),
            Err(msg) => diags.push(DirectiveDiagnostic {
                registry_id: LEEK_DIRECTIVE_INVALID_VALUE,
                span: Span::new(after_leek..line_end),
                message: msg,
            }),
        },
        "allow" | "push" | "pop" => {}
        _ => {
            diags.push(DirectiveDiagnostic {
                registry_id: UNKNOWN_LEEK_DIRECTIVE,
                span: Span::new(leek_start..line_end),
                message: format!("unknown leek directive `{name}`"),
            });
        }
    }
}

fn split_key_value(s: &str) -> (&str, Option<&str>) {
    for (i, c) in s.char_indices() {
        if c == ':' || c == '=' {
            let name = s[..i].trim_end();
            let val = s[i + c.len_utf8()..].trim_start();
            return (name, Some(val));
        }
    }
    (s.trim(), None)
}

fn parse_version(value: Option<&str>) -> Result<u8, String> {
    let Some(raw) = value else {
        return Err("expected a version number after `leek-version`".into());
    };
    let raw = raw.trim();
    let n: u8 = raw
        .parse()
        .map_err(|_| format!("`{raw}` is not a valid language version"))?;
    if !(1..=99).contains(&n) {
        return Err(format!(
            "language version must be between 1 and 99, got {n}"
        ));
    }
    Ok(n)
}

fn parse_strict(value: Option<&str>) -> Result<bool, String> {
    match value {
        None => Ok(true),
        Some(v) => {
            let v = v.trim();
            match v {
                "true" | "1" | "yes" => Ok(true),
                "false" | "0" | "no" => Ok(false),
                _ => Err(format!("`{v}` is not a valid boolean (use true or false)")),
            }
        }
    }
}

fn parse_fmt(value: Option<&str>) -> Result<FmtPreamble, String> {
    let raw = value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "expected key=value pairs after `leek-fmt` (e.g. width=100, indent=4)".to_string()
        })?;

    let mut f = FmtPreamble::default();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let eq = part
            .find('=')
            .ok_or_else(|| format!("expected key=value in leek-fmt fragment `{part}`"))?;
        let key = part[..eq].trim();
        let val = part[eq + 1..].trim();
        match key {
            "width" => f.width = Some(parse_positive_u32(val, "width")?),
            "indent" => f.indent = Some(parse_positive_u32(val, "indent")?),
            "tab_width" => f.tab_width = Some(parse_positive_u32(val, "tab_width")?),
            "use_tabs" => f.use_tabs = Some(parse_bool_lit(val)?),
            _ => {
                return Err(format!(
                    "unknown key `{key}` in leek-fmt (allowed: width, indent, tab_width, use_tabs)"
                ));
            }
        }
    }
    Ok(f)
}

fn parse_positive_u32(s: &str, key: &str) -> Result<u32, String> {
    let n: u32 = s
        .parse()
        .map_err(|_| format!("`{s}` is not a valid integer for {key}"))?;
    if n == 0 {
        return Err(format!("{key} must be greater than 0"));
    }
    Ok(n)
}

fn parse_bool_lit(s: &str) -> Result<bool, String> {
    match s.trim() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(format!("`{s}` is not a valid boolean (use true or false)")),
    }
}

fn parse_experimental(value: Option<&str>) -> Result<Vec<String>, String> {
    let raw = value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "expected at least one feature name after `leek-experimental`".to_string()
        })?;
    let v: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if v.is_empty() {
        return Err("leek-experimental list is empty".into());
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_and_strict() {
        let src = "// leek-version: 3\n// leek-strict: false\nvar x = 1;\n";
        let (p, d) = parse_file_preamble(src, 64);
        assert!(d.is_empty());
        assert_eq!(p.language_version, Some(3));
        assert_eq!(p.strict, Some(false));
    }

    #[test]
    fn version_equals_form() {
        let src = "// leek-version=2\n";
        let (p, d) = parse_file_preamble(src, 64);
        assert!(d.is_empty());
        assert_eq!(p.language_version, Some(2));
    }

    #[test]
    fn strict_flag_only() {
        let src = "// leek-strict\n";
        let (p, d) = parse_file_preamble(src, 64);
        assert!(d.is_empty());
        assert_eq!(p.strict, Some(true));
    }

    #[test]
    fn unknown_directive() {
        let src = "// leek-foo: 1\n";
        let (p, d) = parse_file_preamble(src, 64);
        assert_eq!(p, FilePreamble::default());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].registry_id, UNKNOWN_LEEK_DIRECTIVE);
    }

    #[test]
    fn invalid_version() {
        let src = "// leek-version: 0\n";
        let (p, d) = parse_file_preamble(src, 64);
        assert_eq!(p.language_version, None);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].registry_id, LEEK_DIRECTIVE_INVALID_VALUE);
    }

    #[test]
    fn ignores_after_64_lines() {
        let mut src = String::new();
        for _ in 0..65 {
            src.push_str("// x\n");
        }
        src.push_str("// leek-version: 1\n");
        let (p, _) = parse_file_preamble(&src, 64);
        assert_eq!(p.language_version, None);
    }

    #[test]
    fn code_stops_preamble() {
        let src = "// leek-version: 1\nvar x = 1;\n// leek-version: 3\n";
        let (p, d) = parse_file_preamble(src, 64);
        assert!(d.is_empty());
        assert_eq!(p.language_version, Some(1));
    }

    #[test]
    fn block_comment_then_directive() {
        let src = "/* outer */\n// leek-version: 2\n";
        let (p, d) = parse_file_preamble(src, 64);
        assert!(d.is_empty());
        assert_eq!(p.language_version, Some(2));
    }

    #[test]
    fn multiline_block_skips_inner_slash_slash() {
        let src = "/*\n// leek-version: 9\n*/\n// leek-version: 2\n";
        let (p, _) = parse_file_preamble(src, 64);
        assert_eq!(p.language_version, Some(2));
    }

    #[test]
    fn leek_fmt_parses() {
        let src = "// leek-fmt: width=100, indent=2, use_tabs=false\n";
        let (p, d) = parse_file_preamble(src, 64);
        assert!(d.is_empty(), "{d:?}");
        let f = p.fmt.expect("fmt");
        assert_eq!(f.width, Some(100));
        assert_eq!(f.indent, Some(2));
        assert_eq!(f.use_tabs, Some(false));
    }

    #[test]
    fn leek_fmt_unknown_key_errors() {
        let src = "// leek-fmt: bogus=1\n";
        let (p, d) = parse_file_preamble(src, 64);
        assert!(p.fmt.is_none());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].registry_id, LEEK_DIRECTIVE_INVALID_VALUE);
    }

    #[test]
    fn leek_experimental() {
        let src = "// leek-experimental: ai_hints, preview_types\n";
        let (p, d) = parse_file_preamble(src, 64);
        assert!(d.is_empty());
        assert_eq!(
            p.experimental,
            Some(vec!["ai_hints".into(), "preview_types".into()])
        );
    }
}
