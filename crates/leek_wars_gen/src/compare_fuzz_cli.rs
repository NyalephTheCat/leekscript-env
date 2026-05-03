//! Unified `leekgen-compare` / `leekgen-fuzz` CLI implementation (library module for thin `[[bin]]` wrappers).

use clap::{Parser, ValueEnum};
use rand::Rng;

use crate::engine::{default_java_cwd, resolve_generator_jar, JavaEngineConfig, RunRequest};
use crate::error::GenError;
use crate::fuzz::{
    discover_ai_rel_paths, discover_fuzz_scenario_rels, merge_parity_corpus_scenarios,
    run_fuzz_timed, FuzzBenchSummary, FuzzConfig,
};
use crate::harness::{
    discover_scenario_json_files, discover_scenario_json_files_recursive, run_scenario_harness,
    CompareMode, CompareResult, HarnessRunConfig, ScenarioHarnessReport, TimingSummary,
    INCOMPLETE_SCENARIO_BASELINES,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn resolve_fuzz_artifacts_dir(dir: &Path) -> PathBuf {
    if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| dir.to_path_buf(), |cwd| cwd.join(dir))
    }
}

// --- shared formatting ----------------------------------------------------

fn section_stderr(title: &str) {
    eprintln!();
    eprintln!("── {title} ──");
}

fn section_stdout(title: &str) {
    println!();
    println!("── {title} ──");
}

fn compare_status_one_line(compare: &CompareResult) -> &'static str {
    match compare {
        CompareResult::FullMatch => "ok — full normalized match",
        CompareResult::FullMismatch { note, .. } if note.contains("no A/B comparison") => {
            "n/a — single engine (no A/B compare)"
        }
        CompareResult::FullMismatch { .. } => "mismatch — normalized outcome differs",
        CompareResult::WinnerDurationMatch => "ok — winner and duration match",
        CompareResult::WinnerDurationMismatch { .. } => "mismatch — winner or duration differs",
        CompareResult::ActionsSubsequenceOk => {
            "ok — official generator actions ⊆ Rust order (minimal filter)"
        }
        CompareResult::ActionsSubsequenceFail { .. } => {
            "mismatch — action subsequence check failed"
        }
        CompareResult::ActionsExactMatch => "ok — fight.actions match (exact)",
        CompareResult::ActionsExactMismatch { .. } => "mismatch — fight.actions differ (exact)",
        CompareResult::OpsExactMatch => "ok — fight.ops match (exact)",
        CompareResult::OpsExactMismatch { .. } => "mismatch — fight.ops differ (exact)",
        CompareResult::EngineRunMismatch { .. } => {
            "mismatch — engine run error(s); see stderr blocks below"
        }
        CompareResult::OutcomeNotJson { .. } => "mismatch — outcome stdout is not comparable JSON",
    }
}

fn print_timing_table_stdout(label_java: &str, java: Option<&TimingSummary>, rust: &TimingSummary) {
    println!(
        "  {:<8} {:>9} {:>9} {:>9}  samples",
        "", "min", "median", "mean"
    );
    if let Some(j) = java {
        print_timing_row_stdout(label_java, j);
    } else {
        println!(
            "  {:<8} {:>9} {:>9} {:>9}  (skipped)",
            label_java, "—", "—", "—"
        );
    }
    print_timing_row_stdout("Rust", rust);
}

fn print_timing_row_stdout(engine: &str, t: &TimingSummary) {
    if t.iterations == 0 {
        println!("  {:<8} {:>9} {:>9} {:>9}  n=0", engine, "—", "—", "—");
    } else {
        println!(
            "  {:<8} {:>8.2}ms {:>8.2}ms {:>8.2}ms  n={}",
            engine, t.min_ms, t.median_ms, t.mean_ms, t.iterations
        );
    }
}

fn print_timing_table_stderr(title: &str, rows: &[(&str, &TimingSummary)]) {
    eprintln!("{title}");
    eprintln!(
        "  {:<10} {:>9} {:>9} {:>9}  samples",
        "", "min", "median", "mean"
    );
    for (name, t) in rows {
        if t.iterations == 0 {
            eprintln!("  {:<10} {:>9} {:>9} {:>9}  n=0", name, "—", "—", "—");
        } else {
            eprintln!(
                "  {:<10} {:>8.2}ms {:>8.2}ms {:>8.2}ms  n={}",
                name, t.min_ms, t.median_ms, t.mean_ms, t.iterations
            );
        }
    }
}

struct FuzzBannerParams<'a> {
    root: &'a Path,
    requested: u64,
    run_until_interrupt: bool,
    ok: u64,
    failed: usize,
    master_seed: u64,
    scenario_count: usize,
    ai_count: usize,
    scenarios_recursive: bool,
    parity_corpus_merged: bool,
    fuzz_seed: bool,
    shuffle_ais: bool,
    benchmark_java: bool,
    parity_profile: bool,
    fuzz_compare_mode: Option<&'static str>,
    mutate_ai_level: u8,
    jitter_entity_stats: bool,
    jitter_max_turns: bool,
    jitter_map: bool,
    jitter_cells: bool,
    jitter_max_ops: bool,
    jitter_loadout: bool,
    mutate_ai_inject_complexity: u8,
    mutate_ai_inject_wrap_percent: u8,
    mutate_ai_inject_max_stmts: u8,
    mutate_ai_require_parseable: bool,
    randomize_draw_rule: bool,
    artifacts_dir: Option<&'a Path>,
}

