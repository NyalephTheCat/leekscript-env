//! `leekscript format` — pretty-print and TOML format config.

use std::collections::VecDeque;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use leekscript::format::{BraceStyle, FormatOptions, LineEnding, SemicolonStyle, format_document};
use leekscript::include::infer_include_project_root;
use leekscript::{LanguageOptions, prepare_merged_check_unit};
use serde::Deserialize;

use crate::args::{CliBraceStyle, CliLineEnding, CliSemicolonStyle};
use crate::report;

#[derive(Clone, Debug, Default, Deserialize)]
struct TomlFormatOptions {
    #[serde(alias = "indent-width")]
    indent_width: Option<usize>,
    #[serde(alias = "use-tabs")]
    use_tabs: Option<bool>,
    #[serde(alias = "indent-style", alias = "indent_style")]
    indent_style: Option<String>,
    #[serde(alias = "tab-width")]
    tab_width: Option<usize>,
    #[serde(alias = "line-width")]
    line_width: Option<usize>,
    #[serde(alias = "brace-style")]
    brace_style: Option<String>,
    #[serde(alias = "blank-lines-between-top-level")]
    blank_lines_between_top_level: Option<usize>,
    #[serde(alias = "blank-lines-after-class")]
    blank_lines_after_class: Option<usize>,

    #[serde(alias = "space-after-keyword-before-paren")]
    space_after_keyword_before_paren: Option<bool>,
    #[serde(alias = "space-before-function-decl-paren")]
    space_before_function_decl_paren: Option<bool>,
    #[serde(alias = "space-inside-parens")]
    space_inside_parens: Option<bool>,
    #[serde(alias = "space-around-assign")]
    space_around_assign: Option<bool>,
    #[serde(alias = "space-around-binary-ops")]
    space_around_binary_ops: Option<bool>,
    #[serde(alias = "space-after-comma")]
    space_after_comma: Option<bool>,
    #[serde(alias = "space-around-type-operators")]
    space_around_type_operators: Option<bool>,
    #[serde(alias = "newline-before-else-catch-finally")]
    newline_before_else_catch_finally: Option<bool>,

    #[serde(alias = "trailing-newline")]
    trailing_newline: Option<bool>,
    #[serde(alias = "blank-lines-between-block-statements")]
    blank_lines_between_block_statements: Option<usize>,
    #[serde(alias = "blank-lines-between-class-members")]
    blank_lines_between_class_members: Option<usize>,
    #[serde(alias = "max-consecutive-blank-lines-in-block")]
    max_consecutive_blank_lines_in_block: Option<usize>,
    #[serde(alias = "line-ending")]
    line_ending: Option<String>,
    #[serde(alias = "semicolon-style", alias = "semicolons")]
    semicolon_style: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct TomlConfig {
    #[serde(default)]
    format: Option<TomlFormatOptions>,
    // Allow configs that put format keys at the root.
    #[serde(flatten)]
    root_format: TomlFormatOptions,
}

pub(crate) fn load_format_config(path: &Path) -> Result<FormatOptions, String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let cfg: TomlConfig = toml::from_str(&src).map_err(|e| e.to_string())?;

    let mut o = FormatOptions::default();
    if let Some(f) = cfg.format {
        apply_toml_format_options(&mut o, &f);
    }
    apply_toml_format_options(&mut o, &cfg.root_format);
    Ok(o)
}

