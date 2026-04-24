//! Randomized fight runs over scenario fixtures, `random_seed`, and per-entity `ai` paths (Rust engine).
//!
//! [`run_fuzz_timed`] records wall-clock stats (Rust-only or full harness with the official generator). Optional
//! **AI source** mutation touches every `*.leek` under the configured AI mirror via a temp overlay ([`run_scenario_path_with_ai_overlay`]) or harness sandbox; scenario JSON can be
//! jittered for stats / map / placement / loadouts / `max_turns` / `max_operations_per_entity` / `draw_check_life`. CLI: `leekgen-compare --fuzz` (or `leekgen-fuzz`).
//! Use [`FuzzConfig::iterations`] `0` plus a [`std::sync::atomic::AtomicBool`] stop flag for infinite fuzz
//! (first Ctrl+C in the CLI).

use crate::engine::RunRequest;
use crate::error::GenError;
use crate::fight::merge_signature_globals;
use crate::fight::{run_scenario_path, run_scenario_path_with_ai_overlay};
use crate::fuzz_input::FuzzInput;
use crate::harness::{
    discover_scenario_json_files, discover_scenario_json_files_recursive, run_scenario_harness,
    CompareResult, HarnessRunConfig, ScenarioHarnessReport, TimingSummary,
    INCOMPLETE_SCENARIO_BASELINES,
};
use leekscript_fuzz::MutateSettings;
use leekscript_run::CompileOptions;
use leekscript_signatures::SignatureFile;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::json;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Resolved configuration for a fuzz session.
#[derive(Debug, Clone)]
pub struct FuzzConfig {
    pub generator_root: PathBuf,
    /// Scenario paths relative to [`Self::generator_root`].
    pub scenario_rels: Vec<PathBuf>,
    /// AI paths relative to [`Self::generator_root`], POSIX-style (`/`).
    pub ai_rels: Vec<String>,
    /// `0` = run until [`run_fuzz_timed`]'s `stop_requested` flag is set (e.g. first Ctrl+C).
    pub iterations: u64,
    pub master_seed: u64,
    /// When true, overwrite `random_seed` each iteration.
    pub fuzz_random_seed: bool,
    /// When true, assign each entity `.leek` `ai` from the active pool at random.
    pub shuffle_ais: bool,
    /// When true, the pool is only `.leek` paths already in the scenario (recommended).
    /// When false, the pool is [`Self::ai_rels`] only (aggressive; may hit undefined globals).
    pub restrict_ais_to_scenario: bool,
    pub keep_temps: bool,
    /// Mutate every `*.leek` under [`Self::ai_mirror_rel`] in the overlay/sandbox: `0` = off; `1` = comment only; `2`/`3` = CST-aware edits; unparsed files fall back to digit noise.
    pub mutate_ai_level: u8,
    /// When mutating AIs, injection complexity for level>=4 (0=off, higher=more complex).
    pub mutate_ai_inject_complexity: u8,
    /// When mutating AIs, percent chance (0..=100) to offer statement-wrap injection mutations for level>=4.
    pub mutate_ai_inject_wrap_percent: u8,
    /// When mutating AIs, maximum number of injected statements appended in a wrap mutation.
    pub mutate_ai_inject_max_stmts: u8,
    /// When mutating AIs, require parseable output (retries up to `max_attempts`) instead of always accepting the first candidate.
    pub mutate_ai_require_parseable: bool,
    /// Randomize numeric stats on entities that use a `.leek` AI (life, TP, MP, attributes, etc.).
    pub jitter_entity_stats: bool,
    /// Nudge `max_turns` by a small random delta (clamped).
    pub jitter_max_turns: bool,
    /// Perturb `map.obstacles` (add / remove / swap) when a `map` object is present.
    pub jitter_map_obstacles: bool,
    /// Re-roll `cell` for entities that use a `.leek` AI (uses `map.width` × `map.height` when present).
    pub jitter_entity_cells: bool,
    /// Randomize `max_operations_per_entity` around the fixture baseline (clamped).
    pub jitter_max_operations: bool,
    /// Nudge `weapons` / `chips` integer arrays on `.leek` entities when those keys exist.
    pub jitter_entity_loadouts: bool,
    /// Randomize `draw_check_life` when the field exists.
    pub fuzz_draw_check_life: bool,
    /// Directory relative to [`Self::generator_root`] to deep-copy into the AI overlay when mutating (so includes resolve).
    pub ai_mirror_rel: PathBuf,
    /// Print a one-line `[fuzz] …` progress update on stderr every N **completed** attempts (`0` = off).
    pub progress_report_every: u64,
    /// When set, each parity mismatch or run error writes a subdirectory with `scenario.json`, `meta.json`, and related files.
    pub artifacts_dir: Option<PathBuf>,
    /// CLI `--fuzz-parity`: scenario-stable fuzz + parseable AI + no inject-wrap, aimed at Java↔Rust full compare.
    pub fuzz_parity: bool,
    /// Allow generating a scenario JSON from scratch (instead of starting from a fixture template).
    pub generate_scenarios: bool,
    /// Percent chance (0..=100) to generate from scratch on an iteration when enabled.
    pub generate_scenarios_percent: u8,
    /// External AI sources (arbitrary files outside the generator root) to copy into overlays/sandboxes.
    pub external_ai_files: Vec<PathBuf>,
    /// External AI source directories to mirror (recursive) under `_external_ai/<dir_name>/...` so includes resolve.
    pub external_ai_dirs: Vec<PathBuf>,

    // --- scenario generation knobs ----------------------------------------
    pub gen_min_entities_per_team: u8,
    pub gen_max_entities_per_team: u8,
    pub gen_min_map: u8,
    pub gen_max_map: u8,
    pub gen_loadout_percent: u8,

    /// In non-parity fuzzing, require mutated AIs to compile (reject otherwise).
    pub require_compilable_ai: bool,

    /// If set, gradually expand mutation intensity over time (non-parity fuzz only).
    pub mutate_ramp: bool,
    /// Increase ramp level every N iterations (>=1).
    pub mutate_ramp_every: u64,
}

fn external_ai_dir_rel() -> PathBuf {
    PathBuf::from("_external_ai")
}

fn external_ai_files_root_rel() -> PathBuf {
    external_ai_dir_rel().join("_files")
}

fn external_ai_rel_paths(cfg: &FuzzConfig) -> Vec<String> {
    cfg.external_ai_files
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
            let rel = external_ai_files_root_rel().join(format!("{i:03}_{name}"));
            rel.to_string_lossy().replace('\\', "/")
        })
        .collect()
}

fn external_ai_dir_prefix(dir: &Path) -> String {
    let name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("dir");
    format!("_external_ai/{name}")
}