fn print_fuzz_banner_stderr(p: FuzzBannerParams<'_>) {
    section_stderr("fuzz");
    eprintln!("  {:<18} {}", "Generator root", p.root.display());
    if p.parity_profile {
        eprintln!(
            "  {:<18} strict (no scenario jitter; parseable + compilable AI mutants; Java benchmark on)",
            "Parity preset"
        );
    }
    if let Some(a) = p.artifacts_dir {
        eprintln!("  {:<18} {}", "Artifacts dir", a.display());
    }
    eprintln!(
        "  {:<18} {}",
        "Requested iters",
        if p.run_until_interrupt {
            "∞ (first Ctrl+C: finish current iteration, then print summary; second Ctrl+C: exit 130)"
                .to_string()
        } else {
            p.requested.to_string()
        }
    );
    let attempted = p.ok.saturating_add(p.failed as u64);
    let requested_label = if p.run_until_interrupt {
        "∞".to_string()
    } else {
        p.requested.to_string()
    };
    eprintln!(
        "  {:<18} {} ok, {} failed (attempted {}, requested {})",
        "Progress", p.ok, p.failed, attempted, requested_label
    );
    if p.run_until_interrupt {
        if attempted > 0 {
            eprintln!(
                "  {:<18} {:.1}% of attempted iterations succeeded",
                "Success rate",
                100.0 * (p.ok as f64) / (attempted as f64)
            );
        }
    } else if p.requested > 0 {
        eprintln!(
            "  {:<18} {:.1}% of requested iterations succeeded",
            "Success rate",
            100.0 * (p.ok as f64) / (p.requested as f64)
        );
    }
    eprintln!("  {:<18} {}", "Master seed", p.master_seed);
    eprintln!(
        "  {:<18} --fuzz --fuzz-master-seed {}",
        "Replay cmd", p.master_seed
    );
    eprintln!(
        "  {:<18} {} (recursive={}  parity_corpus={})",
        "Scenario pool", p.scenario_count, p.scenarios_recursive, p.parity_corpus_merged
    );
    eprintln!("  {:<18} {}", "AI scripts", p.ai_count);
    let java_cmp = p
        .fuzz_compare_mode
        .map_or(String::new(), |m| format!("  java_compare={m}"));
    eprintln!(
        "  {:<18} random_seed={}  shuffle_ai={}  java_harness={}{}",
        "Variants", p.fuzz_seed, p.shuffle_ais, p.benchmark_java, java_cmp,
    );
    let mut_ai = if p.mutate_ai_level == 0 {
        "off".to_string()
    } else if p.benchmark_java {
        format!(
            "level {} (official-generator+Rust sandbox)",
            p.mutate_ai_level
        )
    } else {
        format!("level {} (Rust overlay)", p.mutate_ai_level)
    };
    let inject = if p.mutate_ai_level >= 4 {
        format!(
            "inject_complexity={}  inject_wrap={}٪  inject_max_stmts={}  require_parseable={}",
            p.mutate_ai_inject_complexity,
            p.mutate_ai_inject_wrap_percent,
            p.mutate_ai_inject_max_stmts,
            p.mutate_ai_require_parseable
        )
    } else {
        "inject=(level<4)".to_string()
    };
    eprintln!(
        "  {:<18} mutate_leek={}  {}  jitter_entity_stats={}  jitter_max_turns={}  jitter_map={}  jitter_cells={}  jitter_max_ops={}  jitter_loadout={}  random_draw_rule={}",
        "AI / scenario",
        mut_ai,
        inject,
        p.jitter_entity_stats,
        p.jitter_max_turns,
        p.jitter_map,
        p.jitter_cells,
        p.jitter_max_ops,
        p.jitter_loadout,
        p.randomize_draw_rule
    );
}

fn print_fuzz_results_stderr(
    bench: &FuzzBenchSummary,
    benchmark_java: bool,
    fuzz_compare_mode: Option<&str>,
) {
    section_stderr("timing (successful iterations only)");
    let mut rows: Vec<(&str, &TimingSummary)> = vec![("Rust", &bench.rust_wall_ms)];
    if let Some(j) = bench.java_wall_ms.as_ref() {
        rows.push(("Generator", j));
    }
    print_timing_table_stderr(
        "Wall-clock per fuzz iteration (median of inner harness where applicable):",
        &rows,
    );

    if benchmark_java {
        eprintln!();
        if let Some(m) = fuzz_compare_mode {
            eprintln!("  Fuzz compare mode: {m}");
        }
        eprintln!(
            "  Parity: mismatched {} / {} successful iterations",
            bench.parity_mismatches, bench.summary.iterations_ok
        );
    }
}

#[derive(Copy, Clone, Debug, Default, ValueEnum)]
enum ModeArg {
    /// Full JSON equality after normalizing timing fields.
    Full,
    /// Only `winner` and `duration` must match.
    #[default]
    Winner,
    /// Official generator: action codes (filtered) must appear in order within Rust.
    ActionsMinimal,
    /// Exact equality of `fight.actions` after stripping top-level timing keys.
    ActionsExact,
    /// Exact equality of `fight.ops` after stripping top-level timing keys.
    OpsExact,
}

/// Compare mode for `--fuzz --fuzz-benchmark-java` only (independent of `--mode`, which applies to non-fuzz compare).
#[derive(Copy, Clone, Debug, Default, ValueEnum)]
enum FuzzCompareModeArg {
    /// [`CompareMode::FullNormalized`]: structural equality after stripping top-level `*_time` keys and `logs`
    /// (includes `fight.actions`, `fight.ops`, map, entities snapshot, etc.).
    #[default]
    Full,
    /// [`CompareMode::WinnerDuration`].
    Winner,
    /// [`CompareMode::ActionsMinimal`].
    ActionsMinimal,
    /// [`CompareMode::ActionsExact`].
    ActionsExact,
    /// [`CompareMode::OpsExact`].
    OpsExact,
}

fn fuzz_harness_compare_mode(a: FuzzCompareModeArg) -> CompareMode {
    match a {
        FuzzCompareModeArg::Full => CompareMode::FullNormalized,
        FuzzCompareModeArg::Winner => CompareMode::WinnerDuration,
        FuzzCompareModeArg::ActionsMinimal => CompareMode::ActionsMinimal,
        FuzzCompareModeArg::ActionsExact => CompareMode::ActionsExact,
        FuzzCompareModeArg::OpsExact => CompareMode::OpsExact,
    }
}

#[derive(Parser)]
pub struct CompareCli {
    #[arg(long, default_value_t = 0)]
    warmup: usize,

    #[arg(long, default_value_t = 1)]
    iterations: usize,

    #[arg(long, value_enum, default_value_t = ModeArg::Winner)]
    mode: ModeArg,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    dump_json: bool,

    #[arg(long)]
    no_diff: bool,

    /// Skip the official generator: Rust-only runs (no JVM). Does not require `generator.jar` if `--cwd` or `LEEK_GENERATOR_CWD` is set.
    #[arg(long)]
    skip_java: bool,

    #[arg(long)]
    skip_rust: bool,

    #[arg(long)]
    java_bin: Option<PathBuf>,

    #[arg(long)]
    jar: Option<PathBuf>,

    #[arg(long)]
    cwd: Option<PathBuf>,

    #[arg(long, value_name = "DIR")]
    scenarios_dir: Option<PathBuf>,

    /// Walk subdirectories of `--scenarios-dir` when discovering `*.json`.
    #[arg(long)]
    scenarios_recursive: bool,

    #[arg(value_name = "SCENARIO")]
    scenarios: Vec<PathBuf>,
}

#[derive(Parser)]
pub struct FuzzCliOpts {
    /// Replay a fuzz artifact directory (created by `--fuzz --fuzz-artifacts-dir ...`).
    #[arg(long = "replay", value_name = "DIR", conflicts_with = "fuzz")]
    replay: Option<PathBuf>,

    /// Randomized fuzz loop over scenarios / seeds / AI paths (see `--fuzz-*` flags).
    #[arg(long)]
    pub fuzz: bool,

