//! CLI for parsing, formatting, and merging LeekScript sources (`leekscript` binary).

mod report;

use std::collections::VecDeque;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use leekscript::format::{BraceStyle, FormatOptions, LineEnding, SemicolonStyle, format_document};
use leekscript::include::{
    MergedSourceMapping, load_project_with_includes,
    merge_included_sources_to_single_file, merge_included_sources_to_single_file_mapped,
    prepend_signatures_to_merged,
};
use leekscript::syntax::kinds::K;
use leekscript::{
    ExperimentalFeatures, LanguageOptions, SemanticSeverity, Version, is_signature_stub_path,
    language_options_with_source_directives, parse_doc, parse_signature_doc,
};
use serde::Deserialize;
use sipha::tree::tree_display::{TreeDisplayOptions, format_syntax_tree};
use sipha::types::FromSyntaxKind;
use sipha::types::SyntaxKind;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Dialect {
    V1,
    V2,
    V3,
    V4,
}

impl From<Dialect> for Version {
    fn from(d: Dialect) -> Self {
        match d {
            Dialect::V1 => Version::V1,
            Dialect::V2 => Version::V2,
            Dialect::V3 => Version::V3,
            Dialect::V4 => Version::V4,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "leekscript",
    version,
    about = "LeekScript parser, formatter, and include merger"
)]
struct Cli {
    /// Language dialect (v1–v4)
    #[arg(long, global = true, value_enum, default_value_t = Dialect::V4)]
    dialect: Dialect,

    /// Enable every experimental parse feature (`let`, `match`, modules, exceptions, `goto`, `break n`, function defaults, templates, …).
    #[arg(long, global = true)]
    experimental: bool,

    /// Experimental: `let` bindings.
    #[arg(long = "experimental-let", global = true)]
    experimental_let: bool,
    /// Experimental: `const` declarations.
    #[arg(long = "experimental-const", global = true)]
    experimental_const: bool,
    /// Experimental: `match` statement.
    #[arg(long = "experimental-match", global = true)]
    experimental_match: bool,
    /// Experimental: `import` / `export` / `package`.
    #[arg(long = "experimental-modules", global = true)]
    experimental_modules: bool,
    /// Experimental: `try` / `catch` / `finally` / `throw`.
    #[arg(long = "experimental-exceptions", global = true)]
    experimental_exceptions: bool,
    /// Experimental: `goto`.
    #[arg(long = "experimental-goto", global = true)]
    experimental_goto: bool,
    /// Experimental: `break N` / `continue N` loop levels.
    #[arg(long = "experimental-loop-levels", global = true)]
    experimental_loop_levels: bool,
    /// Experimental: default values on top-level / anonymous `function (a = 1)` parameters (methods always allow `=`).
    #[arg(long = "experimental-fn-optional-params", global = true)]
    experimental_fn_optional_params: bool,
    /// Experimental: template parameters on classes and `function` declarations (`function f<T>(…)`, `class C<T>`, `function<T>(…) {}`; not arrow lambdas).
    #[arg(long = "experimental-templates", global = true)]
    experimental_templates: bool,