fn materialize_external_ai_files(
    cfg: &FuzzConfig,
    dst_root: &Path,
) -> Result<Vec<String>, GenError> {
    let mut out_rels: Vec<String> = Vec::new();

    // 1) Mirror directories (preserve structure for includes).
    for d in &cfg.external_ai_dirs {
        if !d.is_dir() {
            continue;
        }
        let prefix = external_ai_dir_prefix(d);
        let dst = dst_root.join(&prefix);
        copy_dir_recursive(d.as_path(), dst.as_path())
            .map_err(|e| GenError::Message(format!("fuzz: mirror external AI dir {}: {e}", d.display())))?;

        // Collect .leek paths under the mirrored dir for shuffling.
        let mut files = Vec::new();
        collect_leek_files(&dst, &mut files).map_err(|e| {
            GenError::Message(format!(
                "fuzz: list external AIs under {}: {e}",
                dst.display()
            ))
        })?;
        files.sort();
        for p in files {
            if let Ok(rel) = p.strip_prefix(dst_root) {
                out_rels.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    // 2) Copy standalone files into `_external_ai/_files/`.
    if !cfg.external_ai_files.is_empty() {
        let rels = external_ai_rel_paths(cfg);
        let dst_dir = dst_root.join(external_ai_files_root_rel());
        fs::create_dir_all(&dst_dir).map_err(GenError::from)?;
        for (src, rel) in cfg.external_ai_files.iter().zip(rels.iter()) {
            let to = dst_root.join(rel);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(GenError::from)?;
            }
            let raw = fs::read_to_string(src).map_err(|e| {
                GenError::Message(format!("fuzz: read external AI {}: {e}", src.display()))
            })?;
            fs::write(&to, raw).map_err(|e| {
                GenError::Message(format!("fuzz: write external AI {}: {e}", to.display()))
            })?;
        }
        out_rels.extend(rels);
    }
    out_rels.sort();
    out_rels.dedup();
    Ok(out_rels)
}

fn generate_scenario_from_scratch(cfg: &FuzzConfig, rng: &mut StdRng, ai_pool: &[String], seed: u64) -> Value {
    // Keep the shape close to existing fixtures, but allow wide exploration.
    let min_map = cfg.gen_min_map.max(5).min(40);
    let max_map = cfg.gen_max_map.max(min_map).min(40);
    let w: i64 = rng.gen_range(min_map as i64..=max_map as i64);
    let h: i64 = w;
    let map_cells = (w * h).max(1);

    let mut obstacles: Vec<i64> = Vec::new();
    // Density: 0..~12% of cells.
    let max_obs = ((map_cells as f64) * 0.12) as usize;
    let obstacle_n = rng.gen_range(0..=max_obs.max(1)).min(map_cells as usize);
    for _ in 0..obstacle_n {
        obstacles.push(rng.gen_range(0..map_cells) as i64);
    }
    obstacles.sort();
    obstacles.dedup();

    // 2 teams, 1..=4 entities per team.
    let min_e = cfg.gen_min_entities_per_team.max(1).min(8);
    let max_e = cfg.gen_max_entities_per_team.max(min_e).min(8);
    let n1 = rng.gen_range(min_e..=max_e) as usize;
    let n2 = rng.gen_range(min_e..=max_e) as usize;

    let pick_cell = |rng: &mut StdRng| -> i64 {
        // Best-effort avoid obstacles; if we fail, just return whatever.
        for _ in 0..8 {
            let c = rng.gen_range(0..map_cells) as i64;
            if !obstacles.contains(&c) {
                return c;
            }
        }
        rng.gen_range(0..map_cells) as i64
    };

    let mut next_entity_id: i64 = 1;
    let mut mk_entity = |team: i64, farmer: i64, name: &str, rng: &mut StdRng| -> Value {
        let ai = if ai_pool.is_empty() {
            "test/ai/basic.leek".to_string()
        } else {
            ai_pool[rng.gen_range(0..ai_pool.len())].clone()
        };
        let id = next_entity_id;
        next_entity_id += 1;

        // Keep stats in reasonable bounds to avoid super long fights, but still cover variance.
        let life = rng.gen_range(1500..=5000);
        let strength = rng.gen_range(50..=600);
        let tp = rng.gen_range(8..=20);
        let mp = rng.gen_range(4..=10);
        let level = rng.gen_range(1..=400);
        let cell = pick_cell(rng);

        let give_loadout = rng.gen_range(0u8..=100) <= cfg.gen_loadout_percent.min(100);
        let weapons = if give_loadout {
            // A small set of common IDs from fixtures.
            let candidates = [37, 43, 44, 46, 47, 107];
            let n = rng.gen_range(0..=2);
            let mut out: Vec<i64> = Vec::new();
            for _ in 0..n {
                out.push(candidates[rng.gen_range(0..candidates.len())] as i64);
            }
            out.sort();
            out.dedup();
            out
        } else {
            vec![]
        };
        let chips = if give_loadout {
            let candidates = [3, 4, 8, 11, 18, 20, 22, 29, 31, 33, 35, 36, 67, 79, 80];
            let n = rng.gen_range(0..=3);
            let mut out: Vec<i64> = Vec::new();
            for _ in 0..n {
                out.push(candidates[rng.gen_range(0..candidates.len())] as i64);
            }
            out.sort();
            out.dedup();
            out
        } else {
            vec![]
        };

        json!({
            "id": id,
            "ai": ai,
            "name": name,
            "type": 1,
            "farmer": farmer,
            "team": team,
            "level": level,
            "life": life,
            "strength": strength,
            "cores": 10,
            "tp": tp,
            "mp": mp,
            "cell": cell,
            "weapons": weapons,
            "chips": chips
        })
    };

    let mut team1: Vec<Value> = Vec::with_capacity(n1);
    for i in 0..n1 {
        team1.push(mk_entity(1, 1, &format!("A{i}"), rng));
    }
    let mut team2: Vec<Value> = Vec::with_capacity(n2);
    for i in 0..n2 {
        team2.push(mk_entity(2, 2, &format!("B{i}"), rng));
    }

    json!({
        "farmers": [
            { "id": 1, "name": "A", "country": "fr" },
            { "id": 2, "name": "B", "country": "fr" }
        ],
        "teams": [
            { "id": 1, "name": "T1" },
            { "id": 2, "name": "T2" }
        ],
        "entities": [ team1, team2 ],
        "map": {
            "width": w,
            "height": h,
            "type": 3,
            "obstacles": obstacles
        },
        "random_seed": seed,
        "max_turns": 64,
        "max_operations_per_entity": 20000000
    })
}

/// Canonical scenario paths used in parity tests; [`merge_parity_corpus_scenarios`] adds those that exist on disk.
pub const FUZZ_PARITY_SCENARIO_CORPUS: &[&str] = &[
    "test/scenario/scenario1.json",
    "test/scenario/parity_seed_424242.json",
    "test/scenario/parity_minimal_1v1.json",
    "test/scenario/parity_2v1.json",
];

/// Insert paths from [`FUZZ_PARITY_SCENARIO_CORPUS`] that exist under `root` and are not already in `seen`.
pub fn merge_parity_corpus_scenarios(root: &Path, scenario_rels: &mut Vec<PathBuf>, seen: &mut std::collections::HashSet<PathBuf>) {
    for rel in FUZZ_PARITY_SCENARIO_CORPUS {
        let p = PathBuf::from(*rel);
        if root.join(&p).is_file() && seen.insert(p.clone()) {
            scenario_rels.push(p);
        }
    }
}

#[derive(Debug)]
pub struct FuzzFailure {
    pub iteration: u64,
    pub scenario_template: PathBuf,
    /// Ephemeral working path (e.g. temp JSON or sandbox `_fuzz/scenario.json`).
    pub temp_path: PathBuf,
    /// When `--fuzz-artifacts-dir` saved a repro bundle for this failure, its directory (`scenario.json` inside).
    pub artifact_dir: Option<PathBuf>,
    pub error: GenError,
}

#[derive(Debug)]
pub struct FuzzSummary {
    pub master_seed: u64,
    pub iterations_ok: u64,
    pub failures: Vec<FuzzFailure>,
}

impl FuzzSummary {
    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Wall-clock aggregates over successful fuzz iterations (Rust-only or harness median per iteration).
#[derive(Debug)]
pub struct FuzzBenchSummary {
    pub summary: FuzzSummary,
    pub rust_wall_ms: TimingSummary,
    pub java_wall_ms: Option<TimingSummary>,
    pub parity_mismatches: u64,
}

impl FuzzBenchSummary {
    pub fn fight_ok(&self) -> bool {
        self.summary.ok()
    }

    pub fn compare_ok(&self) -> bool {
        self.parity_mismatches == 0
    }
}

fn fuzz_input_from_cfg(cfg: &FuzzConfig, iter_seed: u64) -> FuzzInput {
    let mut inpt = FuzzInput::from_seed(iter_seed);

    // CLI presets: force weights on/off to preserve historical behavior.
    inpt.p_fuzz_random_seed = if cfg.fuzz_random_seed { 255 } else { 0 };
    inpt.p_shuffle_ais = if cfg.shuffle_ais { 255 } else { 0 };
    inpt.p_allow_external_ais = if cfg.restrict_ais_to_scenario { 0 } else { 255 };

    inpt.p_jitter_entity_stats = if cfg.jitter_entity_stats { 255 } else { 0 };
    inpt.p_jitter_max_turns = if cfg.jitter_max_turns { 255 } else { 0 };
    inpt.p_randomize_draw_rule = if cfg.fuzz_draw_check_life { 255 } else { 0 };
    inpt.p_jitter_map_obstacles = if cfg.jitter_map_obstacles { 255 } else { 0 };
    inpt.p_jitter_entity_cells = if cfg.jitter_entity_cells { 255 } else { 0 };
    inpt.p_jitter_max_operations = if cfg.jitter_max_operations { 255 } else { 0 };
    inpt.p_jitter_entity_loadouts = if cfg.jitter_entity_loadouts { 255 } else { 0 };

    inpt.mutate_ai_level = cfg.mutate_ai_level.min(4);
    inpt.mutate_ai_inject_complexity = cfg.mutate_ai_inject_complexity;
    inpt.mutate_ai_inject_wrap_percent = cfg.mutate_ai_inject_wrap_percent.min(100);
    inpt.mutate_ai_inject_max_stmts = cfg.mutate_ai_inject_max_stmts.clamp(1, 16);
    inpt.mutate_ai_require_parseable = if cfg.mutate_ai_require_parseable { 255 } else { 0 };

    if cfg.fuzz_parity {
        // Documented `--fuzz-parity` profile: no inject-wrap (known Java vs Rust `fight.actions` desync).
        inpt.mutate_ai_inject_complexity = 0;
        inpt.mutate_ai_inject_wrap_percent = 0;
    }

    inpt
}

fn posix_rel(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn collect_leek_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            collect_leek_files(&p, out)?;
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("leek"))
            == Some(true)
        {
            out.push(p);
        }
    }
    Ok(())
}

/// Discover `*.leek` under `root.join(ai_dir)` (recursive). Returns paths relative to `root` with `/`.
pub fn discover_ai_rel_paths(root: &Path, ai_dir: &Path) -> io::Result<Vec<String>> {
    let base = if ai_dir.is_absolute() {
        ai_dir.to_path_buf()
    } else {
        root.join(ai_dir)
    };
    if !base.is_dir() {
        return Ok(vec![]);
    }
    let mut files = Vec::new();
    collect_leek_files(&base, &mut files)?;
    files.sort();
    let mut rels: Vec<String> = files
        .into_iter()
        .filter_map(|p| posix_rel(root, &p))
        .collect();
    rels.sort();
    rels.dedup();
    Ok(rels)
}

/// List scenario JSON paths relative to `root`, same rules as harness discovery + incomplete skip.
///
/// With `recursive`, also walks subdirectories (generated fixtures, nested suites, …).
pub fn discover_fuzz_scenario_rels(
    root: &Path,
    scenarios_dir: &Path,
    recursive: bool,
) -> io::Result<Vec<PathBuf>> {
    let mut paths = if recursive {
        discover_scenario_json_files_recursive(root, scenarios_dir)?
    } else {
        discover_scenario_json_files(root, scenarios_dir)?
    };
    paths.retain(|p| {
        !p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| INCOMPLETE_SCENARIO_BASELINES.contains(&n))
    });
    Ok(paths)
}

fn is_leek_ai_path(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty() && s.to_ascii_lowercase().ends_with(".leek")
}

/// Distinct `.leek` `ai` strings already present under `entities`.
///
/// If none are found, returns `fallback` (e.g. all files under `test/ai`), so seed-only fixtures
/// can still opt into `--no-shuffle-ai` or accept broader assignment.
pub fn ai_pool_from_document(doc: &Value, fallback: &[String]) -> Vec<String> {
    let mut from_doc: Vec<String> = Vec::new();
    if let Some(entities) = doc.get("entities").and_then(|e| e.as_array()) {
        for team in entities {
            let Some(team) = team.as_array() else {
                continue;
            };
            for ent in team {
                let Some(ai) = ent.get("ai").and_then(|v| v.as_str()) else {
                    continue;
                };
                if is_leek_ai_path(ai) {
                    from_doc.push(ai.to_string());
                }
            }
        }
    }
    from_doc.sort();
    from_doc.dedup();
    if !from_doc.is_empty() {
        from_doc
    } else {
        fallback.to_vec()
    }
}

/// Randomize `random_seed` and/or entity `ai` fields (only `.leek` paths).
pub fn apply_fuzz_variants(
    doc: &mut Value,
    rng: &mut StdRng,
    ai_pool: &[String],
    input: &FuzzInput,
) {
    if input.roll(rng, input.p_fuzz_random_seed) {
        doc["random_seed"] = Value::Number(rng.gen::<i32>().into());
    }
    if input.roll(rng, input.p_shuffle_ais) && !ai_pool.is_empty() {
        let Some(entities) = doc.get_mut("entities").and_then(|e| e.as_array_mut()) else {
            return;
        };
        for team in entities.iter_mut() {
            let Some(team) = team.as_array_mut() else {
                continue;
            };
            for ent in team.iter_mut() {
                let Some(obj) = ent.as_object_mut() else {
                    continue;
                };
                let ai = obj.get("ai").and_then(|v| v.as_str()).unwrap_or("");
                if is_leek_ai_path(ai) {
                    let pick = &ai_pool[rng.gen_range(0..ai_pool.len())];
                    obj.insert("ai".to_string(), Value::String(pick.clone()));
                }
            }
        }
    }
}

