//! Time the Rust HIR interpreter vs the reference Java runner on the same program and check that
//! `AI.export` / [`leekscript_run::value_java_export`] strings match.
//!
//! Prerequisites: JDK 25+ (`java` / `javac`, or `JAVA_HOME`), and
//! `leek-wars-generator/leekscript/leekscript.jar` (build with `./gradlew :leekscript:jar`).

use clap::Parser;
use globset::{Glob, GlobSetBuilder};
use leekscript_directives::parse_file_preamble;
use leekscript_run::{
    compile_source, interpret_hir_with_limits_and_stats, value_java_export, CompileOptions,
    CompiledUnit,
};
use owo_colors::OwoColorize;
use regex::Regex;
use tabled::{
    settings::{
        object::Columns, Alignment, Modify, Padding, Panel, Style,
    },
    Table, Tabled,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use std::io::{self, IsTerminal};

#[derive(clap::ValueEnum, Clone, Debug)]
enum CorpusMatchMode {
    /// Require every provided matcher type to match (intersection).
    All,
    /// Require any provided matcher type to match (union).
    Any,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Parser, Debug)]
#[command(name = "leekscript-bench")]
#[command(about = "Benchmark Rust vs Java LeekScript and compare export output")]
struct Args {
    /// Repository root (directory containing `leek-wars-generator/`). If omitted, walks upward from cwd.
    #[arg(long)]
    root: Option<PathBuf>,

    #[arg(long, default_value_t = 4)]
    version: u8,

    #[arg(long)]
    strict: bool,

    /// Full compile+run repetitions per engine (after warmup).
    #[arg(long, default_value_t = 1)]
    iterations: u32,

    /// Full compile+run warmup repetitions (not counted in summary).
    #[arg(long, default_value_t = 0)]
    warmup: u32,

    /// Output coloring (ANSI).
    #[arg(long, value_enum, default_value_t = ColorMode::Auto)]
    color: ColorMode,

    #[arg(long)]
    snippet: Option<String>,

    /// Run every `*.leek` file in this directory (non-overlapping with `FILE` / `--snippet`).
    #[arg(long, value_name = "DIR")]
    corpus: Option<PathBuf>,

    /// Include subdirectories when using `--corpus`.
    #[arg(long)]
    recursive: bool,

    /// Only keep corpus entries whose *relative path* contains this substring.
    /// Can be passed multiple times (OR semantics).
    #[arg(long, value_name = "SUBSTR")]
    corpus_filter: Vec<String>,

    /// Only keep corpus entries whose *relative path* starts with this prefix.
    /// Can be passed multiple times (OR semantics).
    #[arg(long, value_name = "PREFIX")]
    corpus_prefix: Vec<String>,

    /// Only keep corpus entries whose *relative path* matches this glob.
    /// Can be passed multiple times (OR semantics). Glob syntax is `globset`'s (similar to gitignore globs).
    #[arg(long, value_name = "GLOB")]
    corpus_glob: Vec<String>,

    /// Only keep corpus entries whose *relative path* matches this regex.
    /// Can be passed multiple times (OR semantics). Regex syntax is Rust `regex`.
    #[arg(long, value_name = "REGEX")]
    corpus_regex: Vec<String>,

    /// How to combine different corpus matcher types (prefix/filter/glob/regex).
    /// `all` = intersection (default), `any` = union.
    #[arg(long, value_enum, default_value_t = CorpusMatchMode::All)]
    corpus_match_mode: CorpusMatchMode,

    /// Skip the first N corpus entries (after sorting and filtering).
    #[arg(long, default_value_t = 0)]
    corpus_offset: usize,

    /// Keep at most N corpus entries (after sorting, filtering, and offset).
    #[arg(long)]
    corpus_limit: Option<usize>,

    /// Stop at the first compile error, export mismatch, or Java failure (corpus: mismatch details
    /// printed for files seen so far; without this flag, all mismatches are collected and printed at the end).
    #[arg(long)]
    fail_fast: bool,

    /// `.leek` file path (resolved against cwd if relative).
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    #[arg(long)]
    rust_only: bool,

    #[arg(long)]
    java_only: bool,

    /// Use `// leek-version` / `// leek-strict` from each source file for Rust compile and Java
    /// (instead of forcing `--version` / `--strict`). Use with `data/bench_corpus/java_vm/`.
    #[arg(long)]
    respect_preamble: bool,

    /// After Rust `summarize`, print min/avg interpreter operation and RAM-quad totals (relates wall
    /// time to abstract VM cost). Does not change Java runs.
    #[arg(long)]
    rust_stats: bool,

    /// Compile each Rust sample once, then time only `interpret_hir` for every iteration (Java still
    /// full compile+run per iteration). Use to isolate tree-walker cost from frontend time.
    #[arg(long)]
    compile_once: bool,

    /// Compare ops + export hash for every iteration (including warmup), not just the last.
    #[arg(long)]
    check_all_iters: bool,
}

#[derive(Clone, Debug, Default)]
struct Timing {
    compile_ms: f64,
    /// Java-only: init + staticInit + counter/limit setup time.
    init_ms: f64,
    /// Execution time only (Rust interpreter / Java `runIA`).
    run_ms: f64,
    /// Export formatting time only (Rust `value_java_export` / Java `AI.export`).
    export_ms: f64,
    /// Filled from [`leekscript_run::InterpretStats`] or Java `AI.operations()` (Java-style counter).
    ops_used: u64,
    /// 64-bit FNV-1a of the UTF-8 export string (stable, cheap parity signal).
    export_hash: u64,
    /// Approximate Java VM RAM quads tracked by the Rust interpreter.
    ram_quads_used: u64,
}

/// Last-iteration parity details when Rust and Java disagree.
#[derive(Clone, Debug)]
struct ExportMismatch {
    rust_export: String,
    java_export: String,
    rust_ops: u64,
    java_ops: u64,
    rust_export_hash: u64,
    java_export_hash: u64,
    rust_error: Option<String>,
    java_error: Option<String>,
}

struct SingleRunReport {
    rust_timings: Vec<Timing>,
    java_timings: Vec<Timing>,
    /// `None` when only one engine ran.
    parity_ok: Option<bool>,
    /// Set when `parity_ok == Some(false)` so corpus runs can summarize every mismatch at the end.
    export_mismatch: Option<ExportMismatch>,
}

/// Truncate long export strings for readable stderr (full values are in debug / tooling if needed).
const EXPORT_PREVIEW_MAX_CHARS: usize = 160;

fn preview_export_line(label: &str, value: &str) -> String {
    let n = value.chars().count();
    if n <= EXPORT_PREVIEW_MAX_CHARS {
        return format!("{label}: {value}");
    }
    let head: String = value.chars().take(EXPORT_PREVIEW_MAX_CHARS).collect();
    format!("{label}: {head}… ({n} chars)")
}

#[derive(Clone, Copy)]
enum CorpusItemStatus {
    Ok,
    ExportMismatch,
    RunError,
    SingleEngineOk,
}

fn emit_section(title: &str, color: &ColorMode) {
    let line = format!("── {title} ──");
    if want_color(color) {
        eprintln!("\n{}", line.dimmed());
    } else {
        eprintln!("\n{line}");
    }
}

