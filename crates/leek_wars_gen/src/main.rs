//! `leekgen` — Rust-only Leek Wars generator CLI.
//!
//! This binary is intentionally **Rust engine only** (no JVM, no `generator.jar`). Java parity tooling
//! lives in separate binaries (`leekgen-compare`, `leekgen-fuzz`).

use clap::{Args, Parser, Subcommand, ValueEnum};
use leek_wars_gen::engine::RunRequest;
use leek_wars_gen::{GenError, RustEngine};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "leekgen", version, about = "Leek Wars fight generator CLI (Rust-only)")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a scenario and print the outcome.
    Run(RunCmd),
    /// Simulation view (alias for `run --output pretty --sim`).
    Sim(SimCmd),
    /// Run randomized variants over a scenario pool (Rust engine).
    Fuzz(FuzzCmd),
    /// Replay a fuzz artifact directory (Rust engine).
    Replay(ReplayCmd),
    /// Scenario file utilities (list/validate/convert/new/print).
    Scenario(ScenarioTop),
    /// Show the resolved `Leek.toml` generator configuration.
    Config(ConfigCmd),
    /// Print (or write) a starter `[generator]` config section.
    Init(InitCmd),
    /// Benchmark Rust engine performance on a scenario.
    Bench(BenchCmd),
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    /// Human-readable summary (default).
    #[default]
    Pretty,
    /// Raw outcome JSON to stdout.
    Json,
    /// One JSON object per run (metadata + outcome).
    Ndjson,
}

#[derive(Args)]
struct RunCmd {
    /// Scenario file (`.json` or `.toml`).
    scenario: PathBuf,

    /// Root directory for AI/data resolution (equivalent to `LEEK_GENERATOR_CWD`).
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum)]
    output: Option<OutputFormat>,

    /// Print a deterministic, readable playback derived from the outcome JSON.
    #[arg(long)]
    sim: bool,

    /// Save a repro bundle (config + scenario + outcome) under this directory.
    #[arg(long, value_name = "DIR")]
    save_repro: Option<PathBuf>,

    /// Simulation: group actions by global turn (best-effort).
    #[arg(long)]
    sim_group_turns: bool,

    /// Simulation: pretty (LW-inspired) or raw (debug).
    #[arg(long, value_enum, default_value_t = SimStyleArg::Pretty)]
    sim_style: SimStyleArg,

    /// Simulation: brief (high-signal only), normal, or verbose.
    #[arg(long, value_enum, default_value_t = SimVerbosityArg::Normal)]
    sim_verbosity: SimVerbosityArg,

    /// Simulation: only include actions with this action code.
    #[arg(long, value_name = "CODE")]
    sim_only_code: Option<i64>,

    /// Simulation: only include actions attributed to this entity id (fid).
    #[arg(long, value_name = "FID")]
    sim_only_fid: Option<i64>,

    /// Simulation: maximum number of action lines to print.
    #[arg(long, value_name = "N")]
    sim_limit: Option<usize>,

    /// Simulation: stable, diff-friendly line format.
    #[arg(long)]
    sim_diff: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum SimStyleArg {
    Pretty,
    Raw,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum SimVerbosityArg {
    Brief,
    Normal,
    Verbose,
}

#[derive(Args)]
struct SimCmd {
    /// Scenario file (`.json` or `.toml`).
    scenario: PathBuf,

    /// Root directory for AI/data resolution (equivalent to `LEEK_GENERATOR_CWD`).
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Simulation: group actions by global turn (best-effort).
    #[arg(long)]
    group_turns: bool,

    /// Simulation: pretty (LW-inspired) or raw (debug).
    #[arg(long, value_enum, default_value_t = SimStyleArg::Pretty)]
    style: SimStyleArg,

    /// Simulation: brief (high-signal only), normal, or verbose.
    #[arg(long, value_enum, default_value_t = SimVerbosityArg::Normal)]
    verbosity: SimVerbosityArg,

    /// Simulation: only include actions with this action code.
    #[arg(long, value_name = "CODE")]
    only_code: Option<i64>,

    /// Simulation: only include actions attributed to this entity id (fid).
    #[arg(long, value_name = "FID")]
    only_fid: Option<i64>,

    /// Simulation: maximum number of action lines to print.
    #[arg(long, value_name = "N")]
    limit: Option<usize>,

    /// Simulation: stable, diff-friendly line format.
    #[arg(long)]
    diff: bool,
}

#[derive(Args)]
struct FuzzCmd {
    /// Root directory for AI/data resolution (equivalent to `LEEK_GENERATOR_CWD`).
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Scenario directory relative to root (default: `test/scenario`).
    #[arg(long, value_name = "DIR")]
    scenarios_dir: Option<PathBuf>,

    /// Walk subdirectories of `--scenarios-dir` when discovering scenarios.
    #[arg(long)]
    recursive: bool,

    /// Number of iterations (`0` = run until Ctrl+C).
    #[arg(long, default_value_t = 100)]
    n: u64,

    /// Master seed for deterministic fuzzing.
    #[arg(long)]
    master_seed: Option<u64>,

    /// Mutate `.leek` sources each iteration (`0..=4`).
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(0..=4))]
    mutate_ai_level: u8,

    /// Enable random seed fuzzing (overwrites `random_seed`).
    #[arg(long, default_value_t = true)]
    fuzz_seed: bool,

    /// Shuffle `.leek` AIs per entity each iteration.
    #[arg(long, default_value_t = true)]
    shuffle_ai: bool,

    /// Print progress every N completed iterations (`0` = never).
    #[arg(long)]
    progress_every: Option<u64>,

    /// Save repro bundles on failures.
    #[arg(long, value_name = "DIR")]
    artifacts_dir: Option<PathBuf>,

    /// Keep temp JSON / overlays (debugging).
    #[arg(long)]
    keep_temps: bool,

    /// Quieter stderr (no banner; progress defaults to 0).
    #[arg(long)]
    quiet: bool,

    /// Output format for per-iteration results (only affects successes).
    #[arg(long, value_enum)]
    output: Option<OutputFormat>,
}