/// Collect distinct `.leek` `ai` paths from `entities` after other fuzz edits.
pub fn collect_leek_ai_paths_from_entities(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(entities) = doc.get("entities").and_then(|e| e.as_array()) else {
        return out;
    };
    for team in entities {
        let Some(team) = team.as_array() else {
            continue;
        };
        for ent in team {
            let Some(ai) = ent.get("ai").and_then(|v| v.as_str()) else {
                continue;
            };
            if is_leek_ai_path(ai) {
                out.push(ai.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn jitter_i64_for_stats(n: i64, rng: &mut StdRng, mag: u8) -> i64 {
    let scale = FuzzInput::mag_scale(mag);
    if n <= 0 {
        return rng.gen_range(1..=48_i64.saturating_mul(scale));
    }
    let lo = (n / (2 * scale)).max(1);
    let hi = n
        .saturating_mul(2 * scale)
        .min(50_000 * scale)
        .max(lo);
    rng.gen_range(lo..=hi)
}

/// Randomize combat-related numeric fields on entities that already reference a `.leek` AI.
pub fn apply_fuzz_entity_stats(doc: &mut Value, rng: &mut StdRng, mag: u8) {
    const KEYS: &[&str] = &[
        "life",
        "tp",
        "mp",
        "strength",
        "agility",
        "wisdom",
        "resistance",
        "science",
        "magic",
        "frequency",
        "level",
        "cores",
        "ram",
    ];
    let Some(entities) = doc.get_mut("entities").and_then(|e| e.as_array_mut()) else {
        return;
    };
    for team in entities.iter_mut() {
        let Some(team) = team.as_array_mut() else {
            continue;
        };
        for ent in team.iter_mut() {
            let Some(obj) = ent.as_object_mut() else {
                continue;
            };
            let ai = obj.get("ai").and_then(|v| v.as_str()).unwrap_or("");
            if !is_leek_ai_path(ai) {
                continue;
            }
            for k in KEYS {
                let Some(v) = obj.get_mut(*k) else {
                    continue;
                };
                if let Some(n) = v.as_i64() {
                    let nn = jitter_i64_for_stats(n, rng, mag);
                    *v = Value::Number(nn.into());
                }
            }
        }
    }
}

/// Nudge `max_turns` (default baseline64 if missing).
pub fn apply_fuzz_max_turns(doc: &mut Value, rng: &mut StdRng, mag: u8) {
    let base = doc.get("max_turns").and_then(|v| v.as_i64()).unwrap_or(64) as i32;
    let scale = FuzzInput::mag_scale(mag).clamp(1, 4) as i32;
    let max_delta = 12 * scale;
    let delta = rng.gen_range(-max_delta..=max_delta);
    let nt = (base + delta).clamp(16, 320);
    doc["max_turns"] = Value::Number(nt.into());
}

/// Randomize boolean `draw_check_life`.
pub fn apply_fuzz_draw_check_life(doc: &mut Value, rng: &mut StdRng) {
    doc["draw_check_life"] = Value::Bool(rng.gen_bool(0.5));
}

fn map_dimensions(doc: &Value) -> (i64, i64) {
    doc.get("map")
        .and_then(|m| m.as_object())
        .map(|m| {
            let w = m.get("width").and_then(|v| v.as_i64()).unwrap_or(17);
            let h = m.get("height").and_then(|v| v.as_i64()).unwrap_or(17);
            (w, h)
        })
        .unwrap_or((17, 17))
}

/// Add / remove / shuffle `map.obstacles` cell ids (clamped to the map area when dimensions exist).
pub fn apply_fuzz_map_obstacles(doc: &mut Value, rng: &mut StdRng) {
    let (w, h) = map_dimensions(doc);
    let area = w.saturating_mul(h).max(1);
    let Some(map) = doc.get_mut("map").and_then(|m| m.as_object_mut()) else {
        return;
    };
    let Some(obs_val) = map.get_mut("obstacles") else {
        return;
    };
    let Some(arr) = obs_val.as_array_mut() else {
        return;
    };
    if !arr.is_empty() && rng.gen_bool(0.35) {
        let i = rng.gen_range(0..arr.len());
        arr.remove(i);
    }
    if rng.gen_bool(0.55) {
        arr.push(Value::Number(rng.gen_range(0..area).into()));
    }
    if arr.len() >= 2 && rng.gen_bool(0.35) {
        let i = rng.gen_range(0..arr.len());
        let j = rng.gen_range(0..arr.len());
        arr.swap(i, j);
    }
}

/// Randomize `cell` for each entity that already uses a `.leek` AI.
pub fn apply_fuzz_entity_cells(doc: &mut Value, rng: &mut StdRng) {
    let (w, h) = map_dimensions(doc);
    let area = w.saturating_mul(h).max(1);
    let Some(entities) = doc.get_mut("entities").and_then(|e| e.as_array_mut()) else {
        return;
    };
    for team in entities.iter_mut() {
        let Some(team) = team.as_array_mut() else {
            continue;
        };
        for ent in team.iter_mut() {
            let Some(obj) = ent.as_object_mut() else {
                continue;
            };
            let ai = obj.get("ai").and_then(|v| v.as_str()).unwrap_or("");
            if !is_leek_ai_path(ai) {
                continue;
            }
            if let Some(c) = obj.get_mut("cell") {
                if c.is_i64() || c.is_u64() {
                    *c = Value::Number(rng.gen_range(0..area).into());
                }
            }
        }
    }
}

/// Nudge `max_operations_per_entity` (default 20_000_000 if absent) within a bounded range.
pub fn apply_fuzz_max_operations_per_entity(doc: &mut Value, rng: &mut StdRng) {
    let base = doc
        .get("max_operations_per_entity")
        .and_then(|v| v.as_i64())
        .unwrap_or(20_000_000);
    let lo = (base / 2).max(50_000);
    let hi = base.saturating_mul(2).min(500_000_000).max(lo);
    let nn = rng.gen_range(lo..=hi);
    doc["max_operations_per_entity"] = Value::Number(nn.into());
}

fn jitter_loadout_id(n: i64, rng: &mut StdRng) -> i64 {
    if n <= 0 {
        return rng.gen_range(1..200);
    }
    let lo = (n / 2).max(1);
    let hi = n.saturating_mul(2).min(500);
    rng.gen_range(lo..=hi)
}

fn jitter_entity_loadout_array(arr: &mut Vec<Value>, rng: &mut StdRng) {
    if arr.is_empty() {
        if rng.gen_bool(0.45) {
            arr.push(Value::Number(rng.gen_range(1..200).into()));
        }
        return;
    }
    match rng.gen_range(0..4u8) {
        0 if arr.len() > 1 => {
            arr.remove(rng.gen_range(0..arr.len()));
        }
        1 => {
            arr.push(Value::Number(rng.gen_range(1..200).into()));
        }
        2 => {
            let i = rng.gen_range(0..arr.len());
            if let Some(n) = arr[i].as_i64() {
                arr[i] = Value::Number(jitter_loadout_id(n, rng).into());
            } else {
                arr[i] = Value::Number(rng.gen_range(1..200).into());
            }
        }
        _ => {}
    }
}

/// Randomize `weapons` / `chips` arrays on `.leek` entities when present.
pub fn apply_fuzz_entity_loadouts(doc: &mut Value, rng: &mut StdRng) {
    let Some(entities) = doc.get_mut("entities").and_then(|e| e.as_array_mut()) else {
        return;
    };
    for team in entities.iter_mut() {
        let Some(team) = team.as_array_mut() else {
            continue;
        };
        for ent in team.iter_mut() {
            let Some(obj) = ent.as_object_mut() else {
                continue;
            };
            let ai = obj.get("ai").and_then(|v| v.as_str()).unwrap_or("");
            if !is_leek_ai_path(ai) {
                continue;
            }
            if rng.gen_bool(0.55) {
                if let Some(v) = obj.get_mut("weapons") {
                    if let Some(a) = v.as_array_mut() {
                        jitter_entity_loadout_array(a, rng);
                    }
                }
            }
            if rng.gen_bool(0.55) {
                if let Some(v) = obj.get_mut("chips") {
                    if let Some(a) = v.as_array_mut() {
                        jitter_entity_loadout_array(a, rng);
                    }
                }
            }
        }
    }
}

/// Deterministic, seed-driven edits to `.leek` source for crash / robustness fuzzing.
///
/// Level `1` only appends comments. Levels `2`–`4` use the LeekScript parser for syntax-preserving mutations; level `4` applies a second full pass for deeper diffs. See `fuzz_mutate_leek`.
pub fn mutate_leek_source(src: &str, rng: &mut StdRng, level: u8) -> String {
    leekscript_fuzz::mutate_leek_source(src, rng, level, &leekscript_fuzz::MutateSettings::accept_all())
        .expect("MutateSettings::accept_all is always valid")
        .source
}

/// Same resolve / globals as [`crate::fight::run_scenario_path_inner`] so `compile_source` during fuzz
/// matches fight compilation (catches `INVALID_ASSIGN_TARGET` from bad `++`/`--` mutants, etc.).
fn compile_options_for_fuzz_ai_file(ai_path: &Path) -> Result<CompileOptions, GenError> {
    let sig = SignatureFile::load_path(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("data/wars_functions.toml"),
    )
    .map_err(|e| GenError::Message(e.to_string()))?;
    let signature_globals = merge_signature_globals(sig.resolve_names());
    Ok(CompileOptions {
        source_path: Some(ai_path.to_path_buf()),
        snippet_origin: Some(ai_path.to_path_buf()),
        signature_globals,
        ..Default::default()
    })
}

fn apply_mutations_to_leek_files(
    root: &Path,
    ai_rels: &[String],
    rng: &mut StdRng,
    input: &FuzzInput,
    cfg: &FuzzConfig,
) -> Result<(), GenError> {
    let level = input.mutate_ai_level;
    if level == 0 {
        return Ok(());
    }
    let require_parseable = input.roll(rng, input.mutate_ai_require_parseable);

    for rel in ai_rels {
        let p = root.join(rel);
        let raw = fs::read_to_string(&p)
            .map_err(|e| GenError::Message(format!("fuzz: read AI {}: {e}", p.display())))?;

        let abs = fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        let compile_opts = compile_options_for_fuzz_ai_file(&abs)?;
        let path_disp = abs.display().to_string();

        let settings = if cfg.fuzz_parity || cfg.require_compilable_ai {
            let mut s = MutateSettings::require_compilable(path_disp, compile_opts);
            s.inject.complexity = input.mutate_ai_inject_complexity;
            s.inject.wrap_percent = input.mutate_ai_inject_wrap_percent;
            s.inject.max_injected_stmts = input.mutate_ai_inject_max_stmts;
            // In strict parity mode also constrain mutations to avoid known Java vs Rust divergences.
            if cfg.fuzz_parity {
                s.parity_safe = true;
            }
            s
        } else if require_parseable {
            let mut s = MutateSettings::require_parseable();
            s.inject.complexity = input.mutate_ai_inject_complexity;
            s.inject.wrap_percent = input.mutate_ai_inject_wrap_percent;
            s.inject.max_injected_stmts = input.mutate_ai_inject_max_stmts;
            s
        } else {
            let mut s = MutateSettings::accept_all();
            s.inject.complexity = input.mutate_ai_inject_complexity;
            s.inject.wrap_percent = input.mutate_ai_inject_wrap_percent;
            s.inject.max_injected_stmts = input.mutate_ai_inject_max_stmts;
            s
        };

        let mut out = leekscript_fuzz::mutate_leek_source(&raw, rng, level, &settings)
            .map_err(|e| GenError::Message(format!("fuzz: mutate AI {}: {e}", p.display())))?
            .source;
        if level == 4 {
            out = leekscript_fuzz::mutate_leek_source(&out, rng, 4, &settings)
                .map_err(|e| GenError::Message(format!("fuzz: mutate AI {}: {e}", p.display())))?
                .source;
        }
        fs::write(&p, out)
            .map_err(|e| GenError::Message(format!("fuzz: write AI {}: {e}", p.display())))?;
    }
    Ok(())
}

/// Temp tree with `data/`, mirrored AI scripts, mutated `.leek` sources, and `_fuzz/scenario.json`.
fn build_harness_mutate_sandbox(
    cfg: &FuzzConfig,
    doc: &Value,
    iter: u64,
    rng: &mut StdRng,
    input: &FuzzInput,
) -> Result<PathBuf, GenError> {
    let sandbox =
        std::env::temp_dir().join(format!("leekgen_fuzz_sb_{}_{}", cfg.master_seed, iter));
    if sandbox.is_dir() {
        let _ = fs::remove_dir_all(&sandbox);
    }
    fs::create_dir_all(sandbox.join("_fuzz")).map_err(GenError::from)?;

    let data_src = cfg.generator_root.join("data");
    if !data_src.is_dir() {
        return Err(GenError::Message(format!(
            "fuzz sandbox: missing data directory {}",
            data_src.display()
        )));
    }
    copy_dir_recursive(&data_src, &sandbox.join("data"))
        .map_err(|e| GenError::Message(format!("fuzz sandbox: copy data: {e}")))?;

    let mirror_src = cfg.generator_root.join(&cfg.ai_mirror_rel);
    if !mirror_src.is_dir() {
        return Err(GenError::Message(format!(
            "fuzz sandbox: missing AI directory {}",
            mirror_src.display()
        )));
    }
    copy_dir_recursive(&mirror_src, &sandbox.join(&cfg.ai_mirror_rel)).map_err(|e| {
        GenError::Message(format!(
            "fuzz sandbox: mirror {}: {e}",
            mirror_src.display()
        ))
    })?;

    // Make external AI files available to both Java and Rust via the sandbox runtime_cwd.
    let _external_rels = materialize_external_ai_files(cfg, &sandbox)?;

    let rels = discover_ai_rel_paths(&sandbox, cfg.ai_mirror_rel.as_path()).map_err(|e| {
        GenError::Message(format!(
            "fuzz sandbox: list .leek under {}: {e}",
            mirror_src.display()
        ))
    })?;
    apply_mutations_to_leek_files(&sandbox, &rels, rng, input, cfg)?;

    let scen = sandbox.join("_fuzz/scenario.json");
    let json = serde_json::to_string(doc).map_err(GenError::from)?;
    fs::write(&scen, json).map_err(GenError::from)?;
    Ok(sandbox)
}

fn cleanup_overlay(path: &Path, keep: bool) {
    if keep {
        return;
    }
    let _ = fs::remove_dir_all(path);
}

fn fuzz_template_slug(template: &Path) -> String {
    template
        .iter()
        .map(|c| c.to_string_lossy())
        .collect::<Vec<_>>()
        .join("_")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
}

fn fuzz_artifact_subdir(
    base: &Path,
    master_seed: u64,
    attempt_i: u64,
    kind: &str,
    template: &Path,
) -> PathBuf {
    let stem = template
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("scenario");
    let tail = fuzz_template_slug(template)
        .chars()
        .take(64)
        .collect::<String>();
    base.join(format!(
        "fuzz_{:016x}_{:06}_{}_{}__{}",
        master_seed, attempt_i, kind, stem, tail
    ))
}

/// Persist repro data for debugging (parity mismatch, harness error, Rust engine error).
/// Returns the bundle directory when `scenario.json` was written successfully.
fn try_save_fuzz_artifact(
    cfg: &FuzzConfig,
    attempt_i: u64,
    kind: &str,
    template: &Path,
    doc: &Value,
    input: &FuzzInput,
    iter_seed: u64,
    ai_seed: Option<u64>,
    benchmark_java: bool,
    report: Option<&ScenarioHarnessReport>,
    error: Option<&GenError>,
    sandbox_src: Option<&Path>,
) -> Option<PathBuf> {
    let Some(base) = cfg.artifacts_dir.as_ref() else {
        return None;
    };
    let dir = fuzz_artifact_subdir(base, cfg.master_seed, attempt_i, kind, template);
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!(
            "[fuzz] artifacts: could not create {}: {e}",
            dir.display()
        );
        return None;
    }
    let scenario_json = match serde_json::to_string_pretty(doc) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "[fuzz] artifacts: could not serialize scenario for {}: {e}",
                dir.display()
            );
            return None;
        }
    };
    if let Err(e) = fs::write(dir.join("scenario.json"), &scenario_json) {
        eprintln!(
            "[fuzz] artifacts: could not write scenario.json in {}: {e}",
            dir.display()
        );
        return None;
    }
    if let Some(e) = error {
        let _ = fs::write(dir.join("error.txt"), e.to_string());
    }
    if let Some(r) = report {
        let (outcome_java_parse, outcome_rust_parse) = match &r.compare {
            CompareResult::OutcomeNotJson { java, rust } => (java.as_ref(), rust.as_ref()),
            _ => (None, None),
        };

        if let Some(je) = r.java_error.as_ref() {
            let _ = fs::write(dir.join("error_java.txt"), je);
        } else if let Some(j) = outcome_java_parse {
            let _ = fs::write(dir.join("error_java.txt"), j);
        }
        if let Some(re) = r.rust_error.as_ref() {
            let _ = fs::write(dir.join("error_rust.txt"), re);
        } else if let Some(rust_m) = outcome_rust_parse {
            let _ = fs::write(dir.join("error_rust.txt"), rust_m);
        }

        // `outcome_*.json`: real fight JSON when present; otherwise a small JSON envelope so the Rust/Java side is never an empty file.
        if r.last_rust_json.trim().is_empty() {
            if let Some(msg) = r.rust_error.as_ref().or(outcome_rust_parse) {
                let env = serde_json::json!({
                    "_leekgen_artifact": "rust_side_error",
                    "message": msg,
                    "companion_text": "error_rust.txt",
                });
                let _ = fs::write(
                    dir.join("outcome_rust.json"),
                    serde_json::to_string_pretty(&env).unwrap_or_else(|_| "{}".into()),
                );
            } else {
                let _ = fs::write(dir.join("outcome_rust.json"), "");
            }
        } else {
            let _ = fs::write(dir.join("outcome_rust.json"), &r.last_rust_json);
        }

        match r.last_java_json.as_ref() {
            Some(j) if !j.trim().is_empty() => {
                let _ = fs::write(dir.join("outcome_java.json"), j);
            }
            _ => {
                if let Some(msg) = r.java_error.as_ref().or(outcome_java_parse) {
                    let env = serde_json::json!({
                        "_leekgen_artifact": "java_side_error",
                        "message": msg,
                        "companion_text": "error_java.txt",
                    });
                    let _ = fs::write(
                        dir.join("outcome_java.json"),
                        serde_json::to_string_pretty(&env).unwrap_or_else(|_| "{}".into()),
                    );
                }
            }
        }

        if let Ok(s) = serde_json::to_string_pretty(&r.compare) {
            let _ = fs::write(dir.join("compare.json"), s);
        }
        if let CompareResult::FullMismatch {
            normalized_diff: Some(diff),
            ..
        } = &r.compare
        {
            let _ = fs::write(dir.join("normalized_diff.txt"), diff);
        }
    }
    let mut mirror_patch_written = false;
    if let Some(sand) = sandbox_src {
        match write_minimal_fuzz_sandbox_artifact(cfg, sand, &dir) {
            Ok(wrote_patch) => mirror_patch_written = wrote_patch,
            Err(e) => eprintln!(
                "[fuzz] artifacts: could not write minimal sandbox {} -> {}: {e}",
                sand.display(),
                dir.join("sandbox").display()
            ),
        }
    }
    let mut meta = serde_json::json!({
        "master_seed": cfg.master_seed,
        "attempt_index": attempt_i,
        "iteration_seed": iter_seed,
        "ai_seed": ai_seed,
        "kind": kind,
        "scenario_template": template.display().to_string(),
        "benchmark_java": benchmark_java,
        "fuzz_parity": cfg.fuzz_parity,
        "ai_mirror_rel": cfg.ai_mirror_rel.to_string_lossy().replace('\\', "/"),
        "mutate_ai_level": cfg.mutate_ai_level,
        "fuzz_input_hex": input.to_hex(),
        "mutate_ai_inject_complexity": input.mutate_ai_inject_complexity,
        "mutate_ai_inject_wrap_percent": input.mutate_ai_inject_wrap_percent,
        "mutate_ai_inject_max_stmts": input.mutate_ai_inject_max_stmts,
        "mutate_ai_require_parseable_p": input.mutate_ai_require_parseable,
        "mutate_ai_require_compilable": cfg.fuzz_parity,
        "mutator_parity_safe": cfg.fuzz_parity,
        "error": error.map(|e| e.to_string()),
    });
    if let Some(r) = report {
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("compare_mode".into(), serde_json::json!(r.mode));
        }
    }
    if sandbox_src.is_some() {
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("sandbox_layout".into(), serde_json::json!("minimal"));
            obj.insert(
                "sandbox_excludes_data_dir".into(),
                serde_json::json!(true),
            );
            obj.insert(
                "sandbox_omits_java_ai_output".into(),
                serde_json::json!(true),
            );
            obj.insert(
                "ai_mirror_rel".into(),
                serde_json::json!(cfg.ai_mirror_rel.to_string_lossy().replace('\\', "/")),
            );
            if mirror_patch_written {
                obj.insert(
                    "sandbox_mirror_patch".into(),
                    serde_json::json!("sandbox/mirror.patch"),
                );
            }
        }
    }
    if let Err(e) = fs::write(
        dir.join("meta.json"),
        serde_json::to_string_pretty(&meta).unwrap_or_else(|_| "{}".into()),
    ) {
        eprintln!(
            "[fuzz] artifacts: could not write meta.json in {}: {e}",
            dir.display()
        );
    }
    eprintln!(
        "[fuzz] artifacts: saved {} repro to {}",
        kind,
        dir.display()
    );
    Some(dir)
}