fn format_corpus_status(status: CorpusItemStatus, color: &ColorMode) -> String {
    if !want_color(color) {
        return match status {
            CorpusItemStatus::Ok => "ok".into(),
            CorpusItemStatus::ExportMismatch => "mismatch".into(),
            CorpusItemStatus::RunError => "ERROR".into(),
            CorpusItemStatus::SingleEngineOk => "ok (1 eng)".into(),
        };
    }
    match status {
        CorpusItemStatus::Ok => "ok".green().to_string(),
        CorpusItemStatus::ExportMismatch => "mismatch".yellow().bold().to_string(),
        CorpusItemStatus::RunError => "ERROR".red().bold().to_string(),
        CorpusItemStatus::SingleEngineOk => "ok (1 eng)".dimmed().to_string(),
    }
}

fn print_export_mismatches_summary(entries: &[(String, ExportMismatch)], color: &ColorMode) {
    if entries.is_empty() {
        return;
    }
    emit_section(
        &format!("export mismatches ({})", entries.len()),
        color,
    );
    for (path, m) in entries {
        eprintln!("  {path}");
        if m.rust_error.is_some() || m.java_error.is_some() {
            eprintln!(
                "    errors: Rust {:?} vs Java {:?}",
                m.rust_error, m.java_error
            );
        }
        eprintln!("    {}", preview_export_line("Rust export", &m.rust_export));
        eprintln!("    {}", preview_export_line("Java export", &m.java_export));
        eprintln!(
            "    ops: Rust {} vs Java {} · hash: Rust {} vs Java {}",
            m.rust_ops, m.java_ops, m.rust_export_hash, m.java_export_hash
        );
    }
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let jar = dir.join("leek-wars-generator/leekscript/leekscript.jar");
        if jar.is_file() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
}

fn java_executable() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(PathBuf::from)
        .map(|mut p| {
            p.push("bin/java");
            p
        })
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("java"))
}

fn javac_executable() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(PathBuf::from)
        .map(|mut p| {
            p.push("bin/javac");
            p
        })
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("javac"))
}

fn ensure_java_runner(root: &Path, jar: &Path) -> Result<PathBuf, String> {
    let runner_src = root.join(
        "tools/parity_java_runner/src/main/java/leekscript/parity/ParitySnippetRunner.java",
    );
    if !runner_src.is_file() {
        return Err(format!("missing Java runner source: {}", runner_src.display()));
    }
    let out_dir = root.join("target/parity_java_runner_classes");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let marker = out_dir.join(".stamp");
    let need_compile = !marker.is_file()
        || std::fs::metadata(&runner_src)
            .and_then(|m| m.modified())
            .ok()
            .zip(std::fs::metadata(&marker).and_then(|m| m.modified()).ok())
            .map(|(s, t)| s > t)
            .unwrap_or(true);
    if need_compile {
        let status = Command::new(javac_executable())
            .arg("--release")
            .arg("25")
            .arg("-cp")
            .arg(jar)
            .arg("-d")
            .arg(&out_dir)
            .arg(&runner_src)
            .status()
            .map_err(|e| format!("javac: {e}"))?;
        if !status.success() {
            return Err("javac failed (need JDK 25+)".into());
        }
        std::fs::write(&marker, b"").map_err(|e| e.to_string())?;
    }
    Ok(out_dir)
}

#[derive(Clone, Debug)]
struct JavaBenchError {
    phase: String,
    reference: String,
}

fn parse_java_error(stderr: &str) -> Option<JavaBenchError> {
    for line in stderr.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("leek_bench_error ") else {
            continue;
        };
        let mut phase = None::<String>;
        let mut reference = None::<String>;
        for part in rest.split_whitespace() {
            if let Some(v) = part.strip_prefix("phase=") {
                phase = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("ref=") {
                reference = Some(v.to_string());
            }
        }
        if let (Some(phase), Some(reference)) = (phase, reference) {
            return Some(JavaBenchError { phase, reference });
        }
    }
    None
}

fn parse_java_stderr(stderr: &str) -> Result<Vec<Timing>, String> {
    let mut out = Vec::new();
    for line in stderr.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("leek_bench_error ") {
            // Caller should use `parse_java_error` to interpret this.
            continue;
        }
        let Some(rest) = line.strip_prefix("leek_bench_iter ") else {
            // Allow stack traces and other noise on stderr; we only care about structured rows.
            continue;
        };
        let mut compile_ms = None;
        let mut init_ms = None;
        let mut run_ms = None;
        let mut export_ms = None;
        let mut ops_used = None;
        let mut export_hash = None;
        for part in rest.split_whitespace() {
            if let Some(v) = part.strip_prefix("compile_ms=") {
                compile_ms = Some(v.parse::<f64>().map_err(|e| e.to_string())?);
            } else if let Some(v) = part.strip_prefix("init_ms=") {
                init_ms = Some(v.parse::<f64>().map_err(|e| e.to_string())?);
            } else if let Some(v) = part.strip_prefix("run_ms=") {
                run_ms = Some(v.parse::<f64>().map_err(|e| e.to_string())?);
            } else if let Some(v) = part.strip_prefix("export_ms=") {
                export_ms = Some(v.parse::<f64>().map_err(|e| e.to_string())?);
            } else if let Some(v) = part.strip_prefix("ops=") {
                ops_used = Some(v.parse::<u64>().map_err(|e| e.to_string())?);
            } else if let Some(v) = part.strip_prefix("export_hash=") {
                export_hash = Some(v.parse::<u64>().map_err(|e| e.to_string())?);
            }
        }
        out.push(Timing {
            compile_ms: compile_ms.ok_or_else(|| format!("missing compile_ms in {line}"))?,
            run_ms: run_ms.ok_or_else(|| format!("missing run_ms in {line}"))?,
            init_ms: init_ms.ok_or_else(|| format!("missing init_ms in {line}"))?,
            export_ms: export_ms.ok_or_else(|| format!("missing export_ms in {line}"))?,
            ops_used: ops_used.ok_or_else(|| format!("missing ops in {line}"))?,
            export_hash: export_hash.ok_or_else(|| format!("missing export_hash in {line}"))?,
            ram_quads_used: 0,
        });
    }
    Ok(out)
}

#[derive(Clone, Debug)]
struct BenchError {
    phase: &'static str, // "compile" | "run"
    reference: String,
}

#[derive(Clone, Debug)]
enum BenchOutcome {
    Ok { export: String, timings_all: Vec<Timing> },
    Err { error: BenchError },
}

fn fnv1a64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in s.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn run_java_snippet(
    root: &Path,
    jar: &Path,
    classes: &Path,
    version: u8,
    strict: bool,
    snippet: &str,
    repeat: u32,
) -> Result<BenchOutcome, String> {
    let mut cmd = Command::new(java_executable());
    cmd.current_dir(root);
    let cp = format!("{}:{}", classes.display(), jar.display());
    cmd.arg("-cp").arg(cp);
    cmd.arg("leekscript.parity.ParitySnippetRunner");
    cmd.arg("--version").arg(version.to_string());
    if strict {
        cmd.arg("--strict");
    }
    cmd.arg("--repeat").arg(repeat.to_string());
    cmd.arg("--code").arg(snippet);
    cmd.stdin(Stdio::null());
    cmd.stderr(Stdio::piped());
    cmd.stdout(Stdio::piped());
    let out = cmd.output().map_err(|e| format!("java: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        if let Some(e) = parse_java_error(&stderr) {
            return Ok(BenchOutcome::Err {
                error: BenchError {
                    phase: if e.phase == "compile" { "compile" } else { "run" },
                    reference: e.reference,
                },
            });
        }
        return Err(format!(
            "java exit {:?}\nstderr:\n{}",
            out.status.code(),
            stderr
        ));
    }
    let timings = parse_java_stderr(&stderr)?;
    Ok(BenchOutcome::Ok {
        export: stdout,
        timings_all: timings,
    })
}