#[derive(Args)]
struct ReplayCmd {
    /// Artifact bundle directory containing `scenario.json` and `meta.json`.
    artifact_dir: PathBuf,

    /// Root directory for AI/data resolution (equivalent to `LEEK_GENERATOR_CWD`).
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Keep temp overlay trees created during replay (debugging).
    #[arg(long)]
    keep_temps: bool,

    /// Output format.
    #[arg(long, value_enum)]
    output: Option<OutputFormat>,
}

#[derive(Subcommand)]
enum ScenarioCmd {
    List(ScenarioListCmd),
    Validate(ScenarioValidateCmd),
    Convert(ScenarioConvertCmd),
    Print(ScenarioPrintCmd),
    New(ScenarioNewCmd),
    Doctor(ScenarioDoctorCmd),
    Normalize(ScenarioNormalizeCmd),
}

#[derive(Args)]
struct ScenarioTop {
    #[command(subcommand)]
    cmd: ScenarioCmd,
}

#[derive(Args)]
struct ScenarioListCmd {
    /// Root directory for scenario discovery (defaults from config / env).
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,
    /// Scenario directory relative to root (default: `test/scenario`).
    #[arg(long, value_name = "DIR")]
    dir: Option<PathBuf>,
    /// Walk subdirectories.
    #[arg(long)]
    recursive: bool,
}

#[derive(Args)]
struct ScenarioValidateCmd {
    path: PathBuf,
    #[arg(long)]
    recursive: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum ScenarioFormat {
    Json,
    Toml,
}

#[derive(Args)]
struct ScenarioConvertCmd {
    input: PathBuf,
    #[arg(long, value_enum)]
    to: ScenarioFormat,
    #[arg(long, value_name = "PATH")]
    out: Option<PathBuf>,
}

#[derive(Args)]
struct ScenarioPrintCmd {
    input: PathBuf,
    /// Print normalized (sorted) JSON (useful for diffs).
    #[arg(long)]
    normalized: bool,
}

#[derive(Args)]
struct ScenarioNewCmd {
    #[arg(long, value_enum, default_value_t = ScenarioFormat::Toml)]
    format: ScenarioFormat,
    #[arg(long, value_name = "PATH")]
    out: Option<PathBuf>,
}

#[derive(Args)]
struct ScenarioDoctorCmd {
    path: PathBuf,
    /// Walk subdirectories when `path` is a directory.
    #[arg(long)]
    recursive: bool,
    /// Apply safe fixes in-place (otherwise report only).
    #[arg(long)]
    fix: bool,
}

#[derive(Args)]
struct ScenarioNormalizeCmd {
    path: PathBuf,
    /// Walk subdirectories when `path` is a directory.
    #[arg(long)]
    recursive: bool,
    /// Write normalized output back to files (otherwise print to stdout for single file).
    #[arg(long)]
    in_place: bool,
}

#[derive(Args)]
struct ConfigCmd {
    /// Print as JSON instead of pretty text.
    #[arg(long)]
    json: bool,

    /// Show where each value came from (CLI/env/manifest/default).
    #[arg(long)]
    explain: bool,
}

#[derive(Args)]
struct InitCmd {
    /// Write to the nearest `Leek.toml` (otherwise print to stdout).
    #[arg(long)]
    write: bool,
}

#[derive(Args)]
struct BenchCmd {
    /// Scenario file path or fuzzy name (resolved under `scenarios_dir`).
    scenario: PathBuf,

    /// Root directory for AI/data resolution (equivalent to `LEEK_GENERATOR_CWD`).
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Warmup runs (not recorded).
    #[arg(long, default_value_t = 3)]
    warmup: usize,

    /// Timed iterations.
    #[arg(long, default_value_t = 20)]
    iters: usize,

    /// Output format.
    #[arg(long, value_enum)]
    output: Option<OutputFormat>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Run(cmd) => cmd_run(cmd)?,
        Command::Sim(cmd) => cmd_sim(cmd)?,
        Command::Fuzz(cmd) => cmd_fuzz(cmd)?,
        Command::Replay(cmd) => cmd_replay(cmd)?,
        Command::Scenario(cmd) => cmd_scenario(cmd.cmd)?,
        Command::Config(cmd) => cmd_config(cmd)?,
        Command::Init(cmd) => cmd_init(cmd)?,
        Command::Bench(cmd) => cmd_bench(cmd)?,
    }
    Ok(())
}