    #[arg(long, value_name = "DIR")]
    fuzz_root: Option<PathBuf>,

    #[arg(long, default_value = "test/scenario")]
    fuzz_scenarios_dir: PathBuf,

    /// Walk subdirectories of `--fuzz-scenarios-dir` when discovering `*.json` (e.g. `test/scenario/generated/`).
    #[arg(long = "fuzz-scenarios-recursive")]
    fuzz_scenarios_recursive: bool,

    /// Allow the fuzzer to generate a fresh scenario JSON from scratch (teams/entities/map/etc.)
    /// instead of always starting from an existing scenario template on disk.
    #[arg(long = "fuzz-generate-scenarios")]
    fuzz_generate_scenarios: bool,

    /// When `--fuzz-generate-scenarios` is set, percent chance per iteration (0..=100) to generate
    /// from scratch. The remaining iterations use scenario templates from `--fuzz-scenarios-dir`.
    #[arg(long = "fuzz-generate-scenarios-percent", default_value_t = 10)]
    fuzz_generate_scenarios_percent: u8,

    /// Scenario generation: minimum entities per team (generated scenarios only).
    #[arg(long = "fuzz-gen-min-entities-per-team", default_value_t = 1)]
    fuzz_gen_min_entities_per_team: u8,

    /// Scenario generation: maximum entities per team (generated scenarios only).
    #[arg(long = "fuzz-gen-max-entities-per-team", default_value_t = 4)]
    fuzz_gen_max_entities_per_team: u8,

    /// Scenario generation: minimum map side length (square maps; generated scenarios only).
    #[arg(long = "fuzz-gen-min-map", default_value_t = 9)]
    fuzz_gen_min_map: u8,

    /// Scenario generation: maximum map side length (square maps; generated scenarios only).
    #[arg(long = "fuzz-gen-max-map", default_value_t = 25)]
    fuzz_gen_max_map: u8,

    /// Scenario generation: percent chance to assign a random weapon/chip loadout to an entity.
    #[arg(long = "fuzz-gen-loadout-percent", default_value_t = 35)]
    fuzz_gen_loadout_percent: u8,

    /// Convenience preset: enable all scenario jitter knobs (`--fuzz-jitter-*`) plus legacy ones (`--fuzz-jitter-entity-stats`, `--fuzz-jitter-max-turns`, `--fuzz-randomize-draw-rule`).
    #[arg(long = "fuzz-jitter-all")]
    fuzz_jitter_all: bool,

    /// Convenience preset: “turn it up”: enables `--fuzz-jitter-all`, forces recursive scenario discovery, bumps mutation to level 4, and allows external AIs.
    #[arg(long = "fuzz-chaos")]
    fuzz_chaos: bool,

    /// Favor **official generator vs Rust** agreement on the same mutated AI + scenario: implies `--fuzz-benchmark-java`, disables all scenario jitter (including `--fuzz-jitter-all` / `--fuzz-chaos` stress), enables `--fuzz-mutate-ai-require-parseable`, and **rejects AI mutants that do not compile** in the fight pipeline (same `compile_source` + signatures as the Rust engine). Statement-inject/wrap is skipped automatically on **single-statement** `.leek` files (see `leekscript_fuzz`) so tiny `include` targets stay Java-compatible. Does not change `--fuzz-mutate-ai-level` / inject knobs / `--fuzz-compare-mode` (default remains full normalized JSON).
    #[arg(long = "fuzz-parity", requires = "fuzz")]
    fuzz_parity: bool,

    /// Merge canonical parity scenario paths when they exist under the generator root (see `fuzz::FUZZ_PARITY_SCENARIO_CORPUS`).
    #[arg(long = "fuzz-no-parity-corpus", action = clap::ArgAction::SetTrue)]
    fuzz_no_parity_corpus: bool,

    #[arg(long = "fuzz-scenario", value_name = "REL_PATH")]
    fuzz_scenarios: Vec<PathBuf>,

    #[arg(long, default_value = "test/ai")]
    fuzz_ai_dir: PathBuf,

    #[arg(long = "fuzz-ai", value_name = "REL_PATH")]
    fuzz_ais: Vec<String>,

    /// Add an AI source from an arbitrary file path (outside the generator tree).
    /// The fuzzer will copy it into the per-iteration overlay/sandbox under `_external_ai/`
    /// and may assign it to entities when shuffling AIs.
    #[arg(long = "fuzz-ai-file", value_name = "PATH")]
    fuzz_ai_files: Vec<PathBuf>,

    /// Add all `*.leek` under a directory (recursive), mirrored into `_external_ai/<dir_name>/...`
    /// so relative includes resolve.
    /// Useful for fuzzing a whole AI codebase (e.g. `--fuzz-ai-files-dir ./ai`).
    #[arg(long = "fuzz-ai-files-dir", value_name = "DIR")]
    fuzz_ai_files_dirs: Vec<PathBuf>,

    /// In non-parity fuzzing, reject AI mutants unless they compile in the fight pipeline
    /// (more time spent exploring runtime behavior vs parse/compile failures).
    #[arg(long = "fuzz-require-compilable-ai")]
    fuzz_require_compilable_ai: bool,

    /// Gradually increase mutation intensity over time (non-parity fuzz only).
    #[arg(long = "fuzz-mutate-ramp")]
    fuzz_mutate_ramp: bool,

    /// With `--fuzz-mutate-ramp`, increase ramp level every N iterations.
    #[arg(long = "fuzz-mutate-ramp-every", default_value_t = 200)]
    fuzz_mutate_ramp_every: u64,

    /// When importing external AI directories via `--fuzz-ai-files-dir`, only include likely entrypoints
    /// (`main.leek` and `merged-for-upload.leek`) in the shuffle pool by default.
    #[arg(long = "fuzz-ai-entrypoints-only", default_value_t = true)]
    fuzz_ai_entrypoints_only: bool,

    /// `0` = run until the first **Ctrl+C** (stops before starting another iteration, then prints the same summary as a fixed run). Second Ctrl+C exits immediately with code 130.
    #[arg(long = "fuzz-n", default_value_t = 100)]
    fuzz_iterations: u64,

    #[arg(long = "fuzz-master-seed")]
    fuzz_master_seed: Option<u64>,

    #[arg(long = "fuzz-no-seed")]
    fuzz_no_fuzz_seed: bool,

    #[arg(long = "fuzz-no-shuffle-ai")]
    fuzz_no_shuffle_ai: bool,

    #[arg(long = "fuzz-allow-external-ais")]
    fuzz_allow_external_ais: bool,

    #[arg(long = "fuzz-keep-temps")]
    fuzz_keep_temps: bool,

    #[arg(long = "fuzz-continue-on-error")]
    fuzz_continue_on_error: bool,

    #[arg(long = "fuzz-quiet")]
    fuzz_quiet: bool,