fn fuzz_attempt_done_bump_progress(
    cfg: &FuzzConfig,
    i: &mut u64,
    scenario_template: &Path,
    ok: u64,
    failures_len: usize,
    parity_mismatches: u64,
) {
    *i = i.wrapping_add(1);
    if cfg.progress_report_every == 0 {
        return;
    }
    if *i % cfg.progress_report_every != 0 {
        return;
    }
    eprintln!(
        "[fuzz] attempts={}  ok={}  failed={}  parity_mismatch={}  last_scenario={}",
        *i,
        ok,
        failures_len,
        parity_mismatches,
        scenario_template.display()
    );
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let p = entry.path();
        let name = entry.file_name();
        let to = dst.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&p, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&p, &to)?;
        }
    }
    Ok(())
}

fn collect_files_recursive_files(root: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let p = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files_recursive_files(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

fn posix_path_components(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Unified diff of all files under `sandbox_mirror` that differ from `generator_mirror`.
fn mirror_tree_unified_patch(
    generator_mirror: &Path,
    sandbox_mirror: &Path,
    mirror_rel: &Path,
) -> io::Result<String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files_recursive_files(sandbox_mirror, &mut files)?;
    files.sort();
    let mirror_prefix = posix_path_components(mirror_rel);
    let mut out = String::new();
    for path in files {
        let rel = path.strip_prefix(sandbox_mirror).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "artifact diff: path not under sandbox mirror",
            )
        })?;
        let rel_posix = posix_path_components(rel);
        let display_path = if mirror_prefix.is_empty() {
            rel_posix
        } else if rel_posix.is_empty() {
            mirror_prefix.clone()
        } else {
            format!("{mirror_prefix}/{rel_posix}")
        };
        let gen_file = generator_mirror.join(rel);
        let old = fs::read_to_string(&gen_file).unwrap_or_default();
        let new = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "[fuzz] artifacts: skip mirror file {} (not UTF-8?): {e}",
                    path.display()
                );
                continue;
            }
        };
        if old == new {
            continue;
        }
        let patch = diffy::create_patch(&old, &new);
        use std::fmt::Write;
        writeln!(&mut out, "diff --git a/{display_path} b/{display_path}").unwrap();
        writeln!(&mut out, "--- a/{display_path}").unwrap();
        writeln!(&mut out, "+++ b/{display_path}").unwrap();
        writeln!(&mut out, "{patch}").unwrap();
        writeln!(&mut out).unwrap();
    }
    Ok(out)
}