    #[command(subcommand)]
    command: Command,
}

fn language_options(cli: &Cli) -> LanguageOptions {
    let version = Version::from(cli.dialect);
    let experimental = if cli.experimental {
        ExperimentalFeatures::ALL
    } else {
        ExperimentalFeatures {
            let_bindings: cli.experimental_let,
            lexical_const: cli.experimental_const,
            match_stmt: cli.experimental_match,
            modules: cli.experimental_modules,
            exceptions: cli.experimental_exceptions,
            goto: cli.experimental_goto,
            loop_levels: cli.experimental_loop_levels,
            fn_optional_params: cli.experimental_fn_optional_params,
            templates: cli.experimental_templates,
        }
    };
    LanguageOptions::new(version, experimental)
}

#[derive(Subcommand)]
enum Command {
    /// Parse inputs (with `include("…")` expanded like `merge`), then run scope/type analysis unless `--parse-only`
    Check {
        /// Only run the parser (skip scope / type checks after a successful parse)
        #[arg(long)]
        parse_only: bool,

        /// Project root for resolving `include("…")` (default: parent directory of each entry file)
        #[arg(long, value_name = "DIR")]
        root: Option<PathBuf>,

        /// Prepend signature / stub `.leek` files (stdlib declarations) before the check unit; repeatable, order matters.
        /// Top-level `include("…")` in each file is expanded like `merge` (project root = that file’s directory).
        #[arg(long = "signatures", value_name = "FILE")]
        signature_files: Vec<PathBuf>,

        /// Files to check, or omit to read stdin once
        #[arg(value_name = "FILE")]
        files: Vec<PathBuf>,
    },
    /// Pretty-print LeekScript (writes in-place by default when given paths)
    Format {
        /// Expand top-level `include("…")` like `merge`, then format one buffer (requires `--stdout` or `--out`)
        #[arg(long)]
        merge_includes: bool,

        /// Project root for resolving includes when using `--merge-includes` (default: parent of the entry file)
        #[arg(long, value_name = "DIR", requires = "merge_includes")]
        root: Option<PathBuf>,

        /// Print formatted output to stdout (requires exactly one input file after directory expansion)
        #[arg(long, conflicts_with = "out")]
        stdout: bool,

        /// Write formatted output to this file (requires exactly one input file after directory expansion)
        #[arg(long, value_name = "FILE", conflicts_with = "stdout")]
        out: Option<PathBuf>,

        /// TOML config file with formatting options
        #[arg(long, value_name = "FILE")]
        config: Option<PathBuf>,

        /// Indent width in spaces (ignored when `--use-tabs`)
        #[arg(long, value_name = "N")]
        indent_width: Option<usize>,
        /// Use tab characters for indentation
        #[arg(long)]
        use_tabs: bool,
        /// Logical tab width (display width for tabs; used when wrapping)
        #[arg(long, value_name = "N")]
        tab_width: Option<usize>,
        /// Max line length (`0` = no comma wrapping)
        #[arg(long, value_name = "N")]
        line_width: Option<usize>,
        /// Brace placement style
        #[arg(long, value_enum, value_name = "STYLE")]
        brace_style: Option<CliBraceStyle>,
        /// Blank lines between top-level statements (`0` = single newline only)
        #[arg(long, value_name = "N")]
        blank_lines_between_top_level: Option<usize>,
        /// Extra blank lines after a top-level class before the next item (`0` = none)
        #[arg(long, value_name = "N")]
        blank_lines_after_class: Option<usize>,

        /// Insert space after keywords before `(` (e.g. `if (` vs `if(`)
        #[arg(long, value_name = "BOOL")]
        space_after_keyword_before_paren: Option<bool>,
        /// Insert space before `(` in function declarations (e.g. `function f (` vs `function f(`)
        #[arg(long, value_name = "BOOL")]
        space_before_function_decl_paren: Option<bool>,
        /// Insert spaces inside parentheses (e.g. `( x )` vs `(x)`)
        #[arg(long, value_name = "BOOL")]
        space_inside_parens: Option<bool>,
        /// Insert spaces around assignment `=` (e.g. `x = 1` vs `x=1`)
        #[arg(long, value_name = "BOOL")]
        space_around_assign: Option<bool>,
        /// Insert spaces around binary operators (e.g. `a + b` vs `a+b`)
        #[arg(long, value_name = "BOOL")]
        space_around_binary_ops: Option<bool>,
        /// Insert spaces after commas in lists (e.g. `a, b` vs `a,b`)
        #[arg(long, value_name = "BOOL")]
        space_after_comma: Option<bool>,
        /// Spaces around `|`, `<`, `>` in types (e.g. `integer | real`; omit for `integer|real`)
        #[arg(long, value_name = "BOOL")]
        space_around_type_operators: Option<bool>,
        /// Put `else`/`catch`/`finally` on a new line after `}`
        #[arg(long, value_name = "BOOL")]
        newline_before_else_catch_finally: Option<bool>,

        /// Force a trailing newline at end of output
        #[arg(long, value_name = "BOOL")]
        trailing_newline: Option<bool>,
        /// Extra blank lines between statements inside `{ ... }` (`0` = single newline only)
        #[arg(long, value_name = "N")]
        blank_lines_between_block_statements: Option<usize>,
        /// Extra blank lines between class members in the class body (`0` = single newline only)
        #[arg(long, value_name = "N")]
        blank_lines_between_class_members: Option<usize>,
        /// Caps blank lines between block statements and class members when non-zero (`0` = no cap)
        #[arg(long, value_name = "N")]
        max_consecutive_blank_lines_in_block: Option<usize>,
        /// Line ending for inserted breaks
        #[arg(long, value_enum, value_name = "ENDING")]
        line_ending: Option<CliLineEnding>,
        /// Optional statement semicolons: preserve (default), always, or only-needed (`return;`, `break;`, …)
        #[arg(long, value_enum, value_name = "STYLE")]
        semicolon_style: Option<CliSemicolonStyle>,

        /// Files to format, or omit to read stdin once
        #[arg(value_name = "FILE")]
        files: Vec<PathBuf>,
    },
    /// Load an entry file and all includes, then emit one merged source (metadata comments preserved)
    Merge {
        /// Project root directory (used for include resolution, same as the library loader)
        #[arg(long, default_value = ".", value_name = "DIR")]
        root: PathBuf,
        /// Entry `.leek` file (relative to `--root` or absolute)
        entry: PathBuf,
    },
    /// Parse inputs and print the syntax tree
    Tree {
        /// Include trivia tokens (whitespace/comments) in the tree
        #[arg(long)]
        trivia: bool,
        /// Files to parse, or omit to read stdin once
        #[arg(value_name = "FILE")]
        files: Vec<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum CliBraceStyle {
    SameLine,
    NextLine,
}

impl From<CliBraceStyle> for BraceStyle {
    fn from(v: CliBraceStyle) -> Self {
        match v {
            CliBraceStyle::SameLine => BraceStyle::SameLine,
            CliBraceStyle::NextLine => BraceStyle::NextLine,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum CliLineEnding {
    Lf,
    Crlf,
}

impl From<CliLineEnding> for LineEnding {
    fn from(v: CliLineEnding) -> Self {
        match v {
            CliLineEnding::Lf => LineEnding::Lf,
            CliLineEnding::Crlf => LineEnding::Crlf,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum CliSemicolonStyle {
    Preserve,
    Always,
    OnlyNeeded,
}

impl From<CliSemicolonStyle> for SemicolonStyle {
    fn from(v: CliSemicolonStyle) -> Self {
        match v {
            CliSemicolonStyle::Preserve => SemicolonStyle::Preserve,
            CliSemicolonStyle::Always => SemicolonStyle::Always,
            CliSemicolonStyle::OnlyNeeded => SemicolonStyle::OnlyNeeded,
        }
    }
}

fn main() -> ExitCode {
    report::install_hook();
    let cli = Cli::parse();
    let lang = language_options(&cli);

    match cli.command {
        Command::Check {
            parse_only,
            root,
            signature_files,
            files,
        } => cmd_check(
            lang,
            parse_only,
            root.as_deref(),
            &signature_files,
            &files,
        ),
        Command::Format {
            merge_includes,
            root: format_root,
            stdout,
            out,
            config,
            indent_width,
            use_tabs,
            tab_width,
            line_width,
            brace_style,
            blank_lines_between_top_level,
            blank_lines_after_class,
            space_after_keyword_before_paren,
            space_before_function_decl_paren,
            space_inside_parens,
            space_around_assign,
            space_around_binary_ops,
            space_after_comma,
            space_around_type_operators,
            newline_before_else_catch_finally,
            trailing_newline,
            blank_lines_between_block_statements,
            blank_lines_between_class_members,
            max_consecutive_blank_lines_in_block,
            line_ending,
            semicolon_style,
            files,
        } => {
            let base_from_config = match config {
                Some(path) => match load_format_config(&path) {
                    Ok(o) => o,
                    Err(e) => {
                        report::emit(report::format_config(&path, e));
                        return ExitCode::from(2);
                    }
                },
                None => FormatOptions::default(),
            };

            let opts = build_format_options(
                base_from_config,
                indent_width,
                use_tabs,
                tab_width,
                line_width,
                brace_style,
                blank_lines_between_top_level,
                blank_lines_after_class,
                space_after_keyword_before_paren,
                space_before_function_decl_paren,
                space_inside_parens,
                space_around_assign,
                space_around_binary_ops,
                space_after_comma,
                space_around_type_operators,
                newline_before_else_catch_finally,
                trailing_newline,
                blank_lines_between_block_statements,
                blank_lines_between_class_members,
                max_consecutive_blank_lines_in_block,
                line_ending,
                semicolon_style,
            );
            let dest = if stdout {
                FormatDest::Stdout
            } else if let Some(p) = out {
                FormatDest::File(p)
            } else {
                FormatDest::InPlace
            };
            cmd_format(
                lang,
                dest,
                &files,
                &opts,
                merge_includes,
                format_root.as_deref(),
            )
        }
        Command::Merge { root, entry } => cmd_merge(lang, &root, &entry),
        Command::Tree { trivia, files } => cmd_tree(lang, trivia, &files),
    }
}

fn kind_to_name(k: SyntaxKind) -> Option<&'static str> {
    K::from_syntax_kind(k).map(K::as_str)
}

fn cmd_check(
    lang: LanguageOptions,
    parse_only: bool,
    root_override: Option<&Path>,
    signature_files: &[PathBuf],
    files: &[PathBuf],
) -> ExitCode {
    let mut ok = true;

    if files.is_empty() {
        let mut src = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut src) {
            report::emit(report::stdin_io(e));
            return ExitCode::from(1);
        }
        let label = "<stdin>";

        if !signature_files.is_empty() {
            let (combined, map) = match prepend_signatures_to_merged(
                lang,
                signature_files,
                &src,
                MergedSourceMapping::default(),
            ) {
                Ok(x) => x,
                Err(e) => {
                    report::emit(report::prelude_signatures(e));
                    return ExitCode::from(1);
                }
            };
            let user_base = (combined.len() - src.len()) as u32;
            match parse_signature_doc(&combined, lang) {
                Ok(doc) => {
                    if !parse_only {
                        let resolved = language_options_with_source_directives(&combined, lang);
                        let analysis =
                            leekscript::run_semantic_analysis(doc.root(), resolved.version);
                        for d in &analysis.diagnostics {
                            if d.severity == SemanticSeverity::Error {
                                ok = false;
                            }
                            report::emit(report::check_diagnostic(
                                &map,
                                None,
                                &combined,
                                label,
                                d.span,
                                &d.message,
                                d.related_span,
                                Some((&src, user_base)),
                                d.severity,
                            ));
                        }
                    }
                }
                Err(e) => {
                    ok = false;
                    report::emit(report::parse_diagnostic(label, &combined, &e));
                }
            }
            return if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            };
        }

        match parse_doc(&src, lang) {
            Ok(doc) => {
                if !parse_only {
                    let resolved = language_options_with_source_directives(&src, lang);
                    let analysis =
                        leekscript::run_semantic_analysis(doc.root(), resolved.version);
                    for d in &analysis.diagnostics {
                        if d.severity == SemanticSeverity::Error {
                            ok = false;
                        }
                        report::emit(report::check_diagnostic(
                            &MergedSourceMapping::default(),
                            None,
                            &src,
                            label,
                            d.span,
                            &d.message,
                            d.related_span,
                            None,
                            d.severity,
                        ));
                    }
                }
            }
            Err(e) => {
                ok = false;
                report::emit(report::parse_diagnostic(label, &src, &e));
            }
        }
        return if ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }

    for path in files {
        let label = path.display().to_string();
        let entry = match std::fs::canonicalize(path) {
            Ok(p) => p,
            Err(e) => {
                ok = false;
                report::emit(report::io_path(path, e));
                continue;
            }
        };
        let root = if let Some(r) = root_override {
            match std::fs::canonicalize(r) {
                Ok(p) => p,
                Err(e) => {
                    ok = false;
                    report::emit(report::io_path(r, e));
                    continue;
                }
            }
        } else {
            entry
                .parent()
                .unwrap_or_else(|| Path::new("/"))
                .to_path_buf()
        };

        let project = match load_project_with_includes(&root, &entry, lang) {
            Ok(p) => p,
            Err(e) => {
                ok = false;
                report::emit(report::include_load(&root, path, &e));
                continue;
            }
        };

        let (merged, map) = match merge_included_sources_to_single_file_mapped(&root, &project) {
            Ok(x) => x,
            Err(e) => {
                ok = false;
                report::emit(report::merge_includes(path, e));
                continue;
            }
        };

        let (combined, full_map) = match prepend_signatures_to_merged(
            lang,
            signature_files,
            &merged,
            map,
        ) {
            Ok(x) => x,
            Err(e) => {
                ok = false;
                report::emit(report::signatures_for_entry(path, e));
                continue;
            }
        };

        let use_sig_grammar = !signature_files.is_empty() || is_signature_stub_path(&entry);
        let parse_result = if use_sig_grammar {
            parse_signature_doc(&combined, lang)
        } else {
            parse_doc(&combined, lang)
        };
        match parse_result {
            Ok(doc) => {
                if !parse_only {
                    let resolved = language_options_with_source_directives(&combined, lang);
                    let analysis =
                        leekscript::run_semantic_analysis(doc.root(), resolved.version);
                    for d in &analysis.diagnostics {
                        if d.severity == SemanticSeverity::Error {
                            ok = false;
                        }
                        report::emit(report::check_diagnostic(
                            &full_map,
                            Some(&project),
                            &combined,
                            &label,
                            d.span,
                            &d.message,
                            d.related_span,
                            None,
                            d.severity,
                        ));
                    }
                }
            }
            Err(e) => {
                ok = false;
                report::emit(report::parse_diagnostic(
                    format!("{label} (merged)"),
                    &combined,
                    &e,
                ));
            }
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

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

fn load_format_config(path: &Path) -> Result<FormatOptions, String> {
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
fn build_format_options(
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

enum FormatDest {
    InPlace,
    Stdout,
    File(PathBuf),
}

fn cmd_format(
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
            entry
                .parent()
                .unwrap_or_else(|| Path::new("/"))
                .to_path_buf()
        };

        let label = entry.display().to_string();
        let project = match load_project_with_includes(&root, &entry, lang) {
            Ok(p) => p,
            Err(e) => {
                report::emit(report::include_load(&root, entry_path, &e));
                return ExitCode::from(1);
            }
        };
        let merged = match merge_included_sources_to_single_file(&root, &project) {
            Ok(s) => s,
            Err(e) => {
                report::emit(report::merge_includes(entry_path, e));
                return ExitCode::from(1);
            }
        };

        let out = match format_document(&merged, lang, opts) {
            Ok(o) => o,
            Err(e) => {
                report::emit(report::parse_diagnostic(
                    format!("{label} (merged)"),
                    &merged,
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

fn cmd_tree(lang: LanguageOptions, trivia: bool, files: &[PathBuf]) -> ExitCode {
    let opts = TreeDisplayOptions {
        show_trivia: trivia,
        ..Default::default()
    };

    if files.is_empty() {
        let mut src = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut src) {
            report::emit(report::stdin_io(e));
            return ExitCode::from(1);
        }
        let doc = match parse_doc(&src, lang) {
            Ok(d) => d,
            Err(e) => {
                report::emit(report::parse_diagnostic("<stdin>", &src, &e));
                return ExitCode::from(1);
            }
        };
        let out = format_syntax_tree(doc.root(), &opts, |k| {
            kind_to_name(k).unwrap_or("<?>").to_string()
        });
        print!("{out}");
        return ExitCode::SUCCESS;
    }

    let mut ok = true;
    for path in files {
        let label = path.display().to_string();
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                ok = false;
                report::emit(report::io_path(path, e));
                continue;
            }
        };
        let parse_result = if is_signature_stub_path(path) {
            parse_signature_doc(&src, lang)
        } else {
            parse_doc(&src, lang)
        };
        let doc = match parse_result {
            Ok(d) => d,
            Err(e) => {
                ok = false;
                report::emit(report::parse_diagnostic(&label, &src, &e));
                continue;
            }
        };
        if files.len() > 1 {
            println!("== {} ==", path.display());
        }
        let out = format_syntax_tree(doc.root(), &opts, |k| {
            kind_to_name(k).unwrap_or("<?>").to_string()
        });
        print!("{out}");
        if files.len() > 1 {
            println!();
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn cmd_merge(lang: LanguageOptions, root: &Path, entry: &Path) -> ExitCode {
    let project = match load_project_with_includes(root, entry, lang) {
        Ok(p) => p,
        Err(e) => {
            report::emit(report::include_load(root, entry, &e));
            return ExitCode::from(1);
        }
    };
    match merge_included_sources_to_single_file(root, &project) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            report::emit(report::merge_command(e));
            ExitCode::from(1)
        }
    }
}