    /// Print a `[fuzz] attempts=…` line on stderr every N **finished** attempts (`0` = never). Default: `1` with `--fuzz-n 0`, else `10` (overridden to `0` by `--fuzz-quiet`).
    #[arg(long = "fuzz-progress-every")]
    fuzz_progress_every: Option<u64>,

    /// On parity mismatch or run error, write a subdirectory per finding (`scenario.json`, `meta.json`, `outcome_*.json`, `compare.json`, optional minimal `sandbox/`: `_fuzz/scenario.json` when present, `mirror.patch` (unified diff vs `generator_root` for `--fuzz-ai-dir`), **no** `data/`, **no** generated top-level `ai/` tree, **no** full `.leek` mirror copy). Relative paths are resolved from the **current working directory** (same as `cargo run`), not from the generator root.
    #[arg(long = "fuzz-artifacts-dir", value_name = "DIR")]
    fuzz_artifacts_dir: Option<PathBuf>,

    /// Each fuzz iteration: full official generator vs Rust harness (timing + compare). Slow (many JVM runs). Uses `--fuzz-compare-mode` (default: full outcome parity), `--iterations`, `--skip-*` from compare options; `--warmup` is forced to 0 for the fuzz loop.
    ///
    /// **AI sources:** same `.leek` mutation as Rust-only fuzz (full `--fuzz-ai-dir` tree; see `--fuzz-mutate-ai-level`).
    #[arg(long = "fuzz-benchmark-java")]
    fuzz_benchmark_java: bool,

    /// How to compare official generator vs Rust outcomes when `--fuzz-benchmark-java` is set. Default is full normalized JSON (actions, ops, map, …); see [`crate::parity::normalize_outcome_json`].
    #[arg(long = "fuzz-compare-mode", value_enum, default_value_t = FuzzCompareModeArg::Full)]
    fuzz_compare_mode: FuzzCompareModeArg,

    /// Ignored (kept for old scripts). Mutation is always controlled by `--fuzz-mutate-ai-level` (`0` = off).
    #[arg(long = "fuzz-mutate-ai", hide = true, action = clap::ArgAction::SetTrue)]
    _fuzz_mutate_ai: bool,

    /// Every `*.leek` under `--fuzz-ai-dir` is mutated each iteration. `0` = off. `1` = trailing comment only. `2` = light CST mutations (numbers, bools, commutative `+/*`/`==`/`!=` swap, optional extra parens). `3` = more edits per file. `4` = heavier CST pass plus a second mutation round per file. If the source does not parse, falls back to comment + digit noise. Same for Rust-only and `--fuzz-benchmark-java`.
    #[arg(long = "fuzz-mutate-ai-level", default_value_t = 1, value_parser = clap::value_parser!(u8).range(0..=4))]
    fuzz_mutate_ai_level: u8,

    /// AI mutation: how complex injected statement blocks may be (0=off, 1=simple, 2=medium, 3+=heavier). Only relevant for `--fuzz-mutate-ai-level 4`.
    #[arg(long = "fuzz-mutate-ai-inject-complexity", default_value_t = 2, value_parser = clap::value_parser!(u8).range(0..=8))]
    fuzz_mutate_ai_inject_complexity: u8,

    /// AI mutation: percent chance (0..=100) to offer statement-wrap injection mutations per statement node. Only relevant for `--fuzz-mutate-ai-level 4`.
    #[arg(long = "fuzz-mutate-ai-inject-wrap-percent", default_value_t = 55, value_parser = clap::value_parser!(u8).range(0..=100))]
    fuzz_mutate_ai_inject_wrap_percent: u8,

    /// AI mutation: maximum number of injected statements appended when a wrap mutation is used. Only relevant for `--fuzz-mutate-ai-level 4`.
    #[arg(long = "fuzz-mutate-ai-inject-max-stmts", default_value_t = 3, value_parser = clap::value_parser!(u8).range(1..=16))]
    fuzz_mutate_ai_inject_max_stmts: u8,

    /// AI mutation: require parseable output (retries) instead of always accepting the first candidate.
    #[arg(long = "fuzz-mutate-ai-require-parseable")]
    fuzz_mutate_ai_require_parseable: bool,

    /// Randomize numeric stats on entities that use a `.leek` AI.
    #[arg(long = "fuzz-jitter-entity-stats")]
    fuzz_jitter_entity_stats: bool,

    /// Randomize `max_turns` with a small bounded delta.
    #[arg(long = "fuzz-jitter-max-turns")]
    fuzz_jitter_max_turns: bool,

    /// Randomize `draw_check_life` each iteration.
    #[arg(long = "fuzz-randomize-draw-rule")]
    fuzz_randomize_draw_rule: bool,

    /// Perturb `map.obstacles` when a `map` object is present.
    #[arg(long = "fuzz-jitter-map")]
    fuzz_jitter_map: bool,

    /// Re-roll entity `cell` indices for `.leek` fighters (uses map width × height when present).
    #[arg(long = "fuzz-jitter-entity-cells")]
    fuzz_jitter_entity_cells: bool,

    /// Randomize `max_operations_per_entity` around the fixture baseline.
    #[arg(long = "fuzz-jitter-max-ops")]
    fuzz_jitter_max_ops: bool,

    /// Nudge `weapons` / `chips` arrays on `.leek` entities when those keys exist.
    #[arg(long = "fuzz-jitter-loadout")]
    fuzz_jitter_loadout: bool,
}

#[derive(Parser)]
#[command(name = "leekgen-compare")]
#[command(version)]
#[command(
    about = "Benchmark official generator.jar vs Rust, or fuzz with optional timing and parity checks."
)]
#[command(
    long_about = "Compare mode runs fixed scenarios through the official generator and/or Rust and compares outcomes.\n\
\n\
Fuzz mode (--fuzz) randomizes random_seed and entity AI paths over your scenario pool and reports wall-clock stats.\n\
\n\
Examples:\n\
  leekgen-compare test/scenario/scenario1.json\n\
  leekgen-compare --scenarios-dir test/scenario --mode full\n\
  leekgen-compare --fuzz --fuzz-n 0\n\
  leekgen-compare --fuzz --fuzz-artifacts-dir ./fuzz-out\n\
  leekgen-compare --fuzz --fuzz-n 500 --fuzz-master-seed 42\n\
  leekgen-compare --fuzz --fuzz-n 20 --fuzz-benchmark-java\n\
  leekgen-compare --fuzz --fuzz-parity --fuzz-n 200 --fuzz-mutate-ai-level 4\n\
  leekgen-compare --fuzz --fuzz-benchmark-java --fuzz-compare-mode winner\n\
  leekgen-compare --fuzz --fuzz-jitter-entity-stats --fuzz-jitter-map --fuzz-n 200\n\
  leekgen-compare --fuzz --fuzz-scenarios-recursive --fuzz-mutate-ai-level 4\n\
  leekgen-fuzz -n 100 --quiet\n\
"
)]
pub struct TopCli {
    #[command(flatten)]
    pub compare: CompareCli,

