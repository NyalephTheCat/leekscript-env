//! `lek` — LeekScript toolchain CLI (see [clap](https://docs.rs/clap/latest/clap/) for argument parsing).

use clap::builder::styling::{AnsiColor, Effects};
use clap::{Args, ColorChoice, Parser, Subcommand, ValueEnum};
use lek::check::{
    check_one_file, default_registry_path, expand_check_targets, read_source, CheckOptions,
    CheckTarget, CheckedFile, CheckedOk, DiagnosticRecord,
};
use lek::reporter::{emit_diagnostic, emit_fmt_error, emit_message, install_hook};
use lek::run::{run_compile, RunMessageFormat};
use lek::{format_one_file, FmtOptions, FmtPreamble};
use serde::Serialize;
use std::path::{Path, PathBuf};

const ABOUT: &str =
    "LeekScript toolchain — validate manifests, the diagnostic registry, and .leek sources.";
const LONG_ABOUT: &str = "\
Commands: `init` scaffolds a project, `config` inspects Leek.toml, `registry` validates diagnostic \
codes, `check` and `run` share the same compile pipeline (directives → lexer → parse → HIR); `run` also executes \
the lowered HIR with a built-in interpreter (subset of the language). `fmt` reformats sources (token-based; full parser later). \
Optional signature TOML (`[signatures]` in Leek.toml and/or `--signatures`) pre-declares globals/functions for resolve (e.g. Leek Wars AI).";

#[derive(Parser)]
#[command(
    name = "lek",
    author,
    version,
    about = ABOUT,
    long_about = LONG_ABOUT,
    propagate_version = true,
    arg_required_else_help = true,
    color = ColorChoice::Auto,
    styles = styles(),
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Help headings (clap 4 styles API).
fn styles() -> clap::builder::Styles {
    clap::builder::Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Cyan.on_default())
        .placeholder(AnsiColor::Cyan.on_default())
}

#[derive(Subcommand)]
enum Command {
    /// Validate the diagnostic registry YAML (`E####` codes)
    ///
    /// Loads `data/diagnostics/registry.yaml` by default, or the file set by `LEEK_REGISTRY`.
    /// Use `--verify-emit-refs` to ensure every HIR / run interpreter reference id is registered.
    Registry {
        #[command(flatten)]
        args: RegistryArgs,
    },
    /// Validate and print a `Leek.toml` manifest
    ///
    /// Walks upward from the current directory to find `Leek.toml` unless `--path` is set.
    Config {
        #[command(flatten)]
        args: ConfigArgs,
    },
    /// Create `Leek.toml` and an optional starter `.leek` file in a directory
    ///
    /// Creates the directory if needed. Refuses to overwrite `Leek.toml` unless `--force` is set.
    Init {
        #[command(flatten)]
        args: InitArgs,
    },
    /// Parse `.leek` files through the full grammar and lower to HIR (same pipeline as `lek run`)
    ///
    /// Resolves `// leek-*` preamble directives, then lexer, delimiter + grammar parse, and HIR lowering.
    /// Directories are expanded to all `*.leek` files beneath them (hidden dirs like `.git` skipped).
    /// Use a path of `-` to read source from standard input (see `--stdin-path` for LSP-style labels).
    Check {
        #[command(flatten)]
        args: CheckArgs,
    },
    /// Same compile pipeline as `lek check`, then runs the HIR with the built-in interpreter.
    ///
    /// Same discovery and `Leek.toml` / CLI precedence as `lek check`.
    Run {
        #[command(flatten)]
        args: RunArgs,
    },
    /// Reformat `.leek` sources (uses `[fmt]` and `// leek-fmt:` like other tools)
    ///
    /// Writes to stdout for a single file or stdin; use `--write` for in-place updates. With `--check`,
    /// exits with status 1 if any file would change.
    Fmt {
        #[command(flatten)]
        args: FmtArgs,
    },
}