fn run_java_file(
    jar: &Path,
    classes: &Path,
    version: u8,
    strict: bool,
    file: &Path,
    repeat: u32,
) -> Result<BenchOutcome, String> {
    let file = file
        .canonicalize()
        .map_err(|e| format!("{}: {e}", file.display()))?;
    let work_dir = file
        .parent()
        .ok_or_else(|| format!("{}: no parent directory", file.display()))?
        .to_path_buf();
    let leaf = file
        .file_name()
        .ok_or_else(|| format!("{}: not a file path", file.display()))?;
    let mut cmd = Command::new(java_executable());
    cmd.current_dir(&work_dir);
    let cp = format!("{}:{}", classes.display(), jar.display());
    cmd.arg("-cp").arg(cp);
    cmd.arg("leekscript.parity.ParitySnippetRunner");
    cmd.arg("--version").arg(version.to_string());
    if strict {
        cmd.arg("--strict");
    }
    cmd.arg("--repeat").arg(repeat.to_string());
    cmd.arg("--file").arg(leaf);
    cmd.stdin(Stdio::null());
    cmd.stderr(Stdio::piped());
    cmd.stdout(Stdio::piped());
    let out = cmd.output().map_err(|e| format!("java: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        if let Some(e) = parse_java_error(&stderr) {
            return Ok(BenchOutcome::Err {
                error: BenchError {
                    phase: if e.phase == "compile" { "compile" } else { "run" },
                    reference: e.reference,
                },
            });
        }
        return Err(format!(
            "java exit {:?}\nstderr:\n{}",
            out.status.code(),
            stderr
        ));
    }
    let timings = parse_java_stderr(&stderr)?;
    Ok(BenchOutcome::Ok {
        export: stdout,
        timings_all: timings,
    })
}

fn rust_compile_unit(
    path_label: &str,
    src: &str,
    opts: &CompileOptions,
) -> Result<(CompiledUnit, f64), BenchError> {
    let t0 = Instant::now();
    let unit = compile_source(path_label, src, opts).map_err(|diags| BenchError {
        phase: "compile",
        reference: diags
            .first()
            .map(|d| d.reference.clone())
            .unwrap_or_else(|| "UNKNOWN".into()),
    })?;
    let compile_ms = t0.elapsed().as_secs_f64() * 1000.0;
    Ok((unit, compile_ms))
}

fn rust_interpret_unit(unit: &CompiledUnit) -> Result<(String, f64, f64, u64, u64), BenchError> {
    let t_run = Instant::now();
    let (outcome, stats) = interpret_hir_with_limits_and_stats(
        &unit.hir,
        unit.language_version,
        unit.strict,
        None,
        None,
    )
    .map_err(|e| BenchError {
        phase: "run",
        reference: e.reference.to_string(),
    })?;
    let run_ms = t_run.elapsed().as_secs_f64() * 1000.0;
    let t_export = Instant::now();
    let export = match outcome {
        Some(v) => value_java_export(&v, unit.language_version),
        None => "null".to_string(),
    };
    let export_ms = t_export.elapsed().as_secs_f64() * 1000.0;
    Ok((
        export,
        run_ms,
        export_ms,
        stats.operations_used,
        stats.ram_quads_used,
    ))
}

fn rust_run_once(
    path_label: &str,
    src: &str,
    opts: &CompileOptions,
) -> Result<(String, Timing), BenchError> {
    let (unit, compile_ms) = rust_compile_unit(path_label, src, opts)?;
    let (export, run_ms, export_ms, ops_used, ram_quads_used) = rust_interpret_unit(&unit)?;
    let export_hash = fnv1a64(&export);
    Ok((
        export,
        Timing {
            compile_ms,
            init_ms: 0.0,
            run_ms,
            export_ms,
            ops_used,
            export_hash,
            ram_quads_used,
        },
    ))
}

#[derive(Clone, Copy, Debug)]
struct PhaseStats {
    min: f64,
    p50: f64,
    p90: f64,
    avg: f64,
    max: f64,
    stddev: f64,
}

#[derive(Clone, Copy, Debug)]
struct SummaryStats {
    compile: PhaseStats,
    init: PhaseStats,
    run: PhaseStats,
    export: PhaseStats,
    n: usize,
}

#[derive(Tabled)]
struct PhaseTimingRow {
    #[tabled(rename = "phase")]
    phase: String,
    #[tabled(rename = "min")]
    min_ms: String,
    #[tabled(rename = "p50")]
    p50_ms: String,
    #[tabled(rename = "p90")]
    p90_ms: String,
    #[tabled(rename = "avg")]
    avg_ms: String,
    #[tabled(rename = "max")]
    max_ms: String,
    #[tabled(rename = "σ")]
    std_ms: String,
    #[tabled(rename = "CV%")]
    cv_pct: String,
}

/// One row per pipeline phase when both engines ran: percentiles, spread (CV%), and Java/Rust ratio.
#[derive(Tabled)]
struct OverviewTimingRow {
    #[tabled(rename = "phase")]
    phase: String,
    #[tabled(rename = "R p50")]
    r_p50: String,
    #[tabled(rename = "R p90")]
    r_p90: String,
    #[tabled(rename = "R CV")]
    r_cv: String,
    #[tabled(rename = "J p50")]
    j_p50: String,
    #[tabled(rename = "J p90")]
    j_p90: String,
    #[tabled(rename = "J CV")]
    j_cv: String,
    #[tabled(rename = "J/R p50")]
    jr_p50: String,
    #[tabled(rename = "J/R p90")]
    jr_p90: String,
}

#[derive(Tabled)]
struct CounterSummaryRow {
    #[tabled(rename = "metric")]
    metric: String,
    #[tabled(rename = "min")]
    min: String,
    #[tabled(rename = "avg")]
    avg: String,
    #[tabled(rename = "max")]
    max: String,
}

fn want_color(mode: &ColorMode) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => io::stderr().is_terminal(),
    }
}

fn fmt_ms3(v: f64) -> String {
    format!("{v:.3}")
}

fn fmt_cv_pct(s: PhaseStats) -> String {
    let cv = phase_cv_ratio_pct(s);
    format!("{cv:.1}")
}

fn phase_cv_ratio_pct(s: PhaseStats) -> f64 {
    if s.avg == 0.0 {
        0.0
    } else {
        (s.stddev / s.avg) * 100.0
    }
}