    #[command(flatten)]
    pub fuzz_opts: FuzzCliOpts,
}

fn print_compare_divergence(compare: &CompareResult) {
    match compare {
        CompareResult::FullMismatch {
            normalized_diff: Some(d),
            ..
        } if !d.is_empty() => {
            section_stdout("diff (rust [-] vs generator [+])");
            println!("{d}");
        }
        CompareResult::WinnerDurationMismatch {
            java_winner,
            rust_winner,
            java_duration,
            rust_duration,
        } => {
            section_stdout("winner / duration (rust = reference)");
            println!(
                "  {:<12} rust {rust_winner}  |  generator {java_winner}",
                "Winner"
            );
            println!(
                "  {:<12} rust {rust_duration}  |  generator {java_duration}",
                "Duration"
            );
        }
        CompareResult::ActionsSubsequenceFail {
            java_codes,
            rust_codes,
        } => {
            section_stdout("action codes (filtered; rust = reference)");
            println!("  rust ({}): {:?}", rust_codes.len(), rust_codes);
            println!("  generator ({}): {:?}", java_codes.len(), java_codes);
            let j = java_codes.as_slice();
            let r = rust_codes.as_slice();
            let mut i = 0usize;
            let mut k = 0usize;
            while i < j.len() && k < r.len() {
                if j[i] == r[k] {
                    i += 1;
                }
                k += 1;
            }
            if i < j.len() {
                println!(
                    "  first generator code not in order within rust: {:?} (generator index {})",
                    j.get(i),
                    i
                );
            }
        }
        CompareResult::ActionsExactMismatch {
            java_len,
            rust_len,
            first_diff_index,
        } => {
            section_stdout("fight.actions (exact)");
            println!("  generator actions: {java_len}");
            println!("  rust actions:      {rust_len}");
            if let Some(i) = first_diff_index {
                println!("  first differing index: {i}");
            } else if java_len != rust_len {
                println!("  no differing prefix element; length differs");
            }
        }
        CompareResult::OpsExactMismatch {
            java_len,
            rust_len,
            first_diff_index,
        } => {
            section_stdout("fight.ops (exact)");
            println!("  generator ops: {java_len}");
            println!("  rust ops:      {rust_len}");
            if let Some(i) = first_diff_index {
                println!("  first differing index: {i}");
            } else if java_len != rust_len {
                println!("  no differing prefix element; length differs");
            }
        }
        CompareResult::EngineRunMismatch {
            java_error,
            rust_error,
        } => {
            section_stdout("engine run mismatch (also in report JSON)");
            if let Some(j) = java_error {
                println!("  Java (official generator):");
                for line in j.lines() {
                    println!("    {line}");
                }
            }
            if let Some(r) = rust_error {
                println!("  Rust:");
                for line in r.lines() {
                    println!("    {line}");
                }
            }
        }
        CompareResult::OutcomeNotJson { java, rust } => {
            section_stdout("outcome JSON / parse (generator vs Rust)");
            if let Some(j) = java {
                println!("  Generator outcome parse: {j}");
            } else {
                println!("  Generator outcome parse: (ok)");
            }
            if let Some(r) = rust {
                println!("  Rust outcome parse: {r}");
            } else {
                println!("  Rust outcome parse: (ok)");
            }
        }
        _ => {}
    }
}

fn java_bin() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(PathBuf::from)
        .map(|mut p| {
            p.push("bin/java");
            p
        })
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("java"))
}

fn compare_mode(a: ModeArg) -> CompareMode {
    match a {
        ModeArg::Full => CompareMode::FullNormalized,
        ModeArg::Winner => CompareMode::WinnerDuration,
        ModeArg::ActionsMinimal => CompareMode::ActionsMinimal,
        ModeArg::ActionsExact => CompareMode::ActionsExact,
        ModeArg::OpsExact => CompareMode::OpsExact,
    }
}

fn generator_root(fuzz_root: Option<&PathBuf>) -> Result<PathBuf, GenError> {
    if let Some(r) = fuzz_root {
        if r.is_dir() {
            return Ok(r.clone());
        }
        return Err(GenError::Message(format!(
            "--fuzz-root is not a directory: {}",
            r.display()
        )));
    }
    if let Ok(c) = std::env::var("LEEK_GENERATOR_CWD") {
        let p = PathBuf::from(c);
        if p.is_dir() {
            return Ok(p);
        }
    }
    let jar = resolve_generator_jar()?;
    Ok(default_java_cwd(&jar))
}