#[derive(Args)]
struct RegistryArgs {
    /// Override path to `registry.yaml` (default: repo `data/diagnostics/registry.yaml`)
    #[arg(
        long,
        value_name = "FILE",
        help = "Registry YAML file (overrides LEEK_REGISTRY)",
        long_help = "Path to the diagnostic registry. If omitted, uses `LEEK_REGISTRY` when set, else the bundled `data/diagnostics/registry.yaml` next to the workspace.",
        value_hint = clap::ValueHint::FilePath,
    )]
    path: Option<PathBuf>,

    /// After loading, verify every HIR / interpreter reference id used by `lek` exists in the registry
    #[arg(
        long,
        help = "Fail if the registry lacks any reference emitted by check/run"
    )]
    verify_emit_refs: bool,
}

#[derive(Args)]
struct ConfigArgs {
    /// Explicit `Leek.toml` path (skips upward search)
    #[arg(
        long,
        value_name = "FILE",
        help = "Path to Leek.toml",
        long_help = "If not set, searches upward from the current working directory for `Leek.toml`.",
        value_hint = clap::ValueHint::FilePath,
    )]
    path: Option<PathBuf>,
}

#[derive(Args)]
struct InitArgs {
    /// Project root (created if it does not exist). Defaults to the current directory.
    #[arg(value_name = "DIR", value_hint = clap::ValueHint::DirPath)]
    path: Option<PathBuf>,

    /// Overwrite an existing `Leek.toml`
    #[arg(short = 'f', long, help = "Replace Leek.toml if it already exists")]
    force: bool,

    /// Skip creating `example.leek`
    #[arg(long, help = "Do not write a starter example.leek")]
    no_example: bool,
}

#[derive(Args)]
#[command(next_line_help = true)]
struct CheckArgs {
    /// Project manifest used for default `language.version` / `language.strict`
    #[arg(
        long,
        value_name = "FILE",
        help = "Leek.toml for defaults (else search upward from cwd)",
        long_help = "Explicit manifest path. When omitted, the effective language version and strictness fall back to `Leek.toml` discovered from the current directory, then toolchain defaults.",
        value_hint = clap::ValueHint::FilePath,
    )]
    manifest: Option<PathBuf>,

    /// Lexer language mode (1–99)
    #[arg(
        long,
        value_name = "VER",
        value_parser = parse_cli_language_version,
        help = "Override language version for lexing",
        long_help = "Precedence: this flag, then `// leek-version` in the file, then `[language].version` in Leek.toml, then 4.",
    )]
    language_version: Option<u8>,

    /// Enable strict mode for future lint passes
    #[arg(
        long,
        action = clap::ArgAction::SetTrue,
        conflicts_with = "no_strict",
        help = "Turn strict mode on (CLI overrides file and manifest)",
    )]
    strict: bool,

    /// Disable strict mode
    #[arg(
        long,
        action = clap::ArgAction::SetTrue,
        conflicts_with = "strict",
        help = "Turn strict mode off (CLI overrides file and manifest)",
    )]
    no_strict: bool,

    #[arg(
        long,
        value_name = "FILE",
        action = clap::ArgAction::Append,
        help = "Signature TOML (declare Leek Wars natives, etc.); merges with manifest [signatures]",
        value_hint = clap::ValueHint::FilePath,
    )]
    signatures: Vec<PathBuf>,

    /// Diagnostic output format
    #[arg(
        long,
        value_enum,
        default_value_t = MessageFormat::Human,
        help = "Human stderr lines vs JSON on stdout",
    )]
    message_format: MessageFormat,

    /// Virtual file path for stdin diagnostics
    #[arg(
        long,
        value_name = "PATH",
        help = "Display path when using `-` as input (LSP / editors)",
        long_help = "When `-` appears in FILES, diagnostics and JSON use this path instead of `-`. Ignored if `-` is not present (error at runtime).",
        value_hint = clap::ValueHint::AnyPath,
    )]
    stdin_path: Option<PathBuf>,

    /// Sources to check: files, directories of `*.leek`, or `-` for stdin
    #[arg(
        value_name = "PATH",
        required = true,
        num_args = 1..,
        help = "One or more .leek files, directories, and/or `-` for stdin",
        long_help = "Each argument may be: a `.leek` file, a directory (all `*.leek` files recursively, excluding hidden folders), or `-` to read that check from standard input. Order is preserved.",
        value_hint = clap::ValueHint::AnyPath,
    )]
    files: Vec<PathBuf>,
}