/// For a metric where **lower is better** (latency ms, CV%), highlight the winning side.
/// First value/cell is Rust, second is Java. Ties stay unstyled.
fn color_lower_wins_rust_java_pair(
    rust_val: f64,
    java_val: f64,
    rust_txt: String,
    java_txt: String,
    use_color: bool,
) -> (String, String) {
    if !use_color {
        return (rust_txt, java_txt);
    }
    if !rust_val.is_finite() || !java_val.is_finite() {
        return (rust_txt, java_txt);
    }
    const EPS: f64 = 1e-9;
    if (rust_val - java_val).abs() <= EPS {
        return (rust_txt, java_txt);
    }
    if rust_val < java_val {
        (
            rust_txt.green().bold().to_string(),
            java_txt.dimmed().to_string(),
        )
    } else {
        (
            rust_txt.dimmed().to_string(),
            java_txt.green().bold().to_string(),
        )
    }
}

fn phase_timing_row(phase: &str, s: PhaseStats) -> PhaseTimingRow {
    // Keep phase + numeric cells plain; color only in dedicated ratio columns of the overview table.
    PhaseTimingRow {
        phase: phase.to_string(),
        min_ms: fmt_ms3(s.min),
        p50_ms: fmt_ms3(s.p50),
        p90_ms: fmt_ms3(s.p90),
        avg_ms: fmt_ms3(s.avg),
        max_ms: fmt_ms3(s.max),
        std_ms: fmt_ms3(s.stddev),
        cv_pct: fmt_cv_pct(s),
    }
}

fn print_summary_table(
    label: &str,
    n: usize,
    compile: PhaseStats,
    init: PhaseStats,
    run: PhaseStats,
    export: PhaseStats,
    color_mode: Option<&ColorMode>,
) {
    let use_color = color_mode.is_some_and(want_color);
    let title = format!("{label} · {n} timed samples");
    let title = if use_color {
        title.bold().to_string()
    } else {
        title
    };

    let rows = vec![
        phase_timing_row("compile", compile),
        phase_timing_row("init", init),
        phase_timing_row("run", run),
        phase_timing_row("export", export),
    ];

    let mut table = Table::new(rows);
    table
        .with(Style::markdown())
        .with(Panel::header(title))
        .with(Panel::footer(
            "ms · CV% = 100·σ/avg (relative spread; higher = noisier samples)",
        ))
        .with(Modify::new(Columns::new(1..)).with(Alignment::right()))
        .with(Padding::zero());

    eprintln!("{table}");
}
fn phase_stats(samples: &[f64]) -> Option<PhaseStats> {
    if samples.is_empty() {
        return None;
    }
    let mut v = samples.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    let min = v[0];
    let max = v[n - 1];
    let avg = v.iter().sum::<f64>() / n as f64;
    let var = v
        .iter()
        .map(|x| {
            let d = x - avg;
            d * d
        })
        .sum::<f64>()
        / n as f64;
    let stddev = var.sqrt();
    let pick = |q: f64| -> f64 {
        if n == 1 {
            return v[0];
        }
        let idx = ((n - 1) as f64 * q).round() as usize;
        v[idx.min(n - 1)]
    };
    Some(PhaseStats {
        min,
        p50: pick(0.50),
        p90: pick(0.90),
        avg,
        max,
        stddev,
    })
}

fn summarize_calc(samples: &[Timing]) -> Option<SummaryStats> {
    if samples.is_empty() {
        return None;
    }
    let compile = phase_stats(&samples.iter().map(|s| s.compile_ms).collect::<Vec<_>>())
        .expect("samples non-empty");
    let init =
        phase_stats(&samples.iter().map(|s| s.init_ms).collect::<Vec<_>>()).expect("non-empty");
    let run =
        phase_stats(&samples.iter().map(|s| s.run_ms).collect::<Vec<_>>()).expect("non-empty");
    let export = phase_stats(&samples.iter().map(|s| s.export_ms).collect::<Vec<_>>())
        .expect("non-empty");
    Some(SummaryStats {
        compile,
        init,
        run,
        export,
        n: samples.len(),
    })
}

fn overview_timing_row(phase: &str, r: PhaseStats, j: PhaseStats, use_color: bool) -> OverviewTimingRow {
    let p50_ratio = ratio(j.p50, r.p50);
    let p90_ratio = ratio(j.p90, r.p90);

    let r50 = fmt_ms3(r.p50);
    let j50 = fmt_ms3(j.p50);
    let (r_p50, j_p50) = color_lower_wins_rust_java_pair(r.p50, j.p50, r50, j50, use_color);

    let r90 = fmt_ms3(r.p90);
    let j90 = fmt_ms3(j.p90);
    let (r_p90, j_p90) = color_lower_wins_rust_java_pair(r.p90, j.p90, r90, j90, use_color);

    let r_cv = phase_cv_ratio_pct(r);
    let j_cv = phase_cv_ratio_pct(j);
    let r_cv_s = fmt_cv_pct(r);
    let j_cv_s = fmt_cv_pct(j);
    let (r_cv_cell, j_cv_cell) =
        color_lower_wins_rust_java_pair(r_cv, j_cv, r_cv_s, j_cv_s, use_color);

    OverviewTimingRow {
        phase: phase.to_string(),
        r_p50,
        r_p90,
        r_cv: r_cv_cell,
        j_p50,
        j_p90,
        j_cv: j_cv_cell,
        jr_p50: color_ratio_text(fmt_ratio(p50_ratio), p50_ratio, use_color),
        jr_p90: color_ratio_text(fmt_ratio(p90_ratio), p90_ratio, use_color),
    }
}

fn print_dual_engine_overview(
    scope: &str,
    rust: SummaryStats,
    java: SummaryStats,
    color: &ColorMode,
) {
    let use_color = want_color(color);
    let title = format!("Rust vs Java · {scope} · {} timed samples", rust.n);
    let title = if use_color {
        title.bold().to_string()
    } else {
        title
    };
    let rows = vec![
        overview_timing_row("compile", rust.compile, java.compile, use_color),
        overview_timing_row("init", rust.init, java.init, use_color),
        overview_timing_row("run", rust.run, java.run, use_color),
        overview_timing_row("export", rust.export, java.export, use_color),
    ];
    let mut table = Table::new(rows);
    table
        .with(Style::markdown())
        .with(Panel::header(title))
        .with(Panel::footer(
            "Green bold ms/CV = lower is better for that engine; dim = other side · J/R = Java÷Rust time",
        ))
        .with(Modify::new(Columns::new(1..)).with(Alignment::right()))
        .with(Padding::zero());
    eprintln!("{table}");
}

fn print_timings_report(
    scope: &str,
    rust: Option<SummaryStats>,
    java: Option<SummaryStats>,
    color: &ColorMode,
) {
    match (rust, java) {
        (Some(r), Some(j)) => print_dual_engine_overview(scope, r, j, color),
        (Some(s), None) => print_summary_table(
            &format!("Rust · {scope}"),
            s.n,
            s.compile,
            s.init,
            s.run,
            s.export,
            Some(color),
        ),
        (None, Some(s)) => print_summary_table(
            &format!("Java · {scope}"),
            s.n,
            s.compile,
            s.init,
            s.run,
            s.export,
            Some(color),
        ),
        (None, None) => {}
    }
}

fn fmt_ratio(r: f64) -> String {
    if r.is_nan() {
        return "n/a".into();
    }
    if r == f64::INFINITY {
        return " ∞x".into();
    }
    if r == f64::NEG_INFINITY {
        return "    -∞x".into();
    }
    format!("{:>7.2}x", r)
}