fn run_compare(cli: &CompareCli) -> Result<(), Box<dyn std::error::Error>> {
    let run_java = !cli.skip_java;
    let run_rust = !cli.skip_rust;

    let cwd: PathBuf = cli
        .cwd
        .clone()
        .or_else(|| std::env::var("LEEK_GENERATOR_CWD").ok().map(PathBuf::from))
        .or_else(|| {
            resolve_generator_jar()
                .ok()
                .map(|jar| default_java_cwd(&jar))
        })
        .ok_or_else(|| {
            GenError::Message(
                "could not resolve generator checkout: pass --cwd, set LEEK_GENERATOR_CWD, or place generator.jar for auto-detect"
                    .into(),
            )
        })?;
    if !cwd.is_dir() {
        return Err(Box::new(GenError::CwdMissing(cwd)));
    }

    let jar = if run_java {
        cli.jar
            .clone()
            .or_else(|| resolve_generator_jar().ok())
            .ok_or(GenError::JarNotFound)?
    } else {
        PathBuf::new()
    };

    let java_cfg = JavaEngineConfig {
        jar,
        cwd: cwd.clone(),
        java_bin: cli.java_bin.clone().unwrap_or_else(java_bin),
    };

    let harness_cfg = HarnessRunConfig {
        java: java_cfg,
        mode: compare_mode(cli.mode),
        warmup: cli.warmup,
        iterations: cli.iterations,
        run_java,
        run_rust,
        runtime_cwd: None,
    };

    let mut scenarios: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    if let Some(ref dir) = cli.scenarios_dir {
        let paths = if cli.scenarios_recursive {
            discover_scenario_json_files_recursive(&cwd, dir)?
        } else {
            discover_scenario_json_files(&cwd, dir)?
        };
        for p in paths {
            let skip = p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| INCOMPLETE_SCENARIO_BASELINES.contains(&n));
            if skip {
                continue;
            }
            if seen.insert(p.clone()) {
                scenarios.push(p);
            }
        }
    }
    for p in &cli.scenarios {
        if seen.insert(p.clone()) {
            scenarios.push(p.clone());
        }
    }
    if scenarios.is_empty() {
        scenarios.push(PathBuf::from("test/scenario/scenario1.json"));
    }

    let mut reports: Vec<ScenarioHarnessReport> = Vec::new();
    for rel in scenarios {
        let req = RunRequest {
            file: rel,
            ..Default::default()
        };
        let mut report = run_scenario_harness(&req, &harness_cfg)?;
        if !cli.dump_json {
            report.last_java_json = None;
            report.last_rust_json = String::new();
        }
        reports.push(report);
    }

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
    } else {
        section_stdout("compare run");
        println!("  {:<14} {}", "JVM cwd", cwd.display());
        println!("  {:<14} {}", "Scenarios", reports.len());
        println!(
            "  {:<14} {} (warmup {}, inner iters {})",
            "Harness",
            match (harness_cfg.run_java, harness_cfg.run_rust) {
                (true, true) => "Official generator + Rust",
                (true, false) => "Official generator only",
                (false, true) => "Rust only",
                (false, false) => "(invalid)",
            },
            harness_cfg.warmup,
            harness_cfg.iterations
        );
        for r in &reports {
            section_stdout(&format!("{}", r.scenario.display()));
            println!("  {:<14} {}", "Compare mode", r.mode);
            println!("  {:<14} {}", "Status", compare_status_one_line(&r.compare));
            if r.java_error.is_some() || r.rust_error.is_some() {
                section_stdout("engine stderr / errors (raw)");
                if let Some(j) = &r.java_error {
                    println!("  --- Java (official generator) ---");
                    for line in j.lines() {
                        println!("  {line}");
                    }
                }
                if let Some(rust_e) = &r.rust_error {
                    println!("  --- Rust ---");
                    for line in rust_e.lines() {
                        println!("  {line}");
                    }
                }
            }
            print_timing_table_stdout("Generator", r.java.as_ref(), &r.rust);
            if let Some(ratio) = r.java_over_rust_median {
                println!();
                println!(
                    "  Generator/Rust median ratio: {ratio:.3}x (JVM startup included unless warmup is high)"
                );
            }
            println!();
            println!("  Compare detail (JSON):");
            let json = serde_json::to_string_pretty(&r.compare)?;
            for line in json.lines() {
                println!("    {line}");
            }
            if !cli.no_diff {
                print_compare_divergence(&r.compare);
            }
        }
    }

    let failed = reports
        .iter()
        .any(super::harness::ScenarioHarnessReport::comparison_failed);
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