#[derive(Args)]
#[command(next_line_help = true)]
struct RunArgs {
    #[arg(
        long,
        value_name = "FILE",
        help = "Leek.toml for defaults (else search upward from cwd)",
        long_help = "Explicit manifest path. When omitted, the effective language version and strictness fall back to `Leek.toml` discovered from the current directory, then toolchain defaults.",
        value_hint = clap::ValueHint::FilePath,
    )]
    manifest: Option<PathBuf>,

    #[arg(
        long,
        value_name = "VER",
        value_parser = parse_cli_language_version,
        help = "Override language version (same precedence as `lek check`)",
    )]
    language_version: Option<u8>,

    #[arg(
        long,
        action = clap::ArgAction::SetTrue,
        conflicts_with = "no_strict",
        help = "Turn strict mode on (CLI overrides file and manifest)",
    )]
    strict: bool,

    #[arg(
        long,
        action = clap::ArgAction::SetTrue,
        conflicts_with = "strict",
        help = "Turn strict mode off",
    )]
    no_strict: bool,

    #[arg(
        long,
        value_name = "FILE",
        action = clap::ArgAction::Append,
        help = "Signature TOML (same as `lek check --signatures`)",
        value_hint = clap::ValueHint::FilePath,
    )]
    signatures: Vec<PathBuf>,

    #[arg(
        long,
        value_enum,
        default_value_t = MessageFormat::Human,
        help = "Human stderr lines vs JSON on stdout",
    )]
    message_format: MessageFormat,

    #[arg(
        long,
        value_name = "PATH",
        help = "Display path when using `-` as input (LSP / editors)",
        long_help = "When `-` appears in FILES, diagnostics and JSON use this path instead of `-`. Ignored if `-` is not present (error at runtime).",
        value_hint = clap::ValueHint::AnyPath,
    )]
    stdin_path: Option<PathBuf>,

    #[arg(
        value_name = "PATH",
        required = true,
        num_args = 1..,
        help = "One or more .leek files, directories, and/or `-` for stdin",
        long_help = "Same as `lek check`: files, recursive directories of `*.leek`, or `-` for standard input.",
        value_hint = clap::ValueHint::AnyPath,
    )]
    files: Vec<PathBuf>,
}

#[derive(Args)]
#[command(next_line_help = true)]
struct FmtArgs {
    #[arg(
        long,
        value_name = "FILE",
        help = "Leek.toml for default `[fmt]` (else search upward from cwd)",
        long_help = "Explicit manifest path. When omitted, `[fmt]` defaults come from `Leek.toml` discovered from the current directory, then built-in defaults.",
        value_hint = clap::ValueHint::FilePath,
    )]
    manifest: Option<PathBuf>,

    #[arg(
        long,
        value_name = "VER",
        value_parser = parse_cli_language_version,
        help = "Override language version for lexing (same precedence as `lek check`)",
    )]
    language_version: Option<u8>,

    #[arg(
        long,
        value_name = "COLS",
        value_parser = clap::value_parser!(u32).range(1..=500),
        help = "Override formatted line width hint (default: manifest / built-in)",
    )]
    width: Option<u32>,

    #[arg(
        long,
        value_name = "N",
        value_parser = clap::value_parser!(u32).range(1..=32),
        help = "Override indent width in spaces (default: manifest / built-in)",
    )]
    indent: Option<u32>,

    #[arg(
        short = 'w',
        long,
        conflicts_with = "check",
        help = "Write formatted output in place (files only, not stdin)"
    )]
    write: bool,

    #[arg(
        long,
        conflicts_with = "write",
        help = "Exit with status 1 if any file is not already formatted"
    )]
    check: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "Virtual path label when using `-` as input",
        value_hint = clap::ValueHint::AnyPath,
    )]
    stdin_path: Option<PathBuf>,

    #[arg(
        value_name = "PATH",
        required = true,
        num_args = 1..,
        help = ".leek files, directories of `*.leek`, or `-` for stdin",
        long_help = "Each argument may be a `.leek` file, a directory (recursive `*.leek`), or `-` for standard input.",
        value_hint = clap::ValueHint::AnyPath,
    )]
    files: Vec<PathBuf>,
}