fn cmd_run(cmd: RunCmd) -> Result<(), GenError> {
    let cfg = leek_wars_gen::config::resolve(cmd.root)?;
    let output = cmd.output.map(map_output).unwrap_or(cfg.output);
    let sim_data = load_sim_data(cfg.root.as_path());

    let scenario_arg_for_meta = cmd.scenario.clone();
    let scenario_input = resolve_scenario_arg(&cfg, cmd.scenario)?;
    let (scenario_json_path, _guard) =
        leek_wars_gen::scenario_io::materialize_json_path(scenario_input.as_path())?;
    let scenario_json_path = scenario_path_for_engine(scenario_json_path, cfg.root.as_path());
    let scenario_json_for_bundle = read_scenario_json_for_bundle(&scenario_json_path, cfg.root.as_path())?;

    let req = RunRequest {
        file: scenario_json_path,
        ..Default::default()
    };
    let engine = RustEngine;
    let out = engine.run_scenario_with_cwd(&req, cfg.root.as_path())?;

    if let Some(dir) = cmd.save_repro.as_ref() {
        save_repro_bundle(
            dir.as_path(),
            &cfg,
            &scenario_arg_for_meta,
            &scenario_json_for_bundle,
            &out,
            cmd.sim,
            cmd.sim_style,
            cmd.sim_verbosity,
            cmd.sim_group_turns,
            cmd.sim_only_code,
            cmd.sim_only_fid,
            cmd.sim_limit,
            cmd.sim_diff,
            sim_data.as_ref(),
        )?;
    }

    match output {
        leek_wars_gen::config::OutputFormat::Json => {
            print!("{out}");
        }
        leek_wars_gen::config::OutputFormat::Pretty => {
            let outcome = leek_wars_gen::output::parse_outcome(&out)?;
            print!("{}", leek_wars_gen::output::pretty_summary(&outcome));
            if cmd.sim {
                println!();
                let sim_opts = leek_wars_gen::output::SimOptions {
                    only_code: cmd.sim_only_code,
                    only_fid: cmd.sim_only_fid,
                    limit: cmd.sim_limit,
                    diff_friendly: cmd.sim_diff,
                    group_turns: cmd.sim_group_turns,
                    style: match cmd.sim_style {
                        SimStyleArg::Pretty => leek_wars_gen::output::SimStyle::Pretty,
                        SimStyleArg::Raw => leek_wars_gen::output::SimStyle::Raw,
                    },
                    show_indices: cmd.sim_diff,
                    verbosity: match cmd.sim_verbosity {
                        SimVerbosityArg::Brief => leek_wars_gen::output::SimVerbosity::Brief,
                        SimVerbosityArg::Normal => leek_wars_gen::output::SimVerbosity::Normal,
                        SimVerbosityArg::Verbose => leek_wars_gen::output::SimVerbosity::Verbose,
                    },
                    fid_names: None,
                    data: sim_data.clone(),
                    show_live_stats: true,
                };
                print!("{}", leek_wars_gen::output::sim_text_with_options(&outcome, &sim_opts));
            }
        }
        leek_wars_gen::config::OutputFormat::Ndjson => {
            let outcome: serde_json::Value =
                serde_json::from_str(&out).map_err(|e| GenError::Message(e.to_string()))?;
            let env = serde_json::json!({
                "kind": "leekgen_run",
                "scenario": scenario_arg_for_meta.display().to_string(),
                "root": cfg.root.display().to_string(),
                "outcome": outcome,
            });
            println!(
                "{}",
                serde_json::to_string(&env).map_err(|e| GenError::Message(e.to_string()))?
            );
        }
    }

    Ok(())
}

fn scenario_path_for_engine(path: PathBuf, root: &std::path::Path) -> PathBuf {
    // The Rust engine treats relative paths as relative to `root` (ai_base) and joins them.
    // If the user already passed a path that is effectively `root/...` but still relative,
    // we strip the `root` prefix so we don't end up with `root/root/...`.
    if path.is_absolute() {
        return path;
    }
    let Ok(cwd) = std::env::current_dir() else {
        return path;
    };
    let root_abs = if root.is_absolute() { root.to_path_buf() } else { cwd.join(root) };
    let path_abs = cwd.join(&path);
    if let Ok(rel) = path_abs.strip_prefix(&root_abs) {
        if !rel.as_os_str().is_empty() {
            return rel.to_path_buf();
        }
    }
    path
}

fn cmd_fuzz(cmd: FuzzCmd) -> Result<(), GenError> {
    use rand::Rng;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let cfg = leek_wars_gen::config::resolve(cmd.root)?;
    let root = cfg.root.clone();
    if !root.is_dir() {
        return Err(GenError::Message(format!(
            "root is not a directory: {}",
            root.display()
        )));
    }

    let scenarios_dir = cmd.scenarios_dir.unwrap_or_else(|| cfg.scenarios_dir.clone());
    let scenario_rels = leek_wars_gen::fuzz::discover_fuzz_scenario_rels(
        root.as_path(),
        scenarios_dir.as_path(),
        cmd.recursive,
    )
    .map_err(GenError::from)?;
    if scenario_rels.is_empty() {
        return Err(GenError::Message(format!(
            "no scenarios found under {} (recursive={})",
            scenarios_dir.display(),
            cmd.recursive
        )));
    }

    let ai_rels = leek_wars_gen::fuzz::discover_ai_rel_paths(root.as_path(), cfg.ai_dir.as_path())
        .map_err(GenError::from)?;

    let master_seed = cmd
        .master_seed
        .unwrap_or_else(|| rand::thread_rng().gen::<u64>());

    let run_until_interrupt = cmd.n == 0;
    let progress_every = if cmd.quiet {
        0
    } else if let Some(n) = cmd.progress_every {
        n
    } else if run_until_interrupt {
        1
    } else {
        10
    };

    let fuzz_cfg = leek_wars_gen::fuzz::FuzzConfig {
        generator_root: root.clone(),
        scenario_rels,
        ai_rels,
        iterations: cmd.n,
        master_seed,
        fuzz_random_seed: cmd.fuzz_seed,
        shuffle_ais: cmd.shuffle_ai,
        restrict_ais_to_scenario: true,
        keep_temps: cmd.keep_temps,
        mutate_ai_level: cmd.mutate_ai_level,
        mutate_ai_inject_complexity: 2,
        mutate_ai_inject_wrap_percent: 55,
        mutate_ai_inject_max_stmts: 3,
        mutate_ai_require_parseable: false,
        jitter_entity_stats: false,
        jitter_max_turns: false,
        jitter_map_obstacles: false,
        jitter_entity_cells: false,
        jitter_max_operations: false,
        jitter_entity_loadouts: false,
        fuzz_draw_check_life: false,
        ai_mirror_rel: cfg.ai_dir.clone(),
        progress_report_every: progress_every,
        artifacts_dir: cmd.artifacts_dir.clone(),
        fuzz_parity: false,
        generate_scenarios: false,
        generate_scenarios_percent: 0,
        external_ai_files: vec![],
        external_ai_dirs: vec![],
        gen_min_entities_per_team: 1,
        gen_max_entities_per_team: 4,
        gen_min_map: 9,
        gen_max_map: 25,
        gen_loadout_percent: 35,
        require_compilable_ai: false,
        mutate_ramp: false,
        mutate_ramp_every: 200,
    };

    let stop_flag: Option<Arc<AtomicBool>> = if run_until_interrupt {
        let s = Arc::new(AtomicBool::new(false));
        let s2 = Arc::clone(&s);
        let _ = ctrlc::set_handler(move || {
            s2.store(true, Ordering::SeqCst);
        });
        Some(s)
    } else {
        None
    };

    let bench = leek_wars_gen::fuzz::run_fuzz_timed(
        &fuzz_cfg,
        true,
        None,
        stop_flag.as_deref(),
    );

    let out_fmt = cmd.output.unwrap_or_else(|| map_output_back(cfg.output));
    match out_fmt {
        OutputFormat::Pretty => {
            eprintln!();
            eprintln!("── fuzz summary ──");
            eprintln!("  root          {}", root.display());
            eprintln!("  master_seed   {master_seed}");
            eprintln!(
                "  ok            {}  failed {}",
                bench.summary.iterations_ok,
                bench.summary.failures.len()
            );
            eprintln!(
                "  wall_ms       median {:.2}ms  mean {:.2}ms  n={}",
                bench.rust_wall_ms.median_ms,
                bench.rust_wall_ms.mean_ms,
                bench.rust_wall_ms.iterations
            );
        }
        OutputFormat::Json => {
            let v = serde_json::json!({
                "master_seed": bench.summary.master_seed,
                "ok": bench.summary.iterations_ok,
                "failures": bench.summary.failures.len(),
                "rust_wall_ms": bench.rust_wall_ms,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&v).map_err(|e| GenError::Message(e.to_string()))?
            );
        }
        OutputFormat::Ndjson => {
            let v = serde_json::json!({
                "kind": "leekgen_fuzz_summary",
                "root": root.display().to_string(),
                "master_seed": bench.summary.master_seed,
                "ok": bench.summary.iterations_ok,
                "failures": bench.summary.failures.len(),
                "rust_wall_ms": bench.rust_wall_ms,
            });
            println!(
                "{}",
                serde_json::to_string(&v).map_err(|e| GenError::Message(e.to_string()))?
            );
        }
    }

    if !bench.summary.failures.is_empty() {
        return Err(GenError::Message(format!(
            "fuzz: {} run(s) failed",
            bench.summary.failures.len()
        )));
    }
    Ok(())
}