fn ratio(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        if a == 0.0 {
            1.0
        } else {
            f64::INFINITY
        }
    } else {
        a / b
    }
}

fn color_ratio_text(text: String, r: f64, use_color: bool) -> String {
    if !use_color {
        return text;
    }
    // Heuristic: red if >= 1.10x slower, green if <= 0.90x faster, dim otherwise.
    if r >= 1.10 {
        text.red().bold().to_string()
    } else if r <= 0.90 {
        text.green().bold().to_string()
    } else {
        text.dimmed().to_string()
    }
}

fn summarize_rust_interpreter_counters(label: &str, samples: &[Timing], color: &ColorMode) {
    if samples.is_empty() {
        return;
    }
    let use_color = want_color(color);

    let ops_min = samples.iter().map(|s| s.ops_used).min().unwrap_or(0);
    let ops_max = samples.iter().map(|s| s.ops_used).max().unwrap_or(0);
    let ops_avg =
        (samples.iter().map(|s| s.ops_used).sum::<u64>() as f64) / samples.len() as f64;

    let ram_min = samples.iter().map(|s| s.ram_quads_used).min().unwrap_or(0);
    let ram_max = samples.iter().map(|s| s.ram_quads_used).max().unwrap_or(0);
    let ram_avg =
        (samples.iter().map(|s| s.ram_quads_used).sum::<u64>() as f64) / samples.len() as f64;

    let title = format!("{label} · {} samples", samples.len());
    let title = if use_color {
        title.bold().to_string()
    } else {
        title
    };

    let rows = vec![
        CounterSummaryRow {
            metric: "ops".to_string(),
            min: ops_min.to_string(),
            avg: format!("{ops_avg:.1}"),
            max: ops_max.to_string(),
        },
        CounterSummaryRow {
            metric: "ram_quads".to_string(),
            min: ram_min.to_string(),
            avg: format!("{ram_avg:.1}"),
            max: ram_max.to_string(),
        },
    ];

    let mut table = Table::new(rows);
    table
        .with(Style::markdown())
        .with(Panel::header(title))
        .with(Panel::footer(
            "Rust-only counters · Java stderr lines include per-iter ops",
        ))
        .with(Modify::new(Columns::new(1..)).with(Alignment::right()))
        .with(Padding::zero());

    eprintln!("{table}");
}
fn collect_corpus_leek_files(dir: &Path, recursive: bool) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    collect_corpus_leek_files_inner(dir, recursive, &mut out)?;
    out.sort();
    Ok(out)
}

fn select_corpus_files(
    mut files: Vec<PathBuf>,
    corpus_root: &Path,
    args: &Args,
) -> Result<Vec<PathBuf>, String> {
    files.sort();

    let rel_str = |p: &PathBuf| -> String {
        p.strip_prefix(corpus_root)
            .unwrap_or(p.as_path())
            .display()
            .to_string()
    };

    let globset = if args.corpus_glob.is_empty() {
        None
    } else {
        let mut b = GlobSetBuilder::new();
        for g in &args.corpus_glob {
            let gg = Glob::new(g).map_err(|e| format!("--corpus-glob {g:?}: {e}"))?;
            b.add(gg);
        }
        Some(b.build().map_err(|e| format!("globset build: {e}"))?)
    };

    let regexes = if args.corpus_regex.is_empty() {
        Vec::new()
    } else {
        let mut out = Vec::new();
        for r in &args.corpus_regex {
            out.push(Regex::new(r).map_err(|e| format!("--corpus-regex {r:?}: {e}"))?);
        }
        out
    };

    let any_matchers = !args.corpus_prefix.is_empty()
        || !args.corpus_filter.is_empty()
        || globset.is_some()
        || !regexes.is_empty();
    files.retain(|p| {
        if !any_matchers {
            return true;
        }
        let rel = rel_str(p);
        let prefix_ok = if args.corpus_prefix.is_empty() {
            None
        } else {
            Some(args.corpus_prefix.iter().any(|pre| rel.starts_with(pre)))
        };
        let substr_ok = if args.corpus_filter.is_empty() {
            None
        } else {
            Some(args.corpus_filter.iter().any(|f| rel.contains(f)))
        };
        let glob_ok = match &globset {
            None => None,
            Some(gs) => Some(gs.is_match(rel.as_str())),
        };
        let regex_ok = if regexes.is_empty() {
            None
        } else {
            Some(regexes.iter().any(|re| re.is_match(rel.as_str())))
        };

        match args.corpus_match_mode {
            CorpusMatchMode::All => {
                prefix_ok.unwrap_or(true)
                    && substr_ok.unwrap_or(true)
                    && glob_ok.unwrap_or(true)
                    && regex_ok.unwrap_or(true)
            }
            CorpusMatchMode::Any => {
                prefix_ok.unwrap_or(false)
                    || substr_ok.unwrap_or(false)
                    || glob_ok.unwrap_or(false)
                    || regex_ok.unwrap_or(false)
            }
        }
    });

    let offset = args.corpus_offset.min(files.len());
    files.drain(0..offset);
    if let Some(limit) = args.corpus_limit {
        if files.len() > limit {
            files.truncate(limit);
        }
    }
    Ok(files)
}

fn collect_corpus_leek_files_inner(
    dir: &Path,
    recursive: bool,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for ent in read {
        let ent = ent.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = ent.path();
        let ty = ent
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if ty.is_dir() {
            if recursive {
                collect_corpus_leek_files_inner(&path, true, out)?;
            }
            continue;
        }
        if path.extension().is_some_and(|e| e == "leek") {
            out.push(path);
        }
    }
    Ok(())
}