#[derive(Clone, Copy, Default, ValueEnum)]
enum MessageFormat {
    /// Line-oriented messages on stderr (default)
    #[default]
    #[value(alias = "text")]
    Human,
    /// Single JSON object on stdout (`schema_version`, `files`, `diagnostics`)
    Json,
}

#[derive(Serialize)]
struct JsonDiagnostic {
    file: String,
    line: u32,
    column: u32,
    code: String,
    reference: String,
    message: String,
    phase: &'static str,
}

/// One input path in `--message-format json` (ok / error / io_error).
#[derive(Serialize)]
struct JsonFileLine {
    file: String,
    /// `ok` — full compile clean; `error` — diagnostics present; `io_error` — could not read file.
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    language_version: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hir_stmt_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fmt: Option<FmtPreamble>,
    #[serde(skip_serializing_if = "Option::is_none")]
    experimental: Option<Vec<String>>,
}

impl JsonFileLine {
    fn ok(ok: CheckedOk) -> Self {
        Self {
            file: ok.path_display,
            status: "ok",
            language_version: Some(ok.language_version),
            strict: ok.strict,
            token_count: Some(ok.token_count),
            hir_stmt_count: Some(ok.hir_stmt_count),
            fmt: ok.fmt,
            experimental: ok.experimental,
        }
    }

    fn error(path: &std::path::Path) -> Self {
        Self {
            file: path.display().to_string(),
            status: "error",
            language_version: None,
            strict: None,
            token_count: None,
            hir_stmt_count: None,
            fmt: None,
            experimental: None,
        }
    }

    fn io_error(path: &std::path::Path) -> Self {
        Self {
            file: path.display().to_string(),
            status: "io_error",
            language_version: None,
            strict: None,
            token_count: None,
            hir_stmt_count: None,
            fmt: None,
            experimental: None,
        }
    }
}

fn manifest_path_for_cli(explicit: Option<PathBuf>) -> Option<PathBuf> {
    explicit.or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(leekscript_config::find_manifest)
    })
}

fn parse_cli_language_version(s: &str) -> Result<u8, String> {
    let n: u8 = s
        .parse()
        .map_err(|_| format!("expected an integer 1–99, got `{s}`"))?;
    if !(1..=99).contains(&n) {
        return Err(format!(
            "language version must be between 1 and 99, got {n}"
        ));
    }
    Ok(n)
}

fn package_name_from_dir(path: &Path) -> String {
    let base = path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty() && *s != ".");
    let raw = base.unwrap_or("my-leek-project");
    let mut out = String::new();
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if c.is_ascii_whitespace() {
            if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
        } else {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "my-leek-project".into()
    } else {
        out
    }
}