fn cmd_sim(cmd: SimCmd) -> Result<(), GenError> {
    let cfg = leek_wars_gen::config::resolve(cmd.root)?;
    let sim_data = load_sim_data(cfg.root.as_path());
    let scenario_input = resolve_scenario_arg(&cfg, cmd.scenario)?;
    let (scenario_json_path, _guard) =
        leek_wars_gen::scenario_io::materialize_json_path(scenario_input.as_path())?;
    let scenario_json_path = scenario_path_for_engine(scenario_json_path, cfg.root.as_path());

    let req = RunRequest {
        file: scenario_json_path,
        ..Default::default()
    };
    let engine = RustEngine;
    let out = engine.run_scenario_with_cwd(&req, cfg.root.as_path())?;
    let outcome = leek_wars_gen::output::parse_outcome(&out)?;
    let sim_opts = leek_wars_gen::output::SimOptions {
        only_code: cmd.only_code,
        only_fid: cmd.only_fid,
        limit: cmd.limit,
        diff_friendly: cmd.diff,
        group_turns: cmd.group_turns,
        style: match cmd.style {
            SimStyleArg::Pretty => leek_wars_gen::output::SimStyle::Pretty,
            SimStyleArg::Raw => leek_wars_gen::output::SimStyle::Raw,
        },
        show_indices: cmd.diff,
        verbosity: match cmd.verbosity {
            SimVerbosityArg::Brief => leek_wars_gen::output::SimVerbosity::Brief,
            SimVerbosityArg::Normal => leek_wars_gen::output::SimVerbosity::Normal,
            SimVerbosityArg::Verbose => leek_wars_gen::output::SimVerbosity::Verbose,
        },
        fid_names: None,
        data: sim_data,
        show_live_stats: true,
    };
    print!("{}", leek_wars_gen::output::sim_text_with_options(&outcome, &sim_opts));
    Ok(())
}

fn load_sim_data(root: &std::path::Path) -> Option<leek_wars_gen::output::SimData> {
    let chips = leek_wars_gen::fight::load_chips_json(&root.join("data/chips.json")).ok()?;
    let weapons = leek_wars_gen::fight::load_weapons_json(&root.join("data/weapons.json")).ok()?;

    let mut out = leek_wars_gen::output::SimData::default();
    for (_id, c) in chips {
        out.chip_name_by_template
            .insert(c.template_id as i64, c.name);
    }
    for (_tpl, w) in weapons {
        out.weapon_name_by_template
            .insert(w.template_id as i64, w.name);
    }
    Some(out)
}

fn resolve_scenario_arg(
    cfg: &leek_wars_gen::config::GeneratorConfig,
    scenario: PathBuf,
) -> Result<PathBuf, GenError> {
    // If the path exists as-is (relative to cwd), use it.
    if scenario.is_file() {
        return Ok(scenario);
    }
    // If it's a relative path under root, allow that too.
    if !scenario.is_absolute() {
        let under_root = cfg.root.join(&scenario);
        if under_root.is_file() {
            return Ok(under_root);
        }
    }

    // Fuzzy name match: if the user passed a single path segment without extension,
    // search under scenarios_dir for *.json/*.toml whose stem contains it.
    let is_simple = scenario.components().count() == 1;
    let has_ext = scenario.extension().is_some();
    if !is_simple || has_ext {
        return Ok(scenario);
    }
    let needle = scenario.to_string_lossy().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(scenario);
    }

    let mut matches: Vec<PathBuf> = Vec::new();
    collect_scenarios_recursive(cfg.scenarios_dir.as_path(), &mut matches)?;
    let mut filtered: Vec<PathBuf> = matches
        .into_iter()
        .filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase().contains(&needle))
                .unwrap_or(false)
        })
        .collect();
    filtered.sort();
    filtered.dedup();

    match filtered.len() {
        0 => Ok(scenario),
        1 => Ok(filtered.remove(0)),
        _ => {
            let mut msg = format!(
                "scenario name {:?} is ambiguous ({} matches under {}). Try a more specific name or pass an explicit path.\n",
                scenario.display().to_string(),
                filtered.len(),
                cfg.scenarios_dir.display()
            );
            for p in filtered.iter().take(12) {
                msg.push_str(&format!("  - {}\n", p.display()));
            }
            if filtered.len() > 12 {
                msg.push_str("  - …\n");
            }
            Err(GenError::Message(msg))
        }
    }
}