fn apply_toml_format_options(into: &mut FormatOptions, f: &TomlFormatOptions) {
    if let Some(v) = f.indent_width {
        into.indent_width = v.clamp(1, 32);
    }
    if let Some(v) = f.use_tabs {
        into.use_tabs = v;
    }
    if let Some(v) = f.indent_style.as_deref() {
        let v = v.trim().to_ascii_lowercase();
        if matches!(v.as_str(), "tabs" | "tab") {
            into.use_tabs = true;
        } else if matches!(v.as_str(), "spaces" | "space") {
            into.use_tabs = false;
        }
    }
    if let Some(v) = f.tab_width {
        into.tab_width = v.clamp(1, 32);
    }
    if let Some(v) = f.line_width {
        into.line_width = if v == 0 { 0 } else { v.clamp(20, 500) };
    }
    if let Some(v) = f.brace_style.as_deref() {
        if let Some(bs) = parse_brace_style(v) {
            into.brace_style = bs;
        }
    }
    if let Some(v) = f.blank_lines_between_top_level {
        into.blank_lines_between_top_level = v.min(10);
    }
    if let Some(v) = f.blank_lines_after_class {
        into.blank_lines_after_class = v.min(10);
    }
    if let Some(v) = f.space_after_keyword_before_paren {
        into.space_after_keyword_before_paren = v;
    }
    if let Some(v) = f.space_before_function_decl_paren {
        into.space_before_function_decl_paren = v;
    }
    if let Some(v) = f.space_inside_parens {
        into.space_inside_parens = v;
    }
    if let Some(v) = f.space_around_assign {
        into.space_around_assign = v;
    }
    if let Some(v) = f.space_around_binary_ops {
        into.space_around_binary_ops = v;
    }
    if let Some(v) = f.space_after_comma {
        into.space_after_comma = v;
    }
    if let Some(v) = f.space_around_type_operators {
        into.space_around_type_operators = v;
    }
    if let Some(v) = f.newline_before_else_catch_finally {
        into.newline_before_else_catch_finally = v;
    }
    if let Some(v) = f.trailing_newline {
        into.trailing_newline = v;
    }
    if let Some(v) = f.blank_lines_between_block_statements {
        into.blank_lines_between_block_statements = v.min(10);
    }
    if let Some(v) = f.blank_lines_between_class_members {
        into.blank_lines_between_class_members = v.min(10);
    }
    if let Some(v) = f.max_consecutive_blank_lines_in_block {
        into.max_consecutive_blank_lines_in_block = v.min(10);
    }
    if let Some(v) = f.line_ending.as_deref() {
        if let Some(le) = parse_line_ending(v) {
            into.line_ending = le;
        }
    }
    if let Some(v) = f.semicolon_style.as_deref() {
        if let Some(s) = SemicolonStyle::parse(v) {
            into.semicolon_style = s;
        }
    }
}

fn parse_brace_style(s: &str) -> Option<BraceStyle> {
    let v = s.trim().to_ascii_lowercase().replace('_', "-");
    match v.as_str() {
        "same-line" | "sameline" | "kr" | "k&r" => Some(BraceStyle::SameLine),
        "next-line" | "nextline" | "allman" => Some(BraceStyle::NextLine),
        _ => None,
    }
}