fn run_fuzz_branch(cli: &TopCli) -> Result<(), Box<dyn std::error::Error>> {
    let fo = &cli.fuzz_opts;
    let cmp = &cli.compare;
    let fail_fast = !fo.fuzz_continue_on_error;

    let parity_profile = fo.fuzz_parity;
    let chaos = fo.fuzz_chaos && !parity_profile;
    let jitter_all = (fo.fuzz_jitter_all || chaos) && !parity_profile;
    let scenarios_recursive = fo.fuzz_scenarios_recursive || chaos;
    let mutate_ai_level = if chaos {
        fo.fuzz_mutate_ai_level.max(4)
    } else {
        fo.fuzz_mutate_ai_level
    };
    let allow_external_ais = fo.fuzz_allow_external_ais || chaos;
    let jitter_entity_stats = fo.fuzz_jitter_entity_stats || jitter_all;
    let jitter_max_turns = fo.fuzz_jitter_max_turns || jitter_all;
    let randomize_draw_rule = fo.fuzz_randomize_draw_rule || jitter_all;
    let jitter_map = fo.fuzz_jitter_map || jitter_all;
    let jitter_entity_cells = fo.fuzz_jitter_entity_cells || jitter_all;
    let jitter_max_ops = fo.fuzz_jitter_max_ops || jitter_all;
    let jitter_loadout = fo.fuzz_jitter_loadout || jitter_all;

    let mutate_ai_require_parseable = fo.fuzz_mutate_ai_require_parseable || parity_profile;

    let benchmark_java_effective = fo.fuzz_benchmark_java || parity_profile;
    if benchmark_java_effective && (cmp.skip_java || cmp.skip_rust) {
        return Err(Box::new(GenError::Message(
                "--fuzz-benchmark-java requires both the official generator and Rust (do not pass --skip-java or --skip-rust)"
                    .into(),
            )));
    }
    let root = generator_root(fo.fuzz_root.as_ref())?;
    if !root.is_dir() {
        return Err(Box::new(GenError::Message(format!(
            "generator root not found: {}",
            root.display()
        ))));
    }

    let mut scenario_rels =
        discover_fuzz_scenario_rels(&root, &fo.fuzz_scenarios_dir, scenarios_recursive)?;
    let mut seen: HashSet<PathBuf> = scenario_rels.iter().cloned().collect();
    if !fo.fuzz_no_parity_corpus {
        merge_parity_corpus_scenarios(&root, &mut scenario_rels, &mut seen);
    }
    for p in &fo.fuzz_scenarios {
        if seen.insert(p.clone()) {
            scenario_rels.push(p.clone());
        }
    }
    if scenario_rels.is_empty() {
        return Err(Box::new(GenError::Message(
            "no scenarios found (check --fuzz-root and --fuzz-scenarios-dir)".into(),
        )));
    }

    let mut ai_rels = discover_ai_rel_paths(&root, &fo.fuzz_ai_dir)?;
    for a in &fo.fuzz_ais {
        if !ai_rels.contains(a) {
            ai_rels.push(a.clone());
        }
    }
    // External AIs are materialized into overlays/sandboxes under `_external_ai/…` and added to the pool.
    // They are not required to live under the generator root.
    // Note: relative paths are resolved from the current working directory.
    // (We only add the rel paths here; copying happens in the fuzz runner.)
    // This makes `--fuzz-no-shuffle-ai` still usable with external AIs if scenarios reference them explicitly.
    fn collect_leek_files(
        dir: &std::path::Path,
        out: &mut Vec<std::path::PathBuf>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() {
                collect_leek_files(&p, out)?;
            } else if p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("leek"))
            {
                out.push(p);
            }
        }
        Ok(())
    }

    let mut external_files: Vec<PathBuf> = fo.fuzz_ai_files.clone();
    external_files.sort();
    external_files.dedup();

    // Add rel paths for standalone external files (copied under `_external_ai/_files/`).
    let external_rel_paths = external_files
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let base = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("external.leek");
            let mut name = base.to_string();
            if !name.ends_with(".leek") {
                name.push_str(".leek");
            }
            format!("_external_ai/_files/{i:03}_{name}")
        })
        .collect::<Vec<_>>();

    // Add rel paths for directory-imported AIs (mirrored under `_external_ai/<dir_name>/...`).
    // We only use these for shuffling; actual copying happens inside the fuzz runner.
    let mut external_dir_rel_paths: Vec<String> = Vec::new();
    for d in &fo.fuzz_ai_files_dirs {
        if !d.is_dir() {
            continue;
        }
        let dir_name = d.file_name().and_then(|s| s.to_str()).unwrap_or("dir");
        let mut files = Vec::new();
        collect_leek_files(d.as_path(), &mut files)?;
        files.sort();
        for p in files {
            if let Ok(rel) = p.strip_prefix(d) {
                let rel_posix = rel.to_string_lossy().replace('\\', "/");
                let is_entry = rel_posix.ends_with("/main.leek")
                    || rel_posix == "main.leek"
                    || rel_posix.ends_with("/merged-for-upload.leek")
                    || rel_posix == "merged-for-upload.leek";
                if !fo.fuzz_ai_entrypoints_only || is_entry {
                    external_dir_rel_paths.push(format!("_external_ai/{dir_name}/{rel_posix}"));
                }
            }
        }
    }
    for rel in external_rel_paths
        .iter()
        .chain(external_dir_rel_paths.iter())
    {
        if !ai_rels.contains(rel) {
            ai_rels.push(rel.clone());
        }
    }
    ai_rels.sort();
    ai_rels.dedup();
    if ai_rels.is_empty() && !fo.fuzz_no_shuffle_ai {
        return Err(Box::new(GenError::Message(
            "no .leek files found (check --fuzz-ai-dir / --fuzz-ai); pass --fuzz-no-shuffle-ai if you only fuzz seeds"
                .into(),
        )));
    }

    let master_seed = fo
        .fuzz_master_seed
        .unwrap_or_else(|| rand::thread_rng().gen());

    let harness_cfg_store: Option<HarnessRunConfig> = if benchmark_java_effective {
        let jar = cmp
            .jar
            .clone()
            .or_else(|| resolve_generator_jar().ok())
            .ok_or(GenError::JarNotFound)?;
        let cwd = cmp.cwd.clone().unwrap_or_else(|| default_java_cwd(&jar));
        if !cwd.is_dir() {
            return Err(Box::new(GenError::CwdMissing(cwd)));
        }
        let java_cfg = JavaEngineConfig {
            jar,
            cwd,
            java_bin: cmp.java_bin.clone().unwrap_or_else(java_bin),
        };
        Some(HarnessRunConfig {
            java: java_cfg,
            mode: fuzz_harness_compare_mode(fo.fuzz_compare_mode),
            warmup: 0,
            iterations: cmp.iterations.max(1),
            run_java: true,
            run_rust: true,
            runtime_cwd: None,
        })
    } else {
        None
    };

    let root_for_report = root.clone();
    let scenario_pool_n = scenario_rels.len();
    let ai_pool_n = ai_rels.len();

    let run_until_interrupt = fo.fuzz_iterations == 0;
    let progress_report_every = if fo.fuzz_quiet {
        0
    } else if let Some(n) = fo.fuzz_progress_every {
        n
    } else if run_until_interrupt {
        1
    } else {
        10
    };

    let cfg = FuzzConfig {
        generator_root: root,
        scenario_rels,
        ai_rels,
        iterations: fo.fuzz_iterations,
        master_seed,
        fuzz_random_seed: !fo.fuzz_no_fuzz_seed,
        shuffle_ais: !fo.fuzz_no_shuffle_ai,
        restrict_ais_to_scenario: !allow_external_ais,
        keep_temps: fo.fuzz_keep_temps,
        mutate_ai_level,
        mutate_ai_inject_complexity: fo.fuzz_mutate_ai_inject_complexity,
        mutate_ai_inject_wrap_percent: fo.fuzz_mutate_ai_inject_wrap_percent,
        mutate_ai_inject_max_stmts: fo.fuzz_mutate_ai_inject_max_stmts,
        mutate_ai_require_parseable,
        jitter_entity_stats,
        jitter_max_turns,
        jitter_map_obstacles: jitter_map,
        jitter_entity_cells,
        jitter_max_operations: jitter_max_ops,
        jitter_entity_loadouts: jitter_loadout,
        fuzz_draw_check_life: randomize_draw_rule,
        ai_mirror_rel: fo.fuzz_ai_dir.clone(),
        progress_report_every,
        artifacts_dir: fo
            .fuzz_artifacts_dir
            .as_ref()
            .map(|p| resolve_fuzz_artifacts_dir(p.as_path())),
        fuzz_parity: parity_profile,
        generate_scenarios: fo.fuzz_generate_scenarios || chaos,
        generate_scenarios_percent: if chaos {
            fo.fuzz_generate_scenarios_percent.max(25)
        } else {
            fo.fuzz_generate_scenarios_percent
        },
        external_ai_files: external_files,
        external_ai_dirs: fo.fuzz_ai_files_dirs.clone(),
        gen_min_entities_per_team: fo.fuzz_gen_min_entities_per_team,
        gen_max_entities_per_team: fo.fuzz_gen_max_entities_per_team,
        gen_min_map: fo.fuzz_gen_min_map,
        gen_max_map: fo.fuzz_gen_max_map,
        gen_loadout_percent: fo.fuzz_gen_loadout_percent,
        require_compilable_ai: fo.fuzz_require_compilable_ai && !parity_profile,
        mutate_ramp: fo.fuzz_mutate_ramp && !parity_profile,
        mutate_ramp_every: fo.fuzz_mutate_ramp_every.max(1),
    };
    let stop_flag: Option<Arc<AtomicBool>> = if run_until_interrupt {
        let s = Arc::new(AtomicBool::new(false));
        let s2 = Arc::clone(&s);
        if let Err(e) = ctrlc::set_handler(move || {
            if s2.swap(true, Ordering::SeqCst) {
                eprintln!("\nleekgen-compare: second interrupt — exiting.");
                std::process::exit(130);
            }
            eprintln!(
                "\nleekgen-compare: interrupt — will stop before the next fuzz iteration, then print results."
            );
        }) {
            eprintln!(
                "leekgen-compare: warning: could not install Ctrl+C handler ({e}); stop with kill."
            );
        }
        Some(s)
    } else {
        None
    };

    if run_until_interrupt && !fo.fuzz_quiet {
        if progress_report_every > 0 {
            eprintln!(
                "leekgen-compare: --fuzz-n 0 — infinite fuzz until first Ctrl+C. Live progress every {progress_report_every} attempt(s) on stderr."
            );
        } else {
            eprintln!(
                "leekgen-compare: --fuzz-n 0 — infinite fuzz until first Ctrl+C (no live progress; pass e.g. `--fuzz-progress-every 1`)."
            );
        }
    }

    let bench = run_fuzz_timed(
        &cfg,
        fail_fast,
        harness_cfg_store.as_ref(),
        stop_flag.as_deref(),
    );

    let fuzz_compare_mode_label: Option<&'static str> = if benchmark_java_effective {
        Some(match fo.fuzz_compare_mode {
            FuzzCompareModeArg::Full => "full_normalized",
            FuzzCompareModeArg::Winner => "winner_duration",
            FuzzCompareModeArg::ActionsMinimal => "actions_minimal",
            FuzzCompareModeArg::ActionsExact => "actions_exact",
            FuzzCompareModeArg::OpsExact => "ops_exact",
        })
    } else {
        None
    };

    if !fo.fuzz_quiet {
        let failed = bench.summary.failures.len();
        let ok = bench.summary.iterations_ok;
        print_fuzz_banner_stderr(FuzzBannerParams {
            root: root_for_report.as_path(),
            requested: fo.fuzz_iterations,
            run_until_interrupt,
            ok,
            failed,
            master_seed,
            scenario_count: scenario_pool_n,
            ai_count: ai_pool_n,
            scenarios_recursive,
            parity_corpus_merged: !fo.fuzz_no_parity_corpus,
            fuzz_seed: !fo.fuzz_no_fuzz_seed,
            shuffle_ais: !fo.fuzz_no_shuffle_ai,
            benchmark_java: benchmark_java_effective,
            parity_profile,
            fuzz_compare_mode: fuzz_compare_mode_label,
            mutate_ai_level: cfg.mutate_ai_level,
            jitter_entity_stats: cfg.jitter_entity_stats,
            jitter_max_turns: cfg.jitter_max_turns,
            jitter_map: cfg.jitter_map_obstacles,
            jitter_cells: cfg.jitter_entity_cells,
            jitter_max_ops: cfg.jitter_max_operations,
            jitter_loadout: cfg.jitter_entity_loadouts,
            mutate_ai_inject_complexity: cfg.mutate_ai_inject_complexity,
            mutate_ai_inject_wrap_percent: cfg.mutate_ai_inject_wrap_percent,
            mutate_ai_inject_max_stmts: cfg.mutate_ai_inject_max_stmts,
            mutate_ai_require_parseable: cfg.mutate_ai_require_parseable,
            randomize_draw_rule: cfg.fuzz_draw_check_life,
            artifacts_dir: cfg.artifacts_dir.as_deref(),
        });
        print_fuzz_results_stderr(&bench, benchmark_java_effective, fuzz_compare_mode_label);
    }

    if !bench.summary.failures.is_empty() {
        section_stderr("failures");
        for (i, f) in bench.summary.failures.iter().enumerate() {
            eprintln!(
                "  [{}] iter {}  template {}",
                i + 1,
                f.iteration,
                f.scenario_template.display()
            );
            if let Some(ref dir) = f.artifact_dir {
                eprintln!("      repro {}", dir.join("scenario.json").display());
            } else if !f.temp_path.as_os_str().is_empty() {
                eprintln!("      temp {}", f.temp_path.display());
            }
            eprintln!("      {}", f.error);
        }
    }

    if !bench.fight_ok() {
        return Err(Box::new(GenError::Message(format!(
            "fuzz: {} run(s) failed (see stderr)",
            bench.summary.failures.len()
        ))));
    }
    if benchmark_java_effective && !bench.compare_ok() {
        return Err(Box::new(GenError::Message(format!(
            "fuzz: {} official-generator/Rust compare mismatch(es) (--fuzz-compare-mode {:?}; default is full outcome parity except timing/logs normalization; try --fuzz-parity for fixture-stable inputs, or --fuzz-compare-mode winner for a looser check)",
            bench.parity_mismatches,
            fo.fuzz_compare_mode
        ))));
    }
    Ok(())
}