fn collect_scenarios_recursive(dir: &std::path::Path, out: &mut Vec<PathBuf>) -> Result<(), GenError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            collect_scenarios_recursive(&p, out)?;
        } else if ft.is_file() {
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "json" || ext == "toml" {
                out.push(p);
            }
        }
    }
    Ok(())
}

// Note: we intentionally do not derive fid->name from the scenario `entities[*].id`, because those
// are Leek Wars entity IDs, not the engine’s runtime fids (0..n-1). Prefer `outcome.fight.leeks`.

fn cmd_replay(cmd: ReplayCmd) -> Result<(), GenError> {
    let cfg = leek_wars_gen::config::resolve(cmd.root)?;
    // If this is a repro bundle (created by `leekgen run --save-repro`), prefer reading outcome.json.
    if let Ok(outcome) = try_read_repro_outcome(cmd.artifact_dir.as_path()) {
        match cmd.output {
            Some(OutputFormat::Pretty) | None => {
                let outcome_v = leek_wars_gen::output::parse_outcome(&outcome)?;
                print!("{}", leek_wars_gen::output::pretty_summary(&outcome_v));
            }
            Some(OutputFormat::Json) => print!("{outcome}"),
            Some(OutputFormat::Ndjson) => {
                let outcome: serde_json::Value = serde_json::from_str(&outcome)
                    .map_err(|e| GenError::Message(e.to_string()))?;
                let env = serde_json::json!({
                    "kind": "leekgen_replay_bundle",
                    "bundle_dir": cmd.artifact_dir.display().to_string(),
                    "outcome": outcome,
                });
                println!(
                    "{}",
                    serde_json::to_string(&env).map_err(|e| GenError::Message(e.to_string()))?
                );
            }
        }
        return Ok(());
    }
    // Only support Rust-only artifact bundles here. Java-harness bundles should be replayed via `leekgen-compare --replay`.
    let out = leek_wars_gen::fuzz::replay_fuzz_artifact_dir_rust_outcome(
        cmd.artifact_dir.as_path(),
        cfg.root.as_path(),
        cmd.keep_temps,
    )?;

    let out_fmt = cmd.output.unwrap_or_else(|| map_output_back(cfg.output));
    match out_fmt {
        OutputFormat::Pretty => {
            let outcome = leek_wars_gen::output::parse_outcome(&out)?;
            print!("{}", leek_wars_gen::output::pretty_summary(&outcome));
        }
        OutputFormat::Json => print!("{out}"),
        OutputFormat::Ndjson => {
            let outcome: serde_json::Value =
                serde_json::from_str(&out).map_err(|e| GenError::Message(e.to_string()))?;
            let env = serde_json::json!({
                "kind": "leekgen_replay",
                "artifact_dir": cmd.artifact_dir.display().to_string(),
                "root": cfg.root.display().to_string(),
                "outcome": outcome,
            });
            println!(
                "{}",
                serde_json::to_string(&env).map_err(|e| GenError::Message(e.to_string()))?
            );
        }
    }

    Ok(())
}

fn read_scenario_json_for_bundle(
    scenario_path: &std::path::Path,
    root: &std::path::Path,
) -> Result<String, GenError> {
    let full = if scenario_path.is_absolute() {
        scenario_path.to_path_buf()
    } else {
        root.join(scenario_path)
    };
    Ok(std::fs::read_to_string(full)?)
}

fn save_repro_bundle(
    dir: &std::path::Path,
    cfg: &leek_wars_gen::config::GeneratorConfig,
    scenario_arg: &std::path::Path,
    scenario_json: &str,
    outcome_json: &str,
    also_write_sim: bool,
    sim_style: SimStyleArg,
    sim_verbosity: SimVerbosityArg,
    sim_group_turns: bool,
    sim_only_code: Option<i64>,
    sim_only_fid: Option<i64>,
    sim_limit: Option<usize>,
    sim_diff: bool,
    sim_data: Option<&leek_wars_gen::output::SimData>,
) -> Result<(), GenError> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("config.json"), serde_json::to_string_pretty(cfg).map_err(|e| GenError::Message(e.to_string()))?)?;
    std::fs::write(dir.join("scenario.json"), scenario_json)?;
    std::fs::write(dir.join("outcome.json"), outcome_json)?;
    let meta = serde_json::json!({
        "kind": "leekgen_repro_bundle",
        "scenario_arg": scenario_arg.display().to_string(),
        "root": cfg.root.display().to_string(),
        "output_default": format!("{:?}", cfg.output),
    });
    std::fs::write(
        dir.join("meta.json"),
        serde_json::to_string_pretty(&meta).map_err(|e| GenError::Message(e.to_string()))?,
    )?;

    if also_write_sim {
        let outcome_v = leek_wars_gen::output::parse_outcome(outcome_json)?;
        let opts = leek_wars_gen::output::SimOptions {
            only_code: sim_only_code,
            only_fid: sim_only_fid,
            limit: sim_limit,
            diff_friendly: sim_diff,
            group_turns: sim_group_turns,
            style: match sim_style {
                SimStyleArg::Pretty => leek_wars_gen::output::SimStyle::Pretty,
                SimStyleArg::Raw => leek_wars_gen::output::SimStyle::Raw,
            },
            show_indices: sim_diff,
            verbosity: match sim_verbosity {
                SimVerbosityArg::Brief => leek_wars_gen::output::SimVerbosity::Brief,
                SimVerbosityArg::Normal => leek_wars_gen::output::SimVerbosity::Normal,
                SimVerbosityArg::Verbose => leek_wars_gen::output::SimVerbosity::Verbose,
            },
            fid_names: None,
            data: sim_data.cloned(),
            show_live_stats: true,
        };
        let sim = leek_wars_gen::output::sim_text_with_options(&outcome_v, &opts);
        std::fs::write(dir.join("sim.txt"), sim)?;
    }
    Ok(())
}