/// Artifact `sandbox/`: `_fuzz/` only (mutate+official-generator runs), plus `mirror.patch` vs `generator_root/ai_mirror_rel`.
/// Omits `data/`, generated top-level `ai/` (`.java`/`.class`), and full mirror tree copies.
fn write_minimal_fuzz_sandbox_artifact(
    cfg: &FuzzConfig,
    sand: &Path,
    artifact_bundle: &Path,
) -> io::Result<bool> {
    let dst = artifact_bundle.join("sandbox");
    if dst.exists() {
        let _ = fs::remove_dir_all(&dst);
    }
    fs::create_dir_all(&dst)?;
    let fuzz_src = sand.join("_fuzz");
    if fuzz_src.is_dir() {
        copy_dir_recursive(&fuzz_src, &dst.join("_fuzz"))?;
    }
    let gen_mirror = cfg.generator_root.join(&cfg.ai_mirror_rel);
    let sand_mirror = sand.join(&cfg.ai_mirror_rel);
    let mut wrote_patch = false;
    if sand_mirror.is_dir() {
        let patch = mirror_tree_unified_patch(&gen_mirror, &sand_mirror, &cfg.ai_mirror_rel)?;
        if !patch.is_empty() {
            fs::write(dst.join("mirror.patch"), patch)?;
            wrote_patch = true;
        }
    }
    Ok(wrote_patch)
}

/// Run the Rust fight engine on `iterations` randomized variants.
/// With `fail_fast`, stops after the first run failure (fixture read/parse errors always stop).
pub fn run_fuzz(cfg: &FuzzConfig, fail_fast: bool) -> FuzzSummary {
    assert_ne!(
        cfg.iterations, 0,
        "fuzz: iterations == 0 (infinite) requires run_fuzz_timed(..., stop_requested: Some(...))"
    );
    run_fuzz_timed(cfg, fail_fast, None, None).summary
}