/// Entry for `leekgen-compare` and `leekgen-fuzz` (after optional argv rewriting).
pub fn run(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cli = TopCli::parse_from(args);
    if cli.fuzz_opts.replay.is_some() {
        run_replay_branch(&cli)
    } else if cli.fuzz_opts.fuzz {
        run_fuzz_branch(&cli)
    } else {
        run_compare(&cli.compare)
    }
}

fn compare_mode_from_meta(s: &str) -> CompareMode {
    match s {
        "winner_duration" => CompareMode::WinnerDuration,
        "actions_minimal" => CompareMode::ActionsMinimal,
        "actions_exact" => CompareMode::ActionsExact,
        "ops_exact" => CompareMode::OpsExact,
        _ => CompareMode::FullNormalized,
    }
}

fn run_replay_branch(cli: &TopCli) -> Result<(), Box<dyn std::error::Error>> {
    let fo = &cli.fuzz_opts;
    let cmp = &cli.compare;
    let dir = fo
        .replay
        .as_ref()
        .expect("checked replay is Some in caller");
    let artifact_dir = if dir.is_absolute() {
        dir.clone()
    } else {
        std::env::current_dir().map_or_else(|_| dir.clone(), |cwd| cwd.join(dir))
    };

    let meta_raw = fs::read_to_string(artifact_dir.join("meta.json"))?;
    let meta: serde_json::Value = serde_json::from_str(&meta_raw)?;
    let benchmark_java = meta
        .get("benchmark_java")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let compare_mode = meta
        .get("compare_mode")
        .and_then(|v| v.as_str())
        .map_or(CompareMode::FullNormalized, compare_mode_from_meta);

    let root = generator_root(fo.fuzz_root.as_ref())?;

    let harness_cfg_store: Option<HarnessRunConfig> = if benchmark_java {
        if cmp.skip_java || cmp.skip_rust {
            return Err(Box::new(GenError::Message(
                "--replay requires both the official generator and Rust for this artifact (do not pass --skip-java or --skip-rust)"
                    .into(),
            )));
        }
        let jar = cmp
            .jar
            .clone()
            .or_else(|| resolve_generator_jar().ok())
            .ok_or(GenError::JarNotFound)?;
        let cwd = cmp.cwd.clone().unwrap_or_else(|| default_java_cwd(&jar));
        if !cwd.is_dir() {
            return Err(Box::new(GenError::CwdMissing(cwd)));
        }
        let java_cfg = JavaEngineConfig {
            jar,
            cwd,
            java_bin: cmp.java_bin.clone().unwrap_or_else(java_bin),
        };
        Some(HarnessRunConfig {
            java: java_cfg,
            mode: compare_mode,
            warmup: 0,
            iterations: cmp.iterations.max(1),
            run_java: true,
            run_rust: true,
            runtime_cwd: None,
        })
    } else {
        None
    };

    crate::fuzz::replay_fuzz_artifact_dir(
        &artifact_dir,
        &root,
        harness_cfg_store.as_ref(),
        fo.fuzz_keep_temps,
    )?;
    Ok(())
}