fn run_single_snippet(
    label: String,
    src: String,
    repo_root: &Path,
    jar: &Path,
    java_classes: Option<&Path>,
    args: &Args,
    total: u32,
) -> Result<SingleRunReport, String> {
    let compile_opts = CompileOptions {
        manifest: None,
        cli_language_version: Some(args.version),
        cli_strict: Some(args.strict),
        source_path: None,
        snippet_origin: None,
        signature_globals: vec![],
    };

    let mut rust_exports = Vec::new();
    let mut rust_ops_all = Vec::new();
    let mut rust_hash_all = Vec::new();
    let mut rust_timings = Vec::new();
    let mut rust_error = None::<BenchError>;
    if !args.java_only {
        if args.compile_once {
            let (unit, compile_ms) = rust_compile_unit(&label, &src, &compile_opts)
                .map_err(|e| format!("{} {}", e.phase, e.reference))?;
            eprintln!(
                "Rust: compile once {:.3} ms; iterations time interpret only (Java unchanged)",
                compile_ms
            );
            for i in 0..total {
                let (ex, run_ms, export_ms, ops_used, ram_quads_used) =
                    match rust_interpret_unit(&unit) {
                        Ok(x) => x,
                        Err(e) => {
                            rust_error = Some(e);
                            break;
                        }
                    };
                let h = fnv1a64(&ex);
                rust_exports.push(ex);
                rust_ops_all.push(ops_used);
                rust_hash_all.push(h);
                if i >= args.warmup {
                    rust_timings.push(Timing {
                        compile_ms: 0.0,
                        init_ms: 0.0,
                        run_ms,
                        export_ms,
                        ops_used,
                        export_hash: h,
                        ram_quads_used,
                    });
                }
            }
        } else {
            for i in 0..total {
                let (ex, t) = match rust_run_once(&label, &src, &compile_opts) {
                    Ok(x) => x,
                    Err(e) => {
                        rust_error = Some(e);
                        break;
                    }
                };
                let h = fnv1a64(&ex);
                rust_exports.push(ex);
                rust_ops_all.push(t.ops_used);
                rust_hash_all.push(h);
                if i >= args.warmup {
                    rust_timings.push(Timing {
                        export_hash: h,
                        ..t
                    });
                }
            }
        }
    }

    let mut java_export = None::<String>;
    let mut java_timings = Vec::new();
    let mut java_timings_all = Vec::new();
    let mut java_error = None::<BenchError>;
    if !args.rust_only {
        let classes = java_classes.ok_or_else(|| "internal: missing Java classes".to_string())?;
        let outcome = run_java_snippet(
            repo_root,
            jar,
            classes,
            args.version,
            args.strict,
            &src,
            total,
        )?;
        match outcome {
            BenchOutcome::Ok {
                export,
                timings_all,
            } => {
                if timings_all.len() != total as usize {
                    return Err(format!(
                        "expected {} Java timing rows, got {}",
                        total,
                        timings_all.len()
                    ));
                }
                java_timings_all = timings_all;
                for (i, t) in java_timings_all.iter().cloned().enumerate() {
                    if i >= args.warmup as usize {
                        java_timings.push(t);
                    }
                }
                java_export = Some(export);
            }
            BenchOutcome::Err { error } => {
                java_error = Some(error);
            }
        }
    }

    let (parity_ok, export_mismatch) = if !args.java_only && !args.rust_only {
        if let (Some(re), Some(je)) = (&rust_error, &java_error) {
            if re.phase == je.phase && re.reference == je.reference {
                return Ok(SingleRunReport {
                    rust_timings,
                    java_timings,
                    parity_ok: Some(true),
                    export_mismatch: None,
                });
            }
        }
        if rust_error.is_some() || java_error.is_some() {
            let re = rust_error
                .as_ref()
                .map(|e| format!("{} {}", e.phase, e.reference));
            let je = java_error
                .as_ref()
                .map(|e| format!("{} {}", e.phase, e.reference));
            return Ok(SingleRunReport {
                rust_timings,
                java_timings,
                parity_ok: Some(false),
                export_mismatch: Some(ExportMismatch {
                    rust_export: rust_exports.last().cloned().unwrap_or_else(|| "<none>".into()),
                    java_export: java_export.clone().unwrap_or_else(|| "<none>".into()),
                    rust_ops: *rust_ops_all.last().unwrap_or(&0),
                    java_ops: java_timings_all.last().map(|t| t.ops_used).unwrap_or(0),
                    rust_export_hash: *rust_hash_all.last().unwrap_or(&0),
                    java_export_hash: java_timings_all.last().map(|t| t.export_hash).unwrap_or(0),
                    rust_error: re,
                    java_error: je,
                }),
            });
        }
        let r_last = rust_exports.last().ok_or_else(|| "rust: no export".to_string())?;
        let j_last = java_export.as_ref().ok_or_else(|| "java: no export".to_string())?;
        let r_ops_last = *rust_ops_all
            .last()
            .ok_or_else(|| "rust: no ops counter for last iteration".to_string())?;
        let j_ops_last = java_timings_all
            .last()
            .map(|t| t.ops_used)
            .ok_or_else(|| "java: no ops counter for last iteration".to_string())?;
        let r_hash_last = *rust_hash_all
            .last()
            .ok_or_else(|| "rust: no export hash for last iteration".to_string())?;
        let j_hash_last = java_timings_all
            .last()
            .map(|t| t.export_hash)
            .ok_or_else(|| "java: no export hash for last iteration".to_string())?;

        if args.check_all_iters {
            if rust_ops_all.len() != java_timings_all.len()
                || rust_hash_all.len() != java_timings_all.len()
            {
                return Err("internal: iteration vectors length mismatch".into());
            }
            for (i, jt) in java_timings_all.iter().enumerate() {
                let rops = rust_ops_all[i];
                let rh = rust_hash_all[i];
                if rops != jt.ops_used || rh != jt.export_hash {
                    return Ok(SingleRunReport {
                        rust_timings,
                        java_timings,
                        parity_ok: Some(false),
                        export_mismatch: Some(ExportMismatch {
                            rust_export: rust_exports[i].clone(),
                            java_export: j_last.clone(),
                            rust_ops: rops,
                            java_ops: jt.ops_used,
                            rust_export_hash: rh,
                            java_export_hash: jt.export_hash,
                            rust_error: None,
                            java_error: None,
                        }),
                    });
                }
            }
        }

        // Parity is defined by the exported value string (hashed), not by ops counters.
        // Ops are useful for diagnostics/benchmarking but are not stable across engines.
        if r_hash_last != j_hash_last || r_last != j_last {
            (
                Some(false),
                Some(ExportMismatch {
                    rust_export: r_last.clone(),
                    java_export: j_last.clone(),
                    rust_ops: r_ops_last,
                    java_ops: j_ops_last,
                    rust_export_hash: r_hash_last,
                    java_export_hash: j_hash_last,
                    rust_error: None,
                    java_error: None,
                }),
            )
        } else {
            if rust_exports.windows(2).any(|w| w[0] != w[1]) {
                eprintln!("warning: Rust exports differ across iterations (non-deterministic program?)");
            }
            (Some(true), None)
        }
    } else {
        (None, None)
    };

    Ok(SingleRunReport {
        rust_timings,
        java_timings,
        parity_ok,
        export_mismatch,
    })
}

fn java_version_strict_for_source(src: &str, args: &Args) -> (u8, bool) {
    if args.respect_preamble {
        let (preamble, _) = parse_file_preamble(src, 64);
        let v = preamble.language_version.unwrap_or(4);
        let s = preamble.strict.unwrap_or(args.strict);
        (v, s)
    } else {
        (args.version, args.strict)
    }
}