fn run_init(args: InitArgs) -> std::process::ExitCode {
    let root = args.path.unwrap_or_else(|| PathBuf::from("."));
    if let Err(e) = std::fs::create_dir_all(&root) {
        eprintln!("lek init: could not create {}: {e}", root.display());
        return std::process::ExitCode::from(1);
    }
    let manifest_path = root.join("Leek.toml");
    if manifest_path.exists() && !args.force {
        eprintln!(
            "lek init: {} already exists (use --force to overwrite)",
            manifest_path.display()
        );
        return std::process::ExitCode::from(1);
    }
    let pkg = package_name_from_dir(&root);
    let toml = format!(
        r#"schema_version = 1

[package]
name = "{pkg}"
version = "0.1.0"

[language]
version = 4
strict = false

[fmt]
width = 100
indent = 4

[lint]
level = "warn"

[experimental]
features = []
"#
    );
    match leekscript_config::LeekManifest::from_str(&toml) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("lek init: internal error, invalid template: {e}");
            return std::process::ExitCode::from(1);
        }
    }
    if let Err(e) = std::fs::write(&manifest_path, &toml) {
        eprintln!("lek init: could not write {}: {e}", manifest_path.display());
        return std::process::ExitCode::from(1);
    }
    eprintln!("OK: wrote {}", manifest_path.display());
    if !args.no_example {
        let example = root.join("example.leek");
        if !example.exists() {
            let sample = "// Example LeekScript source\nvar x = 0;\n";
            if let Err(e) = std::fs::write(&example, sample) {
                eprintln!("lek init: could not write {}: {e}", example.display());
                return std::process::ExitCode::from(1);
            }
            eprintln!("     {}", example.display());
        }
    }
    std::process::ExitCode::SUCCESS
}

fn main() -> std::process::ExitCode {
    install_hook();
    let cli = Cli::parse();
    match cli.command {
        Command::Registry { args } => {
            let p = args.path.unwrap_or_else(default_registry_path);
            match leekscript_diagnostics::Registry::load_path(&p) {
                Ok(reg) => {
                    eprintln!(
                        "OK: loaded registry from {}\n  schema_version={} entries={}",
                        p.display(),
                        reg.schema_version,
                        reg.len()
                    );
                    if args.verify_emit_refs {
                        match lek::toolchain_refs::verify_emitted_references(&reg) {
                            Ok(()) => {
                                eprintln!(
                                    "OK: toolchain emit references ({} ids) are covered by the registry",
                                    lek::toolchain_refs::all_emitted_references().len()
                                );
                            }
                            Err(msg) => {
                                eprintln!("E7001 {msg}");
                                return std::process::ExitCode::from(1);
                            }
                        }
                    }
                    std::process::ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("E7001 invalid registry or IO: {e}");
                    std::process::ExitCode::from(1)
                }
            }
        }
        Command::Config { args } => {
            let p = args.path.or_else(|| {
                std::env::current_dir()
                    .ok()
                    .and_then(leekscript_config::find_manifest)
            });
            let Some(p) = p else {
                eprintln!("E7001 no Leek.toml found (use --path or run from cwd)");
                return std::process::ExitCode::from(1);
            };
            match leekscript_config::LeekManifest::load_path(&p) {
                Ok(m) => {
                    eprintln!("OK: {}", p.display());
                    if let Some(l) = m.language {
                        eprintln!("  language.version={:?} strict={:?}", l.version, l.strict);
                    }
                    if let Some(l) = m.lint {
                        eprintln!("  lint.level={:?}", l.level);
                    }
                    std::process::ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("E7001 invalid Leek.toml: {e}");
                    std::process::ExitCode::from(1)
                }
            }
        }
        Command::Init { args } => run_init(args),
        Command::Check { args } => {
            let cli_strict = if args.strict {
                Some(true)
            } else if args.no_strict {
                Some(false)
            } else {
                None
            };
            let manifest_path = manifest_path_for_cli(args.manifest.clone());
            let signature_globals = match lek::signatures::collect_signature_globals(
                manifest_path.as_ref(),
                &args.signatures,
            ) {
                Ok(g) => g,
                Err(e) => {
                    emit_message(format!("lek check: {e}"));
                    return std::process::ExitCode::from(1);
                }
            };
            run_check(
                &args.files,
                args.stdin_path,
                CheckOptions {
                    manifest: args.manifest,
                    cli_language_version: args.language_version,
                    cli_strict,
                    signature_globals,
                },
                args.message_format,
            )
        }
        Command::Run { args } => {
            let cli_strict = if args.strict {
                Some(true)
            } else if args.no_strict {
                Some(false)
            } else {
                None
            };
            let manifest_path = manifest_path_for_cli(args.manifest.clone());
            let signature_globals = match lek::signatures::collect_signature_globals(
                manifest_path.as_ref(),
                &args.signatures,
            ) {
                Ok(g) => g,
                Err(e) => {
                    emit_message(format!("lek run: {e}"));
                    return std::process::ExitCode::from(1);
                }
            };
            let reg = match leekscript_diagnostics::Registry::load_path(default_registry_path()) {
                Ok(r) => r,
                Err(e) => {
                    emit_message(format!("E7001 could not load diagnostic registry: {e}"));
                    return std::process::ExitCode::from(1);
                }
            };
            let fmt = match args.message_format {
                MessageFormat::Human => RunMessageFormat::Human,
                MessageFormat::Json => RunMessageFormat::Json,
            };
            run_compile(
                &args.files,
                args.stdin_path,
                CheckOptions {
                    manifest: args.manifest,
                    cli_language_version: args.language_version,
                    cli_strict,
                    signature_globals,
                },
                fmt,
                &reg,
            )
        }
        Command::Fmt { args } => run_fmt(args),
    }
}

