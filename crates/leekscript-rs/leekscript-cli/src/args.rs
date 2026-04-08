//! Clap definitions: `leekscript` flags, subcommands, and global [`LanguageOptions`].

use clap::{Parser, Subcommand, ValueEnum};
use leekscript::format::{BraceStyle, LineEnding, SemicolonStyle};
use leekscript::{ExperimentalFeatures, LanguageOptions, Version};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum Dialect {
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
pub(crate) struct Cli {
    /// Language dialect (v1–v4)
    #[arg(long, global = true, value_enum, default_value_t = Dialect::V4)]
    pub(crate) dialect: Dialect,

    /// Enable every experimental parse feature (`let`, `match`, modules, exceptions, `goto`, `break n`, templates, …).
    #[arg(long, global = true)]
    pub(crate) experimental: bool,

    /// Experimental: `let` bindings.
    #[arg(long = "experimental-let", global = true)]
    pub(crate) experimental_let: bool,
    /// Experimental: `const` declarations.
    #[arg(long = "experimental-const", global = true)]
    pub(crate) experimental_const: bool,
    /// Experimental: `match` statement.
    #[arg(long = "experimental-match", global = true)]
    pub(crate) experimental_match: bool,
    /// Experimental: `import` / `export` / `package`.
    #[arg(long = "experimental-modules", global = true)]
    pub(crate) experimental_modules: bool,
    /// Experimental: `try` / `catch` / `finally` / `throw`.
    #[arg(long = "experimental-exceptions", global = true)]
    pub(crate) experimental_exceptions: bool,
    /// Experimental: `goto`.
    #[arg(long = "experimental-goto", global = true)]
    pub(crate) experimental_goto: bool,
    /// Experimental: `break N` / `continue N` loop levels.
    #[arg(long = "experimental-loop-levels", global = true)]
    pub(crate) experimental_loop_levels: bool,
    /// Experimental: template parameters on classes and `function` declarations (`function f<T>(…)`, `class C<T>`, `function<T>(…) {}`; not arrow lambdas).
    #[arg(long = "experimental-templates", global = true)]
    pub(crate) experimental_templates: bool,

    #[command(subcommand)]
    pub(crate) command: Command,
}

pub(crate) fn language_options(cli: &Cli) -> LanguageOptions {
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
            templates: cli.experimental_templates,
        }
    };
    LanguageOptions::new(version, experimental)
}

#[derive(Subcommand)]
pub(crate) enum Command {
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
pub(crate) enum CliBraceStyle {
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
pub(crate) enum CliLineEnding {
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
pub(crate) enum CliSemicolonStyle {
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