/// Like [`run_fuzz`], but records wall-clock stats over successful iterations.
///
/// - **`harness: None`**: times a single [`run_scenario_path`] (or [`run_scenario_path_with_ai_overlay`]
///   when [`FuzzConfig::mutate_ai_level`] is non-zero) per iteration.
/// - **`harness: Some`**: runs [`run_scenario_harness`] (compare mode comes from CLI: e.g. `--fuzz-benchmark-java`
///   defaults to full normalized official generator vs Rust parity, including `fight.actions` and `fight.ops`). With
///   [`FuzzConfig::mutate_ai_level`] > 0, builds a temp **sandbox** (copies `data/` + `--fuzz-ai-dir` tree,
///   mutates every `*.leek` under that mirror (levels `2`–`4`: CST-aware edits when the LeekScript parser accepts the file; `4` runs two rounds),
///   writes `_fuzz/scenario.json`) and sets [`HarnessRunConfig::runtime_cwd`]
///   so the official generator and Rust both load the same mutated AI tree.
pub fn run_fuzz_timed(
    cfg: &FuzzConfig,
    fail_fast: bool,
    harness: Option<&HarnessRunConfig>,
    stop_requested: Option<&AtomicBool>,
) -> FuzzBenchSummary {
    assert!(
        !cfg.scenario_rels.is_empty(),
        "fuzz: need at least one scenario"
    );
    if cfg.iterations == 0 && stop_requested.is_none() {
        panic!("fuzz: iterations == 0 requires a stop flag (Ctrl+C handler)");
    }
    let mut rng = StdRng::seed_from_u64(cfg.master_seed);
    let mut ok: u64 = 0;
    let mut failures: Vec<FuzzFailure> = Vec::new();
    let mut rust_ms: Vec<f64> = Vec::new();
    let mut java_ms: Vec<f64> = Vec::new();
    let mut parity_mismatches: u64 = 0;

    let mut i: u64 = 0;
    loop {
        if let Some(flag) = stop_requested {
            if flag.load(Ordering::Relaxed) {
                break;
            }
        }
        if cfg.iterations != 0 && i >= cfg.iterations {
            break;
        }
        let iter_seed: u64 = rng.gen();
        let mut input = fuzz_input_from_cfg(cfg, iter_seed);
        let mut iter_rng = StdRng::seed_from_u64(iter_seed);
        let benchmark_java = harness.is_some();
        let mut ai_seed: Option<u64> = None;
        let want_generate = cfg.generate_scenarios
            && cfg.generate_scenarios_percent > 0
            && rng.gen_range(0u8..=100) <= cfg.generate_scenarios_percent;

        let (template, mut doc): (PathBuf, Value) = if want_generate {
            let pool = if cfg.ai_rels.is_empty() {
                vec!["test/ai/basic.leek".to_string()]
            } else {
                cfg.ai_rels.clone()
            };
            let d = generate_scenario_from_scratch(cfg, &mut iter_rng, &pool, iter_seed);
            (PathBuf::from("generated"), d)
        } else {
            let template = cfg.scenario_rels[rng.gen_range(0..cfg.scenario_rels.len())].clone();
            let full = cfg.generator_root.join(&template);
            let raw = match fs::read_to_string(&full) {
                Ok(s) => s,
                Err(e) => {
                    failures.push(FuzzFailure {
                        iteration: i,
                        scenario_template: template.clone(),
                        temp_path: PathBuf::new(),
                        artifact_dir: None,
                        error: GenError::Message(format!(
                            "read {}: {e}",
                            full.display()
                        )),
                    });
                    break;
                }
            };
            let doc: Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(e) => {
                    failures.push(FuzzFailure {
                        iteration: i,
                        scenario_template: template.clone(),
                        temp_path: PathBuf::new(),
                        artifact_dir: None,
                        error: GenError::ScenarioJson(e),
                    });
                    break;
                }
            };
            (template, doc)
        };

        let allow_external = input.roll(&mut iter_rng, input.p_allow_external_ais);
        let ai_pool = if allow_external {
            cfg.ai_rels.clone()
        } else {
            ai_pool_from_document(&doc, &cfg.ai_rels)
        };

        // Non-parity: optionally ramp mutation intensity with iteration count.
        if !cfg.fuzz_parity && cfg.mutate_ramp {
            let step = (i / cfg.mutate_ramp_every.max(1)).min(10) as u8;
            input.mutate_ai_inject_complexity =
                (input.mutate_ai_inject_complexity.saturating_add(step)).min(8);
            input.mutate_ai_inject_wrap_percent =
                (input.mutate_ai_inject_wrap_percent.saturating_add(step.saturating_mul(5))).min(100);
            input.mutate_ai_inject_max_stmts =
                (input.mutate_ai_inject_max_stmts.saturating_add(step / 2)).clamp(1, 16);
        }

        // Parity mode: avoid Rust-only `too_much_ops` AI errors when the official generator does not
        // enforce the same `max_operations_per_entity` limit (it reports ops but can exceed the cap).
        // This keeps `actions_exact` comparisons focused on engine semantics rather than ops-limit policy.
        if cfg.fuzz_parity {
            doc["max_operations_per_entity"] = json!(1_000_000_000i64);
        }
        apply_fuzz_variants(
            &mut doc,
            &mut iter_rng,
            &ai_pool,
            &input,
        );
        if input.roll(&mut iter_rng, input.p_jitter_entity_stats) {
            apply_fuzz_entity_stats(&mut doc, &mut iter_rng, input.mag_entity_stats);
        }
        if input.roll(&mut iter_rng, input.p_jitter_max_turns) {
            apply_fuzz_max_turns(&mut doc, &mut iter_rng, input.mag_max_turns);
        }
        if input.roll(&mut iter_rng, input.p_randomize_draw_rule) {
            apply_fuzz_draw_check_life(&mut doc, &mut iter_rng);
        }
        if input.roll(&mut iter_rng, input.p_jitter_map_obstacles) {
            apply_fuzz_map_obstacles(&mut doc, &mut iter_rng);
        }
        if input.roll(&mut iter_rng, input.p_jitter_entity_cells) {
            apply_fuzz_entity_cells(&mut doc, &mut iter_rng);
        }
        if input.roll(&mut iter_rng, input.p_jitter_max_operations) {
            apply_fuzz_max_operations_per_entity(&mut doc, &mut iter_rng);
        }
        if input.roll(&mut iter_rng, input.p_jitter_entity_loadouts) {
            apply_fuzz_entity_loadouts(&mut doc, &mut iter_rng);
        }

        if let (Some(h), true) = (harness, input.mutate_ai_level > 0) {
            ai_seed = Some(iter_rng.gen());
            let mut ai_rng = StdRng::seed_from_u64(ai_seed.unwrap_or(iter_seed));
            let sand = match build_harness_mutate_sandbox(cfg, &doc, i, &mut ai_rng, &input) {
                Ok(p) => p,
                Err(e) => {
                    let artifact_dir = try_save_fuzz_artifact(
                        cfg,
                        i,
                        "sandbox_build",
                        template.as_path(),
                        &doc,
                        &input,
                        iter_seed,
                        ai_seed,
                        benchmark_java,
                        None,
                        Some(&e),
                        None,
                    );
                    failures.push(FuzzFailure {
                        iteration: i,
                        scenario_template: template.clone(),
                        temp_path: PathBuf::new(),
                        artifact_dir,
                        error: e,
                    });
                    if fail_fast {
                        break;
                    }
                    fuzz_attempt_done_bump_progress(
                        cfg,
                        &mut i,
                        template.as_path(),
                        ok,
                        failures.len(),
                        parity_mismatches,
                    );
                    continue;
                }
            };
            let mut hcfg = h.clone();
            hcfg.runtime_cwd = Some(sand.clone());
            let req = RunRequest {
                file: PathBuf::from("_fuzz/scenario.json"),
                ..Default::default()
            };
            match run_scenario_harness(&req, &hcfg) {
                Ok(report) => {
                    if report.engine_run_failed() {
                        let err = report
                            .engine_errors_display()
                            .map(GenError::Message)
                            .unwrap_or_else(|| {
                                GenError::Message(
                                    "harness: engine failed (no error text)".into(),
                                )
                            });
                        let artifact_dir = try_save_fuzz_artifact(
                            cfg,
                            i,
                            "harness_error",
                            template.as_path(),
                            &doc,
                            &input,
                            iter_seed,
                            ai_seed,
                            benchmark_java,
                            Some(&report),
                            Some(&err),
                            Some(sand.as_path()),
                        );
                        failures.push(FuzzFailure {
                            iteration: i,
                            scenario_template: template.clone(),
                            temp_path: sand.join("_fuzz/scenario.json"),
                            artifact_dir,
                            error: err,
                        });
                        cleanup_overlay(&sand, cfg.keep_temps);
                        if fail_fast {
                            break;
                        }
                        fuzz_attempt_done_bump_progress(
                            cfg,
                            &mut i,
                            template.as_path(),
                            ok,
                            failures.len(),
                            parity_mismatches,
                        );
                        continue;
                    }
                    ok += 1;
                    rust_ms.push(report.rust.median_ms);
                    if let Some(j) = &report.java {
                        java_ms.push(j.median_ms);
                    }
                    if report.comparison_failed() {
                        parity_mismatches += 1;
                        try_save_fuzz_artifact(
                            cfg,
                            i,
                            "parity",
                            template.as_path(),
                            &doc,
                            &input,
                            iter_seed,
                            ai_seed,
                            benchmark_java,
                            Some(&report),
                            None,
                            Some(sand.as_path()),
                        );
                    }
                }
                Err(e) => {
                    let artifact_dir = try_save_fuzz_artifact(
                        cfg,
                        i,
                        "harness_error",
                        template.as_path(),
                        &doc,
                        &input,
                        iter_seed,
                        ai_seed,
                        benchmark_java,
                        None,
                        Some(&e),
                        Some(sand.as_path()),
                    );
                    failures.push(FuzzFailure {
                        iteration: i,
                        scenario_template: template.clone(),
                        temp_path: sand.join("_fuzz/scenario.json"),
                        artifact_dir,
                        error: e,
                    });
                    cleanup_overlay(&sand, cfg.keep_temps);
                    if fail_fast {
                        break;
                    }
                    fuzz_attempt_done_bump_progress(
                        cfg,
                        &mut i,
                        template.as_path(),
                        ok,
                        failures.len(),
                        parity_mismatches,
                    );
                    continue;
                }
            }
            cleanup_overlay(&sand, cfg.keep_temps);
            fuzz_attempt_done_bump_progress(
                cfg,
                &mut i,
                template.as_path(),
                ok,
                failures.len(),
                parity_mismatches,
            );
            continue;
        }

        let tmp = std::env::temp_dir().join(format!("leekgen_fuzz_{}_{}.json", cfg.master_seed, i));
        let json_str = match serde_json::to_string(&doc) {
            Ok(s) => s,
            Err(e) => {
                failures.push(FuzzFailure {
                    iteration: i,
                    scenario_template: template.clone(),
                    temp_path: tmp,
                    artifact_dir: None,
                    error: GenError::ScenarioJson(e),
                });
                break;
            }
        };
        if let Err(e) = fs::write(&tmp, json_str) {
            failures.push(FuzzFailure {
                iteration: i,
                scenario_template: template.clone(),
                temp_path: tmp,
                artifact_dir: None,
                error: e.into(),
            });
            break;
        }

        // Rust-only mutated AIs: overlay full mirror + mutations on every `*.leek` under it; `data/` still from real generator root.
        let overlay_dir =
            std::env::temp_dir().join(format!("leekgen_fuzz_ai_{}_{}", cfg.master_seed, i));
        let overlay_for_run: Option<PathBuf> = if harness.is_none()
            && (input.mutate_ai_level > 0 || !cfg.external_ai_files.is_empty())
        {
            ai_seed = Some(iter_rng.gen());
            let mut ai_rng = StdRng::seed_from_u64(ai_seed.unwrap_or(iter_seed));
            // Always materialize external AIs if configured; optionally mirror+mutate the in-tree AIs.
            if overlay_dir.is_dir() {
                let _ = fs::remove_dir_all(&overlay_dir);
            }
            if let Err(e) = fs::create_dir_all(&overlay_dir) {
                let err = GenError::Message(format!(
                    "fuzz: create overlay {}: {e}",
                    overlay_dir.display()
                ));
                let artifact_dir = try_save_fuzz_artifact(
                    cfg,
                    i,
                    "overlay_dir",
                    template.as_path(),
                    &doc,
                    &input,
                    iter_seed,
                    ai_seed,
                    benchmark_java,
                    None,
                    Some(&err),
                    None,
                );
                failures.push(FuzzFailure {
                    iteration: i,
                    scenario_template: template.clone(),
                    temp_path: tmp,
                    artifact_dir,
                    error: err,
                });
                if fail_fast {
                    break;
                }
                fuzz_attempt_done_bump_progress(
                    cfg,
                    &mut i,
                    template.as_path(),
                    ok,
                    failures.len(),
                    parity_mismatches,
                );
                continue;
            }
            let _external_rels = match materialize_external_ai_files(cfg, &overlay_dir) {
                Ok(r) => r,
                Err(e) => {
                    cleanup_overlay(&overlay_dir, cfg.keep_temps);
                    let artifact_dir = try_save_fuzz_artifact(
                        cfg,
                        i,
                        "external_ai",
                        template.as_path(),
                        &doc,
                        &input,
                        iter_seed,
                        ai_seed,
                        benchmark_java,
                        None,
                        Some(&e),
                        None,
                    );
                    failures.push(FuzzFailure {
                        iteration: i,
                        scenario_template: template.clone(),
                        temp_path: tmp,
                        artifact_dir,
                        error: e,
                    });
                    if fail_fast {
                        break;
                    }
                    fuzz_attempt_done_bump_progress(
                        cfg,
                        &mut i,
                        template.as_path(),
                        ok,
                        failures.len(),
                        parity_mismatches,
                    );
                    continue;
                }
            };

            if input.mutate_ai_level > 0 {
                let mirror_src = cfg.generator_root.join(&cfg.ai_mirror_rel);
                match copy_dir_recursive(&mirror_src, &overlay_dir.join(&cfg.ai_mirror_rel)) {
                    Ok(()) => {}
                    Err(e) => {
                        cleanup_overlay(&overlay_dir, cfg.keep_temps);
                        let err = GenError::Message(format!(
                            "fuzz: mirror AI tree {}: {e}",
                            mirror_src.display()
                        ));
                        let artifact_dir = try_save_fuzz_artifact(
                            cfg,
                            i,
                            "ai_mirror",
                            template.as_path(),
                            &doc,
                            &input,
                            iter_seed,
                            ai_seed,
                            benchmark_java,
                            None,
                            Some(&err),
                            None,
                        );
                        failures.push(FuzzFailure {
                            iteration: i,
                            scenario_template: template.clone(),
                            temp_path: tmp,
                            artifact_dir,
                            error: err,
                        });
                        if fail_fast {
                            break;
                        }
                        fuzz_attempt_done_bump_progress(
                            cfg,
                            &mut i,
                            template.as_path(),
                            ok,
                            failures.len(),
                            parity_mismatches,
                        );
                        continue;
                    }
                }
            }
            if input.mutate_ai_level == 0 {
                // Overlay exists only to host external AIs.
                Some(overlay_dir)
            } else {
                let mirror_src = cfg.generator_root.join(&cfg.ai_mirror_rel);
                let rels = match discover_ai_rel_paths(&overlay_dir, cfg.ai_mirror_rel.as_path()) {
                    Ok(r) => r,
                    Err(e) => {
                        cleanup_overlay(&overlay_dir, cfg.keep_temps);
                        let err = GenError::Message(format!(
                            "fuzz: list .leek under {}: {e}",
                            mirror_src.display()
                        ));
                        let artifact_dir = try_save_fuzz_artifact(
                            cfg,
                            i,
                            "mutate_ai",
                            template.as_path(),
                            &doc,
                            &input,
                            iter_seed,
                            ai_seed,
                            benchmark_java,
                            None,
                            Some(&err),
                            None,
                        );
                        failures.push(FuzzFailure {
                            iteration: i,
                            scenario_template: template.clone(),
                            temp_path: tmp,
                            artifact_dir,
                            error: err,
                        });
                        if fail_fast {
                            break;
                        }
                        fuzz_attempt_done_bump_progress(
                            cfg,
                            &mut i,
                            template.as_path(),
                            ok,
                            failures.len(),
                            parity_mismatches,
                        );
                        continue;
                    }
                };
                if rels.is_empty() {
                    cleanup_overlay(&overlay_dir, cfg.keep_temps);
                    None
                } else {
                    match apply_mutations_to_leek_files(&overlay_dir, &rels, &mut ai_rng, &input, cfg)
                    {
                        Ok(()) => Some(overlay_dir),
                        Err(e) => {
                            cleanup_overlay(&overlay_dir, cfg.keep_temps);
                            let artifact_dir = try_save_fuzz_artifact(
                                cfg,
                                i,
                                "mutate_ai",
                                template.as_path(),
                                &doc,
                                &input,
                                iter_seed,
                                ai_seed,
                                benchmark_java,
                                None,
                                Some(&e),
                                None,
                            );
                            failures.push(FuzzFailure {
                                iteration: i,
                                scenario_template: template.clone(),
                                temp_path: tmp,
                                artifact_dir,
                                error: e,
                            });
                            if fail_fast {
                                break;
                            }
                            fuzz_attempt_done_bump_progress(
                                cfg,
                                &mut i,
                                template.as_path(),
                                ok,
                                failures.len(),
                                parity_mismatches,
                            );
                            continue;
                        }
                    }
                }
            }
        } else {
            None
        };

        match harness {
            None => {
                let t0 = Instant::now();
                let run_res = match overlay_for_run.as_ref() {
                    Some(o) => run_scenario_path_with_ai_overlay(
                        &tmp,
                        &cfg.generator_root,
                        Some(o.as_path()),
                    ),
                    None => run_scenario_path(&tmp, &cfg.generator_root),
                };
                match run_res {
                    Ok(_) => {
                        ok += 1;
                        rust_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
                        if !cfg.keep_temps {
                            let _ = fs::remove_file(&tmp);
                        }
                    }
                    Err(e) => {
                        let artifact_dir = try_save_fuzz_artifact(
                            cfg,
                            i,
                            "rust_engine",
                            template.as_path(),
                            &doc,
                            &input,
                            iter_seed,
                            ai_seed,
                            benchmark_java,
                            None,
                            Some(&e),
                            overlay_for_run.as_deref(),
                        );
                        failures.push(FuzzFailure {
                            iteration: i,
                            scenario_template: template.clone(),
                            temp_path: tmp,
                            artifact_dir,
                            error: e,
                        });
                        if fail_fast {
                            if let Some(ref o) = overlay_for_run {
                                cleanup_overlay(o, cfg.keep_temps);
                            }
                            break;
                        }
                    }
                }
                if let Some(ref o) = overlay_for_run {
                    cleanup_overlay(o, cfg.keep_temps);
                }
            }
            Some(h) => match run_scenario_harness(
                &RunRequest {
                    file: tmp.clone(),
                    ..Default::default()
                },
                h,
            ) {
                Ok(report) => {
                    if report.engine_run_failed() {
                        let err = report
                            .engine_errors_display()
                            .map(GenError::Message)
                            .unwrap_or_else(|| {
                                GenError::Message(
                                    "harness: engine failed (no error text)".into(),
                                )
                            });
                        let artifact_dir = try_save_fuzz_artifact(
                            cfg,
                            i,
                            "harness_error",
                            template.as_path(),
                            &doc,
                            &input,
                            iter_seed,
                            ai_seed,
                            benchmark_java,
                            Some(&report),
                            Some(&err),
                            None,
                        );
                        failures.push(FuzzFailure {
                            iteration: i,
                            scenario_template: template.clone(),
                            temp_path: tmp.clone(),
                            artifact_dir,
                            error: err,
                        });
                        if fail_fast {
                            break;
                        }
                    } else {
                        ok += 1;
                        rust_ms.push(report.rust.median_ms);
                        if let Some(j) = &report.java {
                            java_ms.push(j.median_ms);
                        }
                        if report.comparison_failed() {
                            parity_mismatches += 1;
                            try_save_fuzz_artifact(
                                cfg,
                                i,
                                "parity",
                                template.as_path(),
                                &doc,
                                &input,
                                iter_seed,
                                ai_seed,
                                benchmark_java,
                                Some(&report),
                                None,
                                None,
                            );
                        }
                    }
                    if !cfg.keep_temps {
                        let _ = fs::remove_file(&tmp);
                    }
                }
                Err(e) => {
                    let artifact_dir = try_save_fuzz_artifact(
                        cfg,
                        i,
                        "harness_error",
                        template.as_path(),
                        &doc,
                        &input,
                        iter_seed,
                        ai_seed,
                        benchmark_java,
                        None,
                        Some(&e),
                        None,
                    );
                    failures.push(FuzzFailure {
                        iteration: i,
                        scenario_template: template.clone(),
                        temp_path: tmp,
                        artifact_dir,
                        error: e,
                    });
                    if fail_fast {
                        break;
                    }
                }
            },
        }
        fuzz_attempt_done_bump_progress(
            cfg,
            &mut i,
            template.as_path(),
            ok,
            failures.len(),
            parity_mismatches,
        );
    }

    let summary = FuzzSummary {
        master_seed: cfg.master_seed,
        iterations_ok: ok,
        failures,
    };
    FuzzBenchSummary {
        rust_wall_ms: TimingSummary::from_samples(rust_ms),
        java_wall_ms: if java_ms.is_empty() {
            None
        } else {
            Some(TimingSummary::from_samples(java_ms))
        },
        parity_mismatches,
        summary,
    }
}