fn run_fmt(args: FmtArgs) -> std::process::ExitCode {
    if args.stdin_path.is_some() && !args.files.iter().any(|p| p.as_os_str() == "-") {
        emit_message("lek fmt: --stdin-path only applies when '-' is among the input paths");
        return std::process::ExitCode::from(2);
    }
    let targets = match expand_check_targets(&args.files, args.stdin_path) {
        Ok(t) => t,
        Err(e) => {
            emit_message(format!("lek fmt: {e}"));
            return std::process::ExitCode::from(1);
        }
    };
    if targets.is_empty() {
        emit_message("lek fmt: no .leek files found under the given directories");
        return std::process::ExitCode::from(2);
    }

    if !args.write && !args.check && targets.len() > 1 {
        emit_message(
            "lek fmt: multiple inputs require --write or --check (stdout supports one file or stdin)",
        );
        return std::process::ExitCode::from(2);
    }

    let opts = FmtOptions {
        manifest: args.manifest,
        cli_language_version: args.language_version,
        cli_width: args.width,
        cli_indent: args.indent,
    };

    let mut exit: u8 = 0;

    for target in targets {
        let (src, path_display) = match &target {
            CheckTarget::Stdin { display } => match read_source(Path::new("-")) {
                Ok(s) => (s, display.display().to_string()),
                Err(e) => {
                    emit_message(format!("{}: {e}", display.display()));
                    exit = 1;
                    continue;
                }
            },
            CheckTarget::File(p) => match read_source(p) {
                Ok(s) => (s, p.display().to_string()),
                Err(e) => {
                    emit_message(format!("{}: {e}", p.display()));
                    exit = 1;
                    continue;
                }
            },
        };

        let formatted = match format_one_file(&src, &opts) {
            Ok(s) => s,
            Err(e) => {
                emit_fmt_error(&path_display, &src, &e);
                exit = 1;
                continue;
            }
        };

        if args.check {
            if src != formatted {
                eprintln!("would reformat: {path_display}");
                exit = 1;
            }
            continue;
        }

        if args.write {
            match &target {
                CheckTarget::Stdin { .. } => {
                    emit_message("lek fmt: cannot use --write with stdin (`-`)");
                    return std::process::ExitCode::from(2);
                }
                CheckTarget::File(p) => {
                    if let Err(e) = std::fs::write(p, formatted.as_str()) {
                        emit_message(format!("{}: {e}", p.display()));
                        exit = 1;
                    }
                }
            }
        } else {
            print!("{formatted}");
        }
    }

    std::process::ExitCode::from(exit)
}

fn record_to_json(r: DiagnosticRecord) -> JsonDiagnostic {
    JsonDiagnostic {
        file: r.file,
        line: r.line,
        column: r.column,
        code: r.code,
        reference: r.reference,
        message: r.message,
        phase: r.phase,
    }
}