fn parse_line_ending(s: &str) -> Option<LineEnding> {
    let v = s.trim().to_ascii_lowercase();
    match v.as_str() {
        "lf" | "unix" | "\\n" => Some(LineEnding::Lf),
        "crlf" | "windows" | "\\r\\n" => Some(LineEnding::Crlf),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_format_options(
    base: FormatOptions,
    indent_width: Option<usize>,
    use_tabs: bool,
    tab_width: Option<usize>,
    line_width: Option<usize>,
    brace_style: Option<CliBraceStyle>,
    blank_lines_between_top_level: Option<usize>,
    blank_lines_after_class: Option<usize>,
    space_after_keyword_before_paren: Option<bool>,
    space_before_function_decl_paren: Option<bool>,
    space_inside_parens: Option<bool>,
    space_around_assign: Option<bool>,
    space_around_binary_ops: Option<bool>,
    space_after_comma: Option<bool>,
    space_around_type_operators: Option<bool>,
    newline_before_else_catch_finally: Option<bool>,
    trailing_newline: Option<bool>,
    blank_lines_between_block_statements: Option<usize>,
    blank_lines_between_class_members: Option<usize>,
    max_consecutive_blank_lines_in_block: Option<usize>,
    line_ending: Option<CliLineEnding>,
    semicolon_style: Option<CliSemicolonStyle>,
) -> FormatOptions {
    let mut o = base;

    if let Some(v) = indent_width {
        o.indent_width = v.clamp(1, 32);
    }
    if use_tabs {
        o.use_tabs = true;
    }
    if let Some(v) = tab_width {
        o.tab_width = v.clamp(1, 32);
    }
    if let Some(v) = line_width {
        o.line_width = if v == 0 { 0 } else { v.clamp(20, 500) };
    }
    if let Some(v) = brace_style {
        o.brace_style = v.into();
    }
    if let Some(v) = blank_lines_between_top_level {
        o.blank_lines_between_top_level = v.min(10);
    }
    if let Some(v) = blank_lines_after_class {
        o.blank_lines_after_class = v.min(10);
    }
    if let Some(v) = space_after_keyword_before_paren {
        o.space_after_keyword_before_paren = v;
    }
    if let Some(v) = space_before_function_decl_paren {
        o.space_before_function_decl_paren = v;
    }
    if let Some(v) = space_inside_parens {
        o.space_inside_parens = v;
    }
    if let Some(v) = space_around_assign {
        o.space_around_assign = v;
    }
    if let Some(v) = space_around_binary_ops {
        o.space_around_binary_ops = v;
    }
    if let Some(v) = space_after_comma {
        o.space_after_comma = v;
    }
    if let Some(v) = space_around_type_operators {
        o.space_around_type_operators = v;
    }
    if let Some(v) = newline_before_else_catch_finally {
        o.newline_before_else_catch_finally = v;
    }
    if let Some(v) = trailing_newline {
        o.trailing_newline = v;
    }
    if let Some(v) = blank_lines_between_block_statements {
        o.blank_lines_between_block_statements = v.min(10);
    }
    if let Some(v) = blank_lines_between_class_members {
        o.blank_lines_between_class_members = v.min(10);
    }
    if let Some(v) = max_consecutive_blank_lines_in_block {
        o.max_consecutive_blank_lines_in_block = v.min(10);
    }
    if let Some(v) = line_ending {
        o.line_ending = v.into();
    }
    if let Some(v) = semicolon_style {
        o.semicolon_style = v.into();
    }

    o
}

pub(crate) enum FormatDest {
    InPlace,
    Stdout,
    File(PathBuf),
}

pub(crate) fn cmd_format(
    lang: LanguageOptions,
    dest: FormatDest,
    files: &[PathBuf],
    opts: &FormatOptions,
    merge_includes: bool,
    merge_root: Option<&Path>,
) -> ExitCode {
    if merge_includes {
        if files.is_empty() {
            report::emit(report::format_usage(
                "format: --merge-includes requires a single entry file argument (not stdin)",
            ));
            return ExitCode::from(2);
        }
        if files.len() != 1 {
            report::emit(report::format_usage(
                "format: --merge-includes requires exactly one entry file",
            ));
            return ExitCode::from(2);
        }
        let entry_path = &files[0];
        if !entry_path.is_file() {
            report::emit(report::format_usage(
                "format: --merge-includes needs a single file path, not a directory",
            ));
            return ExitCode::from(2);
        }
        if matches!(dest, FormatDest::InPlace) {
            report::emit(report::format_usage(
                "format: --merge-includes requires --stdout or --out (merged output is not written back to individual files)",
            ));
            return ExitCode::from(2);
        }

        let entry = match std::fs::canonicalize(entry_path) {
            Ok(p) => p,
            Err(e) => {
                report::emit(report::io_path(entry_path, e));
                return ExitCode::from(1);
            }
        };
        let root = if let Some(r) = merge_root {
            match std::fs::canonicalize(r) {
                Ok(p) => p,
                Err(e) => {
                    report::emit(report::io_path(r, e));
                    return ExitCode::from(1);
                }
            }
        } else {
            infer_include_project_root(&entry)
        };

        let label = entry.display().to_string();
        let prep = match prepare_merged_check_unit(&root, &entry, lang, &[], None) {
            Ok(p) => p,
            Err(e) => {
                report::emit(report::merged_check_prep(
                    &root,
                    entry_path,
                    e,
                    report::MergedCheckPrepContext::CheckOrFormat,
                ));
                return ExitCode::from(1);
            }
        };

        let out = match format_document(&prep.combined, prep.resolved, opts) {
            Ok(o) => o,
            Err(e) => {
                report::emit(report::parse_diagnostic(
                    format!("{label} (merged)"),
                    &prep.combined,
                    &e,
                ));
                return ExitCode::from(1);
            }
        };
        match dest {
            FormatDest::Stdout => print!("{out}"),
            FormatDest::File(to) => {
                if let Err(e) = std::fs::write(&to, out.as_bytes()) {
                    report::emit(report::io_path(to, e));
                    return ExitCode::from(1);
                }
            }
            FormatDest::InPlace => {
                report::emit(report::format_usage(
                    "format: internal error: in-place dest after --merge-includes validation",
                ));
                return ExitCode::from(2);
            }
        }
        return ExitCode::SUCCESS;
    }

    if files.is_empty() {
        let mut src = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut src) {
            report::emit(report::stdin_io(e));
            return ExitCode::from(1);
        }
        return match format_document(&src, lang, opts) {
            Ok(out) => {
                match dest {
                    FormatDest::InPlace => {
                        report::emit(report::format_usage(
                            "format: cannot write in-place when reading stdin",
                        ));
                        return ExitCode::from(2);
                    }
                    FormatDest::Stdout => {
                        print!("{out}");
                    }
                    FormatDest::File(path) => {
                        if let Err(e) = std::fs::write(&path, out.as_bytes()) {
                            report::emit(report::io_path(&path, e));
                            return ExitCode::from(1);
                        }
                    }
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                report::emit(report::parse_diagnostic("<stdin>", &src, &e));
                ExitCode::from(1)
            }
        };
    }

    let files = match expand_format_inputs(files) {
        Ok(v) => v,
        Err(e) => {
            report::emit(report::format_usage(format!("format: {e}")));
            return ExitCode::from(2);
        }
    };

    match &dest {
        FormatDest::Stdout | FormatDest::File(_) if files.len() != 1 => {
            report::emit(report::format_usage(
                "format: --stdout/--out require exactly one input file (after directory expansion)",
            ));
            return ExitCode::from(2);
        }
        _ => {}
    }

    let mut ok = true;
    for path in files {
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                ok = false;
                report::emit(report::io_path(&path, e));
                continue;
            }
        };
        let out = match format_document(&src, lang, opts) {
            Ok(o) => o,
            Err(e) => {
                ok = false;
                report::emit(report::parse_diagnostic(
                    path.display().to_string(),
                    &src,
                    &e,
                ));
                continue;
            }
        };
        match &dest {
            FormatDest::InPlace => {
                if let Err(e) = std::fs::write(&path, out.as_bytes()) {
                    ok = false;
                    report::emit(report::io_path(&path, e));
                }
            }
            FormatDest::Stdout => {
                print!("{out}");
            }
            FormatDest::File(to) => {
                if let Err(e) = std::fs::write(to, out.as_bytes()) {
                    ok = false;
                    report::emit(report::io_path(to, e));
                }
            }
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn expand_format_inputs(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut out: Vec<PathBuf> = Vec::new();

    for input in inputs {
        if input.is_file() {
            out.push(input.clone());
            continue;
        }
        if input.is_dir() {
            out.extend(collect_leek_files_recursively(input)?);
            continue;
        }
        return Err(format!("{}: no such file or directory", input.display()));
    }

    // Stable ordering (useful for error output and deterministic runs).
    out.sort();
    out.dedup();

    if out.is_empty() {
        return Err("no input files found".to_string());
    }
    Ok(out)
}

fn collect_leek_files_recursively(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut q: VecDeque<PathBuf> = VecDeque::new();
    q.push_back(root.to_path_buf());

    while let Some(dir) = q.pop_front() {
        let rd = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in rd {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = entry.path();
            if path.is_dir() {
                q.push_back(path);
                continue;
            }
            if path.is_file() {
                if path.extension().is_some_and(|e| e == "leek") {
                    out.push(path);
                }
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_format_options_precedence_cli_over_config() {
        let mut base = FormatOptions::default();
        let cfg = TomlFormatOptions {
            indent_width: Some(2),
            space_around_binary_ops: Some(false),
            ..Default::default()
        };
        apply_toml_format_options(&mut base, &cfg);

        let out = build_format_options(
            base,
            Some(6),
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert_eq!(out.indent_width, 6);
        assert_eq!(out.space_around_binary_ops, true);
    }

    #[test]
    fn parses_config_with_format_table_and_root_keys() {
        let src = r#"
indent-width = 2
[format]
space-around-binary-ops = false
"#;
        let cfg: TomlConfig = toml::from_str(src).unwrap();
        let mut o = FormatOptions::default();
        if let Some(f) = cfg.format {
            apply_toml_format_options(&mut o, &f);
        }
        apply_toml_format_options(&mut o, &cfg.root_format);

        assert_eq!(o.indent_width, 2);
        assert_eq!(o.space_around_binary_ops, false);
    }

    #[test]
    fn parses_brace_style_and_line_ending_synonyms() {
        let src = r#"
[format]
brace-style = "allman"
line-ending = "windows"
"#;
        let cfg: TomlConfig = toml::from_str(src).unwrap();
        let mut o = FormatOptions::default();
        if let Some(f) = cfg.format {
            apply_toml_format_options(&mut o, &f);
        }
        assert_eq!(o.brace_style, BraceStyle::NextLine);
        assert_eq!(o.line_ending, LineEnding::Crlf);
    }

    #[test]
    fn parses_space_after_comma_and_block_blank_lines() {
        let src = r#"
[format]
space-after-comma = false
blank-lines-between-block-statements = 2
"#;
        let cfg: TomlConfig = toml::from_str(src).unwrap();
        let mut o = FormatOptions::default();
        if let Some(f) = cfg.format {
            apply_toml_format_options(&mut o, &f);
        }
        assert!(!o.space_after_comma);
        assert_eq!(o.blank_lines_between_block_statements, 2);
    }

    #[test]
    fn parses_blank_lines_after_class_toml() {
        let src = r#"
[format]
blank-lines-after-class = 0
"#;
        let cfg: TomlConfig = toml::from_str(src).unwrap();
        let mut o = FormatOptions::default();
        if let Some(f) = cfg.format {
            apply_toml_format_options(&mut o, &f);
        }
        assert_eq!(o.blank_lines_after_class, 0);
    }

    #[test]
    fn parses_space_around_type_operators_toml() {
        let src = r#"
[format]
space-around-type-operators = true
"#;
        let cfg: TomlConfig = toml::from_str(src).unwrap();
        let mut o = FormatOptions::default();
        if let Some(f) = cfg.format {
            apply_toml_format_options(&mut o, &f);
        }
        assert!(o.space_around_type_operators);
    }
}