fn try_read_repro_outcome(dir: &std::path::Path) -> Result<String, std::io::Error> {
    std::fs::read_to_string(dir.join("outcome.json"))
}

fn cmd_scenario(cmd: ScenarioCmd) -> Result<(), GenError> {
    match cmd {
        ScenarioCmd::List(c) => {
            let cfg = leek_wars_gen::config::resolve(c.root)?;
            let dir = c.dir.unwrap_or_else(|| cfg.scenarios_dir.clone());
            let paths = list_scenarios(dir.as_path(), c.recursive)?;
            for p in paths {
                println!("{}", p.display());
            }
            Ok(())
        }
        ScenarioCmd::Validate(c) => validate_path(c.path.as_path(), c.recursive),
        ScenarioCmd::Convert(c) => {
            let v = leek_wars_gen::scenario_io::load_value(c.input.as_path())?;
            leek_wars_gen::scenario_io::validate_value(&v)?;
            let to = match c.to {
                ScenarioFormat::Json => leek_wars_gen::scenario_io::ScenarioFormat::Json,
                ScenarioFormat::Toml => leek_wars_gen::scenario_io::ScenarioFormat::Toml,
            };
            let out = c.out.unwrap_or_else(|| {
                let mut p = c.input.clone();
                p.set_extension(match c.to {
                    ScenarioFormat::Json => "json",
                    ScenarioFormat::Toml => "toml",
                });
                p
            });
            leek_wars_gen::scenario_io::write_value(out.as_path(), to, &v)?;
            Ok(())
        }
        ScenarioCmd::Print(c) => {
            let v = leek_wars_gen::scenario_io::load_value(c.input.as_path())?;
            if c.normalized {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&v)
                        .map_err(|e| GenError::Message(e.to_string()))?
                );
            } else {
                match leek_wars_gen::scenario_io::infer_format(c.input.as_path()) {
                    Some(leek_wars_gen::scenario_io::ScenarioFormat::Toml) => {
                        let tv = json_to_toml_for_print(&v)?;
                        println!(
                            "{}",
                            toml::to_string_pretty(&tv)
                                .map_err(|e| GenError::Message(e.to_string()))?
                        );
                    }
                    _ => {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&v)
                                .map_err(|e| GenError::Message(e.to_string()))?
                        );
                    }
                }
            }
            Ok(())
        }
        ScenarioCmd::New(c) => {
            let tmpl = minimal_scenario_template();
            let fmt = match c.format {
                ScenarioFormat::Json => leek_wars_gen::scenario_io::ScenarioFormat::Json,
                ScenarioFormat::Toml => leek_wars_gen::scenario_io::ScenarioFormat::Toml,
            };
            if let Some(out) = c.out {
                leek_wars_gen::scenario_io::write_value(out.as_path(), fmt, &tmpl)?;
            } else {
                match fmt {
                    leek_wars_gen::scenario_io::ScenarioFormat::Json => {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&tmpl)
                                .map_err(|e| GenError::Message(e.to_string()))?
                        );
                    }
                    leek_wars_gen::scenario_io::ScenarioFormat::Toml => {
                        let tv = json_to_toml_for_print(&tmpl)?;
                        println!(
                            "{}",
                            toml::to_string_pretty(&tv)
                                .map_err(|e| GenError::Message(e.to_string()))?
                        );
                    }
                }
            }
            Ok(())
        }
        ScenarioCmd::Doctor(c) => scenario_doctor(c),
        ScenarioCmd::Normalize(c) => scenario_normalize(c),
    }
}

fn cmd_config(cmd: ConfigCmd) -> Result<(), GenError> {
    let (cfg, explain) = leek_wars_gen::config::resolve_with_explain(None)?;
    if cmd.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&cfg).map_err(|e| GenError::Message(e.to_string()))?
        );
    } else {
        println!("Resolved config");
        if let Some(p) = cfg.manifest_path.as_ref() {
            println!("  manifest      {}", p.display());
        } else {
            println!("  manifest      (none)");
        }
        println!("  root          {}", cfg.root.display());
        println!("  scenarios_dir {}", cfg.scenarios_dir.display());
        println!("  ai_dir        {}", cfg.ai_dir.display());
        println!("  output        {:?}", cfg.output);
        if cmd.explain {
            println!();
            println!("Provenance");
            println!("  root          {:?}", explain.root);
            println!("  scenarios_dir {:?}", explain.scenarios_dir);
            println!("  ai_dir        {:?}", explain.ai_dir);
            println!("  output        {:?}", explain.output);
        }
    }
    Ok(())
}