/// Replay a saved fuzz artifact directory.
///
/// This is primarily intended for `leekgen-compare --replay <dir>` and for feeding saved
/// `fuzz_input_hex` values into libFuzzer corpora.
pub fn replay_fuzz_artifact_dir(
    artifact_dir: &Path,
    generator_root: &Path,
    harness: Option<&HarnessRunConfig>,
    keep_temps: bool,
) -> Result<(), GenError> {
    replay_fuzz_artifact_dir_with_input(artifact_dir, generator_root, harness, keep_temps, None)
}

/// Like [`replay_fuzz_artifact_dir`], but allows overriding the recorded `fuzz_input_hex`.
///
/// This is intended for libFuzzer minimization: keep the same artifact “shape” (scenario + engine
/// configuration) while letting the fuzzer shrink/mutate the input bytes.
pub fn replay_fuzz_artifact_dir_with_input(
    artifact_dir: &Path,
    generator_root: &Path,
    harness: Option<&HarnessRunConfig>,
    keep_temps: bool,
    input_override: Option<&FuzzInput>,
) -> Result<(), GenError> {
    let meta_path = artifact_dir.join("meta.json");
    let scen_path = artifact_dir.join("scenario.json");
    let meta_raw = fs::read_to_string(&meta_path)
        .map_err(|e| GenError::Message(format!("replay: read {}: {e}", meta_path.display())))?;
    let meta: Value = serde_json::from_str(&meta_raw)
        .map_err(|e| GenError::Message(format!("replay: parse meta.json: {e}")))?;
    let scen_raw = fs::read_to_string(&scen_path)
        .map_err(|e| GenError::Message(format!("replay: read {}: {e}", scen_path.display())))?;
    let mut doc: Value = serde_json::from_str(&scen_raw)
        .map_err(|e| GenError::Message(format!("replay: parse scenario.json: {e}")))?;

    let input = if let Some(i) = input_override {
        i.clone()
    } else {
        let fuzz_hex = meta
            .get("fuzz_input_hex")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GenError::Message("replay: meta.json missing fuzz_input_hex".into()))?;
        FuzzInput::from_hex(fuzz_hex)
            .ok_or_else(|| GenError::Message("replay: invalid fuzz_input_hex".into()))?
    };
    let ai_seed = meta
        .get("ai_seed")
        .and_then(|v| v.as_u64())
        .or_else(|| meta.get("iteration_seed").and_then(|v| v.as_u64()))
        .unwrap_or(input.seed);

    let ai_mirror_rel = meta
        .get("ai_mirror_rel")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("test/ai"));

    let benchmark_java = meta
        .get("benchmark_java")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let fuzz_parity = meta
        .get("fuzz_parity")
        .or_else(|| meta.get("mutate_ai_require_compilable"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if fuzz_parity {
        // Keep replay behavior consistent with the `--fuzz-parity` runner: do not let Rust-only
        // ops-limit policy introduce AI errors when Java continues.
        doc["max_operations_per_entity"] = json!(1_000_000_000i64);
    }

    let replay_fuzz_cfg = FuzzConfig {
        generator_root: generator_root.to_path_buf(),
        scenario_rels: vec![],
        ai_rels: vec![],
        iterations: 1,
        master_seed: meta
            .get("master_seed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        fuzz_random_seed: true,
        shuffle_ais: true,
        restrict_ais_to_scenario: true,
        keep_temps,
        mutate_ai_level: input.mutate_ai_level,
        mutate_ai_inject_complexity: input.mutate_ai_inject_complexity,
        mutate_ai_inject_wrap_percent: input.mutate_ai_inject_wrap_percent,
        mutate_ai_inject_max_stmts: input.mutate_ai_inject_max_stmts,
        mutate_ai_require_parseable: input.mutate_ai_require_parseable == 255,
        jitter_entity_stats: false,
        jitter_max_turns: false,
        jitter_map_obstacles: false,
        jitter_entity_cells: false,
        jitter_max_operations: false,
        jitter_entity_loadouts: false,
        fuzz_draw_check_life: false,
        ai_mirror_rel: ai_mirror_rel.clone(),
        progress_report_every: 0,
        artifacts_dir: None,
        fuzz_parity,
        generate_scenarios: false,
        generate_scenarios_percent: 0,
        external_ai_files: vec![],
        external_ai_dirs: vec![],
        gen_min_entities_per_team: 1,
        gen_max_entities_per_team: 3,
        gen_min_map: 9,
        gen_max_map: 17,
        gen_loadout_percent: 0,
        require_compilable_ai: false,
        mutate_ramp: false,
        mutate_ramp_every: 200,
    };

    if benchmark_java {
        let Some(h) = harness else {
            return Err(GenError::Message(
                "replay: artifact requires Java harness; pass generator settings (do not --skip-java/--skip-rust)"
                    .into(),
            ));
        };

        // Always build a sandbox so paths are relative and AIs can be mutated consistently.
        let mut ai_rng = StdRng::seed_from_u64(ai_seed);
        let sand = build_harness_mutate_sandbox(&replay_fuzz_cfg, &doc, 0, &mut ai_rng, &input)?;
        let mut hcfg = h.clone();
        hcfg.runtime_cwd = Some(sand.clone());
        let req = RunRequest {
            file: PathBuf::from("_fuzz/scenario.json"),
            ..Default::default()
        };
        let report = run_scenario_harness(&req, &hcfg)?;
        cleanup_overlay(&sand, keep_temps);

        if report.engine_run_failed() {
            let err = report
                .engine_errors_display()
                .unwrap_or_else(|| "harness: engine failed (no error text)".into());
            return Err(GenError::Message(err));
        }
        if report.comparison_failed() {
            return Err(GenError::Message("replay: outcomes still mismatch".into()));
        }
        return Ok(());
    }

    // Rust-only replay.
    if input.mutate_ai_level == 0 {
        run_scenario_path(scen_path.as_path(), generator_root)?;
        return Ok(());
    }

    let overlay_dir = std::env::temp_dir().join(format!("leekgen_replay_ai_{:016x}", ai_seed));
    if overlay_dir.is_dir() {
        let _ = fs::remove_dir_all(&overlay_dir);
    }
    let mirror_src = generator_root.join(&ai_mirror_rel);
    copy_dir_recursive(&mirror_src, &overlay_dir.join(&ai_mirror_rel)).map_err(|e| {
        GenError::Message(format!("replay: mirror AI tree {}: {e}", mirror_src.display()))
    })?;
    let rels = discover_ai_rel_paths(&overlay_dir, ai_mirror_rel.as_path()).map_err(|e| {
        GenError::Message(format!(
            "replay: list .leek under {}: {e}",
            mirror_src.display()
        ))
    })?;
    let mut ai_rng = StdRng::seed_from_u64(ai_seed);
    apply_mutations_to_leek_files(&overlay_dir, &rels, &mut ai_rng, &input, &replay_fuzz_cfg)?;
    let res = run_scenario_path_with_ai_overlay(&scen_path, generator_root, Some(overlay_dir.as_path()));
    cleanup_overlay(&overlay_dir, keep_temps);
    res.map(|_| ())
}

/// Replay a saved fuzz artifact directory and return the Rust outcome JSON.
///
/// This is intended for the Rust-only `leekgen replay` UX. If the artifact was created with
/// `benchmark_java=true`, this returns an error (use `leekgen-compare --replay` instead).
pub fn replay_fuzz_artifact_dir_rust_outcome(
    artifact_dir: &Path,
    generator_root: &Path,
    keep_temps: bool,
) -> Result<String, GenError> {
    let meta_path = artifact_dir.join("meta.json");
    let scen_path = artifact_dir.join("scenario.json");
    let meta_raw = fs::read_to_string(&meta_path)
        .map_err(|e| GenError::Message(format!("replay: read {}: {e}", meta_path.display())))?;
    let meta: Value = serde_json::from_str(&meta_raw)
        .map_err(|e| GenError::Message(format!("replay: parse meta.json: {e}")))?;

    let benchmark_java = meta
        .get("benchmark_java")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if benchmark_java {
        return Err(GenError::Message(
            "replay: artifact requires Java harness; use `leekgen-compare --replay`".into(),
        ));
    }

    let fuzz_hex = meta
        .get("fuzz_input_hex")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GenError::Message("replay: meta.json missing fuzz_input_hex".into()))?;
    let input = FuzzInput::from_hex(fuzz_hex)
        .ok_or_else(|| GenError::Message("replay: invalid fuzz_input_hex".into()))?;

    let ai_seed = meta
        .get("ai_seed")
        .and_then(|v| v.as_u64())
        .or_else(|| meta.get("iteration_seed").and_then(|v| v.as_u64()))
        .unwrap_or(input.seed);

    let ai_mirror_rel = meta
        .get("ai_mirror_rel")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("test/ai"));

    // Rust-only replay.
    if input.mutate_ai_level == 0 {
        return run_scenario_path(scen_path.as_path(), generator_root);
    }

    // Mirror + mutate AIs into an overlay and run.
    let replay_fuzz_cfg = FuzzConfig {
        generator_root: generator_root.to_path_buf(),
        scenario_rels: vec![],
        ai_rels: vec![],
        iterations: 1,
        master_seed: meta
            .get("master_seed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        fuzz_random_seed: true,
        shuffle_ais: true,
        restrict_ais_to_scenario: true,
        keep_temps,
        mutate_ai_level: input.mutate_ai_level,
        mutate_ai_inject_complexity: input.mutate_ai_inject_complexity,
        mutate_ai_inject_wrap_percent: input.mutate_ai_inject_wrap_percent,
        mutate_ai_inject_max_stmts: input.mutate_ai_inject_max_stmts,
        mutate_ai_require_parseable: input.mutate_ai_require_parseable == 255,
        jitter_entity_stats: false,
        jitter_max_turns: false,
        jitter_map_obstacles: false,
        jitter_entity_cells: false,
        jitter_max_operations: false,
        jitter_entity_loadouts: false,
        fuzz_draw_check_life: false,
        ai_mirror_rel: ai_mirror_rel.clone(),
        progress_report_every: 0,
        artifacts_dir: None,
        fuzz_parity: false,
        generate_scenarios: false,
        generate_scenarios_percent: 0,
        external_ai_files: vec![],
        external_ai_dirs: vec![],
        gen_min_entities_per_team: 1,
        gen_max_entities_per_team: 3,
        gen_min_map: 9,
        gen_max_map: 17,
        gen_loadout_percent: 0,
        require_compilable_ai: false,
        mutate_ramp: false,
        mutate_ramp_every: 200,
    };

    let overlay_dir = std::env::temp_dir().join(format!("leekgen_replay_ai_{:016x}", ai_seed));
    if overlay_dir.is_dir() {
        let _ = fs::remove_dir_all(&overlay_dir);
    }
    let mirror_src = generator_root.join(&ai_mirror_rel);
    copy_dir_recursive(&mirror_src, &overlay_dir.join(&ai_mirror_rel)).map_err(|e| {
        GenError::Message(format!("replay: mirror AI tree {}: {e}", mirror_src.display()))
    })?;
    let rels = discover_ai_rel_paths(&overlay_dir, ai_mirror_rel.as_path()).map_err(|e| {
        GenError::Message(format!(
            "replay: list .leek under {}: {e}",
            mirror_src.display()
        ))
    })?;
    let mut ai_rng = StdRng::seed_from_u64(ai_seed);
    apply_mutations_to_leek_files(&overlay_dir, &rels, &mut ai_rng, &input, &replay_fuzz_cfg)?;
    let res =
        run_scenario_path_with_ai_overlay(&scen_path, generator_root, Some(overlay_dir.as_path()));
    cleanup_overlay(&overlay_dir, keep_temps);
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn apply_fuzz_variants_deterministic() {
        let mut doc = json!({
            "entities": [[{"ai": "a.leek", "id": 1}]],
            "random_seed": 1
        });
        let pool = vec!["x.leek".to_string(), "y.leek".to_string()];
        let mut rng = StdRng::seed_from_u64(42);
        let mut input = FuzzInput::default();
        input.p_fuzz_random_seed = 255;
        input.p_shuffle_ais = 255;
        apply_fuzz_variants(&mut doc, &mut rng, &pool, &input);
        assert!(doc.get("random_seed").and_then(|v| v.as_i64()).is_some());
        let ai = doc["entities"][0][0]["ai"].as_str().unwrap();
        assert!(ai == "x.leek" || ai == "y.leek");
    }

    #[test]
    fn ai_pool_prefers_document_paths() {
        let doc = json!({
            "entities": [[{"ai": "foo/a.leek"}, {"ai": "foo/b.leek"}]]
        });
        let pool = ai_pool_from_document(&doc, &["fallback.leek".to_string()]);
        assert_eq!(pool, vec!["foo/a.leek", "foo/b.leek"]);
    }

    #[test]
    fn collect_leek_ai_paths_sorted_deduped() {
        let doc = json!({
            "entities": [[
                {"ai": "z.leek", "id": 1},
                {"ai": "a.leek", "id": 2},
                {"ai": "z.leek", "id": 3}
            ]]
        });
        assert_eq!(
            collect_leek_ai_paths_from_entities(&doc),
            vec!["a.leek", "z.leek"]
        );
    }

    #[test]
    fn mutate_leek_level1_appends_marker() {
        let mut rng = StdRng::seed_from_u64(99);
        let out = mutate_leek_source("x = 1", &mut rng, 1);
        assert!(out.contains("leekgen-fuzz:"));
        assert!(out.starts_with("x = 1"));
    }

    #[test]
    fn fuzz_parity_profile_zeros_inject_weights() {
        let cfg = FuzzConfig {
            generator_root: PathBuf::new(),
            scenario_rels: vec![],
            ai_rels: vec![],
            iterations: 1,
            master_seed: 0,
            fuzz_random_seed: false,
            shuffle_ais: false,
            restrict_ais_to_scenario: true,
            keep_temps: false,
            mutate_ai_level: 4,
            mutate_ai_inject_complexity: 5,
            mutate_ai_inject_wrap_percent: 100,
            mutate_ai_inject_max_stmts: 8,
            mutate_ai_require_parseable: false,
            jitter_entity_stats: false,
            jitter_max_turns: false,
            jitter_map_obstacles: false,
            jitter_entity_cells: false,
            jitter_max_operations: false,
            jitter_entity_loadouts: false,
            fuzz_draw_check_life: false,
            ai_mirror_rel: PathBuf::from("ai"),
            progress_report_every: 0,
            artifacts_dir: None,
            fuzz_parity: true,
            generate_scenarios: false,
            generate_scenarios_percent: 0,
            external_ai_files: vec![],
            external_ai_dirs: vec![],
            gen_min_entities_per_team: 1,
            gen_max_entities_per_team: 3,
            gen_min_map: 9,
            gen_max_map: 17,
            gen_loadout_percent: 0,
            require_compilable_ai: false,
            mutate_ramp: false,
            mutate_ramp_every: 200,
        };
        let input = fuzz_input_from_cfg(&cfg, 1);
        assert_eq!(input.mutate_ai_inject_complexity, 0);
        assert_eq!(input.mutate_ai_inject_wrap_percent, 0);
    }

    #[test]
    fn apply_fuzz_map_cells_loadout_smoke() {
        let mut doc = json!({
            "map": { "width": 3, "height": 3, "obstacles": [1, 2] },
            "entities": [[
                {"ai": "a.leek", "cell": 0, "weapons": [10], "chips": []}
            ]],
            "max_operations_per_entity": 1_000_000
        });
        let mut rng = StdRng::seed_from_u64(7);
        apply_fuzz_map_obstacles(&mut doc, &mut rng);
        assert!(doc["map"]["obstacles"].as_array().unwrap().len() >= 1);
        apply_fuzz_entity_cells(&mut doc, &mut rng);
        let c = doc["entities"][0][0]["cell"].as_i64().unwrap();
        assert!((0..9).contains(&c));
        apply_fuzz_max_operations_per_entity(&mut doc, &mut rng);
        assert!(doc["max_operations_per_entity"].as_i64().unwrap() >= 50_000);
        apply_fuzz_entity_loadouts(&mut doc, &mut rng);
        assert!(doc["entities"][0][0]["weapons"].as_array().unwrap().len() >= 1);
    }

    #[test]
    fn generate_scenario_from_scratch_smoke() {
        let mut rng = StdRng::seed_from_u64(123);
        let ai_pool = vec![
            "test/ai/basic.leek".to_string(),
            "test/ai/ops_math.leek".to_string(),
        ];
        let cfg = FuzzConfig {
            generator_root: PathBuf::new(),
            scenario_rels: vec![],
            ai_rels: vec![],
            iterations: 1,
            master_seed: 0,
            fuzz_random_seed: true,
            shuffle_ais: true,
            restrict_ais_to_scenario: true,
            keep_temps: false,
            mutate_ai_level: 0,
            mutate_ai_inject_complexity: 0,
            mutate_ai_inject_wrap_percent: 0,
            mutate_ai_inject_max_stmts: 1,
            mutate_ai_require_parseable: false,
            jitter_entity_stats: false,
            jitter_max_turns: false,
            jitter_map_obstacles: false,
            jitter_entity_cells: false,
            jitter_max_operations: false,
            jitter_entity_loadouts: false,
            fuzz_draw_check_life: false,
            ai_mirror_rel: PathBuf::from("test/ai"),
            progress_report_every: 0,
            artifacts_dir: None,
            fuzz_parity: false,
            generate_scenarios: true,
            generate_scenarios_percent: 100,
            external_ai_files: vec![],
            external_ai_dirs: vec![],
            gen_min_entities_per_team: 1,
            gen_max_entities_per_team: 3,
            gen_min_map: 9,
            gen_max_map: 15,
            gen_loadout_percent: 50,
            require_compilable_ai: false,
            mutate_ramp: false,
            mutate_ramp_every: 200,
        };
        let doc = generate_scenario_from_scratch(&cfg, &mut rng, &ai_pool, 424242);
        assert!(doc.get("farmers").is_some());
        assert!(doc.get("teams").is_some());
        assert!(doc.get("entities").is_some());
        assert!(doc.get("map").is_some());
        assert_eq!(doc.get("random_seed").and_then(|v| v.as_i64()), Some(424242));

        let entities = doc
            .get("entities")
            .and_then(|v| v.as_array())
            .expect("entities array");
        assert_eq!(entities.len(), 2, "2 teams");
        let n_total: usize = entities
            .iter()
            .map(|team| team.as_array().map(|a| a.len()).unwrap_or(0))
            .sum();
        assert!(n_total >= 2, "at least 1 entity per team");
    }
}