fn run_single_file(
    path: &Path,
    jar: &Path,
    java_classes: Option<&Path>,
    args: &Args,
    total: u32,
) -> Result<SingleRunReport, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let canon = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let (cli_version, cli_strict) = if args.respect_preamble {
        (None, None)
    } else {
        (Some(args.version), Some(args.strict))
    };
    let compile_opts = CompileOptions {
        manifest: None,
        cli_language_version: cli_version,
        cli_strict: cli_strict,
        source_path: Some(canon.clone()),
        snippet_origin: Some(canon),
        signature_globals: vec![],
    };
    let mut rust_exports = Vec::new();
    let mut rust_ops_all = Vec::new();
    let mut rust_hash_all = Vec::new();
    let mut rust_timings = Vec::new();
    let mut rust_error = None::<BenchError>;
    if !args.java_only {
        let path_label = path.display().to_string();
        if args.compile_once {
            let (unit, compile_ms) = rust_compile_unit(&path_label, &src, &compile_opts)
                .map_err(|e| format!("{} {}", e.phase, e.reference))?;
            eprintln!(
                "Rust: compile once {:.3} ms; iterations time interpret only (Java unchanged)",
                compile_ms
            );
            for i in 0..total {
                let (ex, run_ms, export_ms, ops_used, ram_quads_used) =
                    match rust_interpret_unit(&unit) {
                        Ok(x) => x,
                        Err(e) => {
                            rust_error = Some(e);
                            break;
                        }
                    };
                let h = fnv1a64(&ex);
                rust_exports.push(ex);
                rust_ops_all.push(ops_used);
                rust_hash_all.push(h);
                if i >= args.warmup {
                    rust_timings.push(Timing {
                        compile_ms: 0.0,
                        init_ms: 0.0,
                        run_ms,
                        export_ms,
                        ops_used,
                        export_hash: h,
                        ram_quads_used,
                    });
                }
            }
        } else {
            for i in 0..total {
                let (ex, t) = match rust_run_once(&path_label, &src, &compile_opts) {
                    Ok(x) => x,
                    Err(e) => {
                        rust_error = Some(e);
                        break;
                    }
                };
                let h = fnv1a64(&ex);
                rust_exports.push(ex);
                rust_ops_all.push(t.ops_used);
                rust_hash_all.push(h);
                if i >= args.warmup {
                    rust_timings.push(Timing {
                        export_hash: h,
                        ..t
                    });
                }
            }
        }
    }

    let mut java_export = None::<String>;
    let mut java_timings = Vec::new();
    let mut java_timings_all = Vec::new();
    let mut java_error = None::<BenchError>;
    if !args.rust_only {
        let classes = java_classes.ok_or_else(|| "internal: missing Java classes".to_string())?;
        let (java_v, java_s) = java_version_strict_for_source(&src, args);
        let outcome = run_java_file(
            jar,
            classes,
            java_v,
            java_s,
            path,
            total,
        )?;
        match outcome {
            BenchOutcome::Ok {
                export,
                timings_all,
            } => {
                if timings_all.len() != total as usize {
                    return Err(format!(
                        "expected {} Java timing rows, got {}",
                        total,
                        timings_all.len()
                    ));
                }
                java_timings_all = timings_all;
                for (i, t) in java_timings_all.iter().cloned().enumerate() {
                    if i >= args.warmup as usize {
                        java_timings.push(t);
                    }
                }
                java_export = Some(export);
            }
            BenchOutcome::Err { error } => {
                java_error = Some(error);
            }
        }
    }

    let (parity_ok, export_mismatch) = if !args.java_only && !args.rust_only {
        if let (Some(re), Some(je)) = (&rust_error, &java_error) {
            if re.phase == je.phase && re.reference == je.reference {
                return Ok(SingleRunReport {
                    rust_timings,
                    java_timings,
                    parity_ok: Some(true),
                    export_mismatch: None,
                });
            }
        }
        if rust_error.is_some() || java_error.is_some() {
            let re = rust_error
                .as_ref()
                .map(|e| format!("{} {}", e.phase, e.reference));
            let je = java_error
                .as_ref()
                .map(|e| format!("{} {}", e.phase, e.reference));
            return Ok(SingleRunReport {
                rust_timings,
                java_timings,
                parity_ok: Some(false),
                export_mismatch: Some(ExportMismatch {
                    rust_export: rust_exports.last().cloned().unwrap_or_else(|| "<none>".into()),
                    java_export: java_export.clone().unwrap_or_else(|| "<none>".into()),
                    rust_ops: *rust_ops_all.last().unwrap_or(&0),
                    java_ops: java_timings_all.last().map(|t| t.ops_used).unwrap_or(0),
                    rust_export_hash: *rust_hash_all.last().unwrap_or(&0),
                    java_export_hash: java_timings_all.last().map(|t| t.export_hash).unwrap_or(0),
                    rust_error: re,
                    java_error: je,
                }),
            });
        }
        let r_last = rust_exports.last().ok_or_else(|| "rust: no export".to_string())?;
        let j_last = java_export.as_ref().ok_or_else(|| "java: no export".to_string())?;
        let r_ops_last = *rust_ops_all
            .last()
            .ok_or_else(|| "rust: no ops counter for last iteration".to_string())?;
        let j_ops_last = java_timings_all
            .last()
            .map(|t| t.ops_used)
            .ok_or_else(|| "java: no ops counter for last iteration".to_string())?;
        let r_hash_last = *rust_hash_all
            .last()
            .ok_or_else(|| "rust: no export hash for last iteration".to_string())?;
        let j_hash_last = java_timings_all
            .last()
            .map(|t| t.export_hash)
            .ok_or_else(|| "java: no export hash for last iteration".to_string())?;

        if args.check_all_iters {
            if rust_ops_all.len() != java_timings_all.len()
                || rust_hash_all.len() != java_timings_all.len()
            {
                return Err("internal: iteration vectors length mismatch".into());
            }
            for (i, jt) in java_timings_all.iter().enumerate() {
                let rops = rust_ops_all[i];
                let rh = rust_hash_all[i];
                if rops != jt.ops_used || rh != jt.export_hash {
                    return Ok(SingleRunReport {
                        rust_timings,
                        java_timings,
                        parity_ok: Some(false),
                        export_mismatch: Some(ExportMismatch {
                            rust_export: rust_exports[i].clone(),
                            java_export: j_last.clone(),
                            rust_ops: rops,
                            java_ops: jt.ops_used,
                            rust_export_hash: rh,
                            java_export_hash: jt.export_hash,
                            rust_error: None,
                            java_error: None,
                        }),
                    });
                }
            }
        }

        // Parity is defined by exported value (string/hash). Ops counters are useful diagnostics
        // but not stable across engines, so do not fail parity on ops by default.
        if r_last != j_last || r_hash_last != j_hash_last {
            (
                Some(false),
                Some(ExportMismatch {
                    rust_export: r_last.clone(),
                    java_export: j_last.clone(),
                    rust_ops: r_ops_last,
                    java_ops: j_ops_last,
                    rust_export_hash: r_hash_last,
                    java_export_hash: j_hash_last,
                    rust_error: None,
                    java_error: None,
                }),
            )
        } else {
            if rust_exports.windows(2).any(|w| w[0] != w[1]) {
                eprintln!("warning: Rust exports differ across iterations (non-deterministic program?)");
            }
            (Some(true), None)
        }
    } else {
        (None, None)
    };

    Ok(SingleRunReport {
        rust_timings,
        java_timings,
        parity_ok,
        export_mismatch,
    })
}