fn cmd_init(cmd: InitCmd) -> Result<(), GenError> {
    let snippet = r#"[generator]
generator_root = "leek-wars-generator"
scenarios_dir = "leek-wars-generator/test/scenario"
ai_dir = "leek-wars-generator/test/ai"
output = "pretty" # or "json" / "ndjson"
"#;
    if !cmd.write {
        print!("{snippet}");
        return Ok(());
    }

    let cwd = std::env::current_dir().map_err(GenError::from)?;
    let Some(path) = leekscript_config::find_manifest(cwd) else {
        return Err(GenError::Message(
            "init: no Leek.toml found (run `lek init` first, or create one)".into(),
        ));
    };
    let raw = std::fs::read_to_string(&path)?;
    if raw.contains("\n[generator]") || raw.starts_with("[generator]") {
        return Err(GenError::Message(format!(
            "init: {} already contains a [generator] section",
            path.display()
        )));
    }
    let mut out = raw;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(snippet);
    std::fs::write(&path, out)?;
    println!("wrote [generator] to {}", path.display());
    Ok(())
}

fn cmd_bench(cmd: BenchCmd) -> Result<(), GenError> {
    let cfg = leek_wars_gen::config::resolve(cmd.root)?;
    let scenario_input = resolve_scenario_arg(&cfg, cmd.scenario)?;
    let (scenario_json_path, _guard) =
        leek_wars_gen::scenario_io::materialize_json_path(scenario_input.as_path())?;
    let scenario_json_path = scenario_path_for_engine(scenario_json_path, cfg.root.as_path());

    let req = RunRequest {
        file: scenario_json_path,
        ..Default::default()
    };

    let engine = RustEngine;
    for _ in 0..cmd.warmup {
        let _ = engine.run_scenario_with_cwd(&req, cfg.root.as_path())?;
    }

    let mut samples: Vec<f64> = Vec::with_capacity(cmd.iters.max(1));
    for _ in 0..cmd.iters.max(1) {
        let t0 = std::time::Instant::now();
        let _ = engine.run_scenario_with_cwd(&req, cfg.root.as_path())?;
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let summary = leek_wars_gen::harness::TimingSummary::from_samples(samples);

    match cmd.output.unwrap_or(OutputFormat::Pretty) {
        OutputFormat::Pretty => {
            println!("Bench");
            println!("  root     {}", cfg.root.display());
            println!("  scenario {}", req.file.display());
            println!("  warmup   {}", cmd.warmup);
            println!("  iters    {}", cmd.iters.max(1));
            println!();
            println!("  {:<8} {:>9} {:>9} {:>9}  {}", "", "min", "median", "mean", "samples");
            if summary.iterations == 0 {
                println!("  {:<8} {:>9} {:>9} {:>9}  {}", "Rust", "—", "—", "—", "n=0");
            } else {
                println!(
                    "  {:<8} {:>8.2}ms {:>8.2}ms {:>8.2}ms  n={}",
                    "Rust", summary.min_ms, summary.median_ms, summary.mean_ms, summary.iterations
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&summary)
                    .map_err(|e| GenError::Message(e.to_string()))?
            );
        }
        OutputFormat::Ndjson => {
            let env = serde_json::json!({
                "kind": "leekgen_bench",
                "root": cfg.root.display().to_string(),
                "scenario": req.file.display().to_string(),
                "warmup": cmd.warmup,
                "iters": cmd.iters.max(1),
                "timing": summary,
            });
            println!(
                "{}",
                serde_json::to_string(&env).map_err(|e| GenError::Message(e.to_string()))?
            );
        }
    }

    Ok(())
}

fn map_output(o: OutputFormat) -> leek_wars_gen::config::OutputFormat {
    match o {
        OutputFormat::Pretty => leek_wars_gen::config::OutputFormat::Pretty,
        OutputFormat::Json => leek_wars_gen::config::OutputFormat::Json,
        OutputFormat::Ndjson => leek_wars_gen::config::OutputFormat::Ndjson,
    }
}

fn map_output_back(o: leek_wars_gen::config::OutputFormat) -> OutputFormat {
    match o {
        leek_wars_gen::config::OutputFormat::Pretty => OutputFormat::Pretty,
        leek_wars_gen::config::OutputFormat::Json => OutputFormat::Json,
        leek_wars_gen::config::OutputFormat::Ndjson => OutputFormat::Ndjson,
    }
}

fn list_scenarios(dir: &std::path::Path, recursive: bool) -> Result<Vec<PathBuf>, GenError> {
    fn walk(base: &std::path::Path, out: &mut Vec<PathBuf>, rec: bool) -> Result<(), GenError> {
        for entry in std::fs::read_dir(base)? {
            let entry = entry?;
            let p = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() && rec {
                walk(&p, out, rec)?;
            } else if ft.is_file() {
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if ext == "json" || ext == "toml" {
                    out.push(p);
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    if dir.is_dir() {
        walk(dir, &mut out, recursive)?;
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn validate_path(path: &std::path::Path, recursive: bool) -> Result<(), GenError> {
    if path.is_dir() {
        let files = list_scenarios(path, recursive)?;
        for f in files {
            let v = leek_wars_gen::scenario_io::load_value(f.as_path())?;
            leek_wars_gen::scenario_io::validate_value(&v).map_err(|e| {
                GenError::Message(format!("{}: {}", f.display(), e))
            })?;
        }
        Ok(())
    } else {
        let v = leek_wars_gen::scenario_io::load_value(path)?;
        leek_wars_gen::scenario_io::validate_value(&v)?;
        Ok(())
    }
}

fn scenario_doctor(c: ScenarioDoctorCmd) -> Result<(), GenError> {
    let paths = if c.path.is_dir() {
        list_scenarios(c.path.as_path(), c.recursive)?
    } else {
        vec![c.path.clone()]
    };
    for p in paths {
        let mut v = leek_wars_gen::scenario_io::load_value(p.as_path())?;
        leek_wars_gen::scenario_io::validate_value(&v)?;

        let mut issues: Vec<String> = Vec::new();
        let changed = apply_safe_doctor_fixes(&mut v, &mut issues, c.fix);

        if !issues.is_empty() {
            println!("{}", p.display());
            for i in &issues {
                println!("  - {i}");
            }
        }

        if c.fix && changed {
            let fmt = leek_wars_gen::scenario_io::infer_format(p.as_path())
                .unwrap_or(leek_wars_gen::scenario_io::ScenarioFormat::Json);
            leek_wars_gen::scenario_io::write_value(p.as_path(), fmt, &v)?;
            println!("  fixed");
        }
    }
    Ok(())
}

fn apply_safe_doctor_fixes(
    v: &mut serde_json::Value,
    issues: &mut Vec<String>,
    fix: bool,
) -> bool {
    let mut changed = false;
    let Some(obj) = v.as_object_mut() else {
        issues.push("scenario is not an object".into());
        return false;
    };

    if !obj.contains_key("random_seed") {
        issues.push("missing random_seed".into());
        if fix {
            obj.insert("random_seed".into(), serde_json::json!(1234567));
            changed = true;
        }
    }
    if !obj.contains_key("max_turns") {
        issues.push("missing max_turns (default 64)".into());
        if fix {
            obj.insert("max_turns".into(), serde_json::json!(64));
            changed = true;
        }
    }
    if !obj.contains_key("max_operations_per_entity") {
        issues.push("missing max_operations_per_entity (default 20000000)".into());
        if fix {
            obj.insert("max_operations_per_entity".into(), serde_json::json!(20_000_000));
            changed = true;
        }
    }
    if !obj.contains_key("draw_check_life") {
        issues.push("missing draw_check_life (default false)".into());
        if fix {
            obj.insert("draw_check_life".into(), serde_json::json!(false));
            changed = true;
        }
    }

    // Ensure each entity has `weapons`/`chips` arrays when entities are objects.
    if let Some(entities) = obj.get_mut("entities").and_then(|e| e.as_array_mut()) {
        for team in entities {
            let Some(team_arr) = team.as_array_mut() else { continue };
            for ent in team_arr {
                let Some(eobj) = ent.as_object_mut() else { continue };
                if !eobj.contains_key("weapons") {
                    issues.push("entity missing weapons (default [])".into());
                    if fix {
                        eobj.insert("weapons".into(), serde_json::json!([]));
                        changed = true;
                    }
                }
                if !eobj.contains_key("chips") {
                    issues.push("entity missing chips (default [])".into());
                    if fix {
                        eobj.insert("chips".into(), serde_json::json!([]));
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

fn scenario_normalize(c: ScenarioNormalizeCmd) -> Result<(), GenError> {
    let paths = if c.path.is_dir() {
        list_scenarios(c.path.as_path(), c.recursive)?
    } else {
        vec![c.path.clone()]
    };
    if paths.len() > 1 && !c.in_place {
        return Err(GenError::Message(
            "normalize: refusing to print multiple files to stdout; pass --in-place".into(),
        ));
    }
    for p in paths {
        let v = leek_wars_gen::scenario_io::load_value(p.as_path())?;
        leek_wars_gen::scenario_io::validate_value(&v)?;
        let normalized = normalize_json_value(&v);
        if c.in_place {
            let fmt = leek_wars_gen::scenario_io::infer_format(p.as_path())
                .unwrap_or(leek_wars_gen::scenario_io::ScenarioFormat::Json);
            leek_wars_gen::scenario_io::write_value(p.as_path(), fmt, &normalized)?;
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&normalized)
                    .map_err(|e| GenError::Message(e.to_string()))?
            );
        }
    }
    Ok(())
}

fn normalize_json_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(o) => {
            let mut keys: Vec<_> = o.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(vv) = o.get(&k) {
                    out.insert(k, normalize_json_value(vv));
                }
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(a) => {
            serde_json::Value::Array(a.iter().map(normalize_json_value).collect())
        }
        _ => v.clone(),
    }
}

fn minimal_scenario_template() -> serde_json::Value {
    serde_json::json!({
        "farmers": [
            { "id": 1, "name": "A", "country": "fr" },
            { "id": 2, "name": "B", "country": "fr" }
        ],
        "teams": [
            { "id": 1, "name": "T1" },
            { "id": 2, "name": "T2" }
        ],
        "entities": [
            [
                {
                    "id": 1, "ai": "test/ai/basic.leek", "name": "A0", "type": 1,
                    "farmer": 1, "team": 1, "level": 1, "life": 3000, "strength": 100,
                    "cores": 10, "tp": 12, "mp": 6, "cell": 0, "weapons": [], "chips": []
                }
            ],
            [
                {
                    "id": 2, "ai": "test/ai/basic.leek", "name": "B0", "type": 1,
                    "farmer": 2, "team": 2, "level": 1, "life": 3000, "strength": 100,
                    "cores": 10, "tp": 12, "mp": 6, "cell": 1, "weapons": [], "chips": []
                }
            ]
        ],
        "random_seed": 1234567,
        "max_turns": 64,
        "max_operations_per_entity": 20000000
    })
}

fn json_to_toml_for_print(v: &serde_json::Value) -> Result<toml::Value, GenError> {
    // Reuse the same conversion logic as scenario_io.
    // (Keep this local to avoid exporting conversion helpers in the public API yet.)
    fn conv(v: &serde_json::Value) -> Result<toml::Value, GenError> {
        Ok(match v {
            serde_json::Value::Null => {
                return Err(GenError::Message(
                    "cannot represent JSON null in TOML".into(),
                ))
            }
            serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    toml::Value::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    toml::Value::Float(f)
                } else {
                    return Err(GenError::Message(format!(
                        "cannot represent JSON number {n} in TOML"
                    )));
                }
            }
            serde_json::Value::String(s) => toml::Value::String(s.clone()),
            serde_json::Value::Array(arr) => {
                let mut out = Vec::with_capacity(arr.len());
                for el in arr {
                    out.push(conv(el)?);
                }
                toml::Value::Array(out)
            }
            serde_json::Value::Object(obj) => {
                let mut t = toml::value::Table::new();
                for (k, vv) in obj {
                    t.insert(k.clone(), conv(vv)?);
                }
                toml::Value::Table(t)
            }
        })
    }
    conv(v)
}