fn run_check(
    files: &[PathBuf],
    stdin_path: Option<PathBuf>,
    opts: CheckOptions,
    message_format: MessageFormat,
) -> std::process::ExitCode {
    if stdin_path.is_some() && !files.iter().any(|p| p.as_os_str() == "-") {
        emit_message("lek check: --stdin-path only applies when '-' is among the input paths");
        return std::process::ExitCode::from(2);
    }
    let targets = match expand_check_targets(files, stdin_path) {
        Ok(t) => t,
        Err(e) => {
            emit_message(format!("lek check: {e}"));
            return std::process::ExitCode::from(1);
        }
    };
    if targets.is_empty() {
        emit_message("lek check: no .leek files found under the given directories");
        return std::process::ExitCode::from(2);
    }
    let reg = match leekscript_diagnostics::Registry::load_path(default_registry_path()) {
        Ok(r) => r,
        Err(e) => {
            emit_message(format!("E7001 could not load diagnostic registry: {e}"));
            return std::process::ExitCode::from(1);
        }
    };
    let mut json_out: Vec<JsonDiagnostic> = Vec::new();
    let mut json_files: Vec<JsonFileLine> = Vec::new();
    let mut exit: u8 = 0;

    for target in targets {
        let (src, path) = match &target {
            CheckTarget::Stdin { display } => match read_source(Path::new("-")) {
                Ok(s) => (s, display.as_path()),
                Err(e) => {
                    exit = 1;
                    let msg = format!("{e}");
                    match message_format {
                        MessageFormat::Human => emit_message(format!("{}: {e}", display.display())),
                        MessageFormat::Json => {
                            json_files.push(JsonFileLine::io_error(display));
                            json_out.push(JsonDiagnostic {
                                file: display.display().to_string(),
                                line: 0,
                                column: 0,
                                code: "E????".into(),
                                reference: "IO_ERROR".into(),
                                message: msg,
                                phase: "io",
                            });
                        }
                    }
                    continue;
                }
            },
            CheckTarget::File(p) => match read_source(p) {
                Ok(s) => (s, p.as_path()),
                Err(e) => {
                    exit = 1;
                    let msg = format!("{e}");
                    match message_format {
                        MessageFormat::Human => emit_message(format!("{}: {e}", p.display())),
                        MessageFormat::Json => {
                            json_files.push(JsonFileLine::io_error(p));
                            json_out.push(JsonDiagnostic {
                                file: p.display().to_string(),
                                line: 0,
                                column: 0,
                                code: "E????".into(),
                                reference: "IO_ERROR".into(),
                                message: msg,
                                phase: "io",
                            });
                        }
                    }
                    continue;
                }
            },
        };

        match check_one_file(&reg, path, &src, &opts) {
            CheckedFile::Ok(ok) => {
                if matches!(message_format, MessageFormat::Human) {
                    let strict_note = match ok.strict {
                        Some(true) => ", strict on",
                        Some(false) => ", strict off",
                        None => "",
                    };
                    eprintln!(
                        "{}: OK (HIR {} stmt(s), language {}{}, {} token(s))",
                        ok.path_display,
                        ok.hir_stmt_count,
                        ok.language_version,
                        strict_note,
                        ok.token_count
                    );
                } else {
                    json_files.push(JsonFileLine::ok(ok));
                }
            }
            CheckedFile::Failed(records) => {
                exit = 1;
                if matches!(message_format, MessageFormat::Json) {
                    json_files.push(JsonFileLine::error(path));
                }
                for r in records {
                    match message_format {
                        MessageFormat::Human => emit_diagnostic(&src, &r),
                        MessageFormat::Json => json_out.push(record_to_json(r)),
                    }
                }
            }
        }
    }

    if matches!(message_format, MessageFormat::Json) {
        match serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 4,
            "command": "check",
            "files": json_files,
            "diagnostics": json_out
        })) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                emit_message(format!("json error: {e}"));
                return std::process::ExitCode::from(1);
            }
        }
    }

    std::process::ExitCode::from(exit)
}