fn main_inner(args: Args) -> Result<(), String> {
    if args.rust_only && args.java_only {
        return Err("choose at most one of --rust-only and --java-only".into());
    }

    let mode_count = usize::from(args.snippet.is_some())
        + usize::from(args.file.is_some())
        + usize::from(args.corpus.is_some());
    if mode_count == 0 {
        return Err("pass a FILE argument, --snippet, or --corpus DIR".into());
    }
    if mode_count > 1 {
        return Err("choose exactly one of: FILE, --snippet, --corpus".into());
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = args
        .root
        .clone()
        .or_else(|| find_repo_root(&cwd))
        .ok_or_else(|| {
            String::from(
                "could not find repo root (expected leek-wars-generator/leekscript/leekscript.jar); use --root",
            )
        })?;
    let jar = root.join("leek-wars-generator/leekscript/leekscript.jar");
    if !jar.is_file() {
        return Err(format!(
            "missing {} — build with: (cd leek-wars-generator && ./gradlew :leekscript:jar)",
            jar.display()
        ));
    }

    let total = args.warmup + args.iterations;
    if total == 0 {
        return Err("--warmup + --iterations must be > 0".into());
    }

    let java_classes = if args.rust_only {
        None
    } else {
        Some(ensure_java_runner(&root, &jar)?)
    };

    if let Some(corpus_dir) = &args.corpus {
        let corpus_dir = corpus_dir
            .canonicalize()
            .map_err(|e| format!("{}: {e}", corpus_dir.display()))?;
        if !corpus_dir.is_dir() {
            return Err(format!("--corpus is not a directory: {}", corpus_dir.display()));
        }
        let all_files = collect_corpus_leek_files(&corpus_dir, args.recursive)?;
        let files = select_corpus_files(all_files, &corpus_dir, &args)?;
        if files.is_empty() {
            return Err(format!(
                "no selected *.leek files under {} (after filters/offset/limit)",
                corpus_dir.display()
            ));
        }

        emit_section("corpus", &args.color);
        eprintln!(
            "  {} file(s) · match {:?} · prefix {} · substr {} · glob {} · regex {} · offset {} · limit {:?}",
            files.len(),
            args.corpus_match_mode,
            args.corpus_prefix.len(),
            args.corpus_filter.len(),
            args.corpus_glob.len(),
            args.corpus_regex.len(),
            args.corpus_offset,
            args.corpus_limit
        );
        eprintln!();

        let mut all_rust = Vec::new();
        let mut all_java = Vec::new();
        let mut failures = 0usize;
        let mut export_mismatches: Vec<(String, ExportMismatch)> = Vec::new();

        for path in &files {
            let rel = path.strip_prefix(&corpus_dir).unwrap_or(path.as_path());
            let rel_str = rel.display().to_string();
            match run_single_file(path, &jar, java_classes.as_deref(), &args, total) {
                Ok(rep) => {
                    let status = match rep.parity_ok {
                        Some(true) => CorpusItemStatus::Ok,
                        Some(false) => {
                            failures += 1;
                            if let Some(detail) = rep.export_mismatch.clone() {
                                export_mismatches.push((rel_str.clone(), detail));
                            }
                            CorpusItemStatus::ExportMismatch
                        }
                        None => CorpusItemStatus::SingleEngineOk,
                    };
                    let st = format_corpus_status(status, &args.color);
                    eprintln!("  {st}  {rel_str}");
                    all_rust.extend(rep.rust_timings);
                    all_java.extend(rep.java_timings);
                    if rep.parity_ok == Some(false) && args.fail_fast {
                        eprintln!();
                        if want_color(&args.color) {
                            eprintln!("{}", "Stopped by --fail-fast (export mismatch).".yellow().bold());
                        } else {
                            eprintln!("Stopped by --fail-fast (export mismatch).");
                        }
                        print_export_mismatches_summary(&export_mismatches, &args.color);
                        std::process::exit(4);
                    }
                }
                Err(e) => {
                    failures += 1;
                    let st = format_corpus_status(CorpusItemStatus::RunError, &args.color);
                    eprintln!("  {st}  {rel_str}");
                    eprintln!(" {e}");
                    if args.fail_fast {
                        eprintln!();
                        if want_color(&args.color) {
                            eprintln!("{}", "Stopped by --fail-fast (run error).".yellow().bold());
                        } else {
                            eprintln!("Stopped by --fail-fast (run error).");
                        }
                        print_export_mismatches_summary(&export_mismatches, &args.color);
                        std::process::exit(4);
                    }
                }
            }
        }

        emit_section("summary", &args.color);
        eprintln!(
            "  {} file(s) · {} ok · {} failed",
            files.len(),
            files.len() - failures,
            failures
        );
        print_export_mismatches_summary(&export_mismatches, &args.color);

        emit_section("timings", &args.color);
        let rust_sum = if !args.java_only {
            summarize_calc(&all_rust)
        } else {
            None
        };
        let java_sum = if !args.rust_only {
            summarize_calc(&all_java)
        } else {
            None
        };
        print_timings_report("corpus aggregate", rust_sum, java_sum, &args.color);

        if !args.java_only && args.rust_stats && !all_rust.is_empty() {
            emit_section("rust interpreter", &args.color);
            summarize_rust_interpreter_counters(
                "Corpus aggregate · interpreter counters",
                &all_rust,
                &args.color,
            );
        }
        if failures > 0 {
            std::process::exit(4);
        }
        return Ok(());
    }

    let report = if let Some(snippet) = &args.snippet {
        let mut s = snippet.clone();
        if !s.ends_with('\n') {
            s.push('\n');
        }
        run_single_snippet(
            "<snippet>".to_string(),
            s,
            &root,
            &jar,
            java_classes.as_deref(),
            &args,
            total,
        )?
    } else {
        let path = args.file.as_ref().unwrap();
        run_single_file(path, &jar, java_classes.as_deref(), &args, total)?
    };

    let input_label = args
        .file
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<snippet>".into());

    if report.parity_ok == Some(false) {
        if let Some(m) = report.export_mismatch.clone() {
            print_export_mismatches_summary(&[(input_label.clone(), m)], &args.color);
        }
    }

    emit_section("result", &args.color);
    if report.parity_ok == Some(false) {
        let msg = format!(
            "  parity: FAILED (last of {total} full pipeline run(s) per engine; timings below are still shown)"
        );
        if want_color(&args.color) {
            eprintln!("{}", msg.red().bold());
        } else {
            eprintln!("{msg}");
        }
    } else if !args.java_only && !args.rust_only {
        eprintln!("  parity: ok · {total} full pipeline run(s) per engine");
    } else {
        eprintln!("  single-engine run · {total} iteration(s) recorded");
    }

    emit_section("timings", &args.color);
    let rust_sum = if !args.java_only {
        summarize_calc(&report.rust_timings)
    } else {
        None
    };
    let java_sum = if !args.rust_only {
        summarize_calc(&report.java_timings)
    } else {
        None
    };
    print_timings_report(&input_label, rust_sum, java_sum, &args.color);

    if !args.java_only && args.rust_stats && !report.rust_timings.is_empty() {
        emit_section("rust interpreter", &args.color);
        summarize_rust_interpreter_counters(
            "Interpreter counters",
            &report.rust_timings,
            &args.color,
        );
    }

    if report.parity_ok == Some(false) {
        return Err("parity mismatch (Rust vs Java)".into());
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    // Some corpus snippets use deep recursion (e.g. `rec(1000)`). The Rust interpreter is not
    // stackless, so run the benchmark logic on a larger stack to avoid aborting the whole tool.
    let handle = std::thread::Builder::new()
        .name("leekscript-bench-run".into())
        // Must be large enough for deep recursion corpus samples, but not so large that systems with
        // strict virtual memory limits kill the process.
        //
        // Runtime evidence (debug session a01b8c) shows stack overflow can occur in `value_java_export`
        // for some v1–v3 corpus samples, so we provision a larger stack for the bench thread.
        .stack_size(256 * 1024 * 1024)
        .spawn(move || main_inner(args))
        .map_err(|e| format!("failed to spawn bench thread: {e}"))?;

    match handle.join() {
        Ok(res) => res,
        Err(_) => Err("bench thread panicked".into()),
    }
}
