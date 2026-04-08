use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Generator, Outcome, Scenario};

/// Hard limit so a typo in axis lengths or seed lists does not allocate billions of runs.
const MAX_BATCH_RUNS: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    pub mode: BatchMode,
    /// If set, each generated run gets a deterministic seed:
    /// `scenario.random_seed = base + run_index` (run index after all other expansion).
    /// Mutually exclusive with [`BatchJob::random_seeds`].
    #[serde(default)]
    pub seed_schedule_base: Option<i32>,

    /// When non-empty, **every** expanded plan is run once per listed seed (`scenario.random_seed` set each time).
    /// Run labels get a `_seed<value>` suffix. Total runs = `plans × random_seeds.len()`.
    /// Mutually exclusive with [`BatchJob::seed_schedule_base`].
    #[serde(default)]
    pub random_seeds: Vec<i32>,

    /// Parallel worker threads. `None` (field omitted) uses the host's logical CPU count when possible
    /// (on Unix: online processors via `_SC_NPROCESSORS_ONLN`; otherwise
    /// [`std::thread::available_parallelism`]). `Some(0)` forces sequential execution.
    /// `Some(n)` for `n >= 1` caps concurrency at `n` (still sequential when `n <= 1` or only one plan).
    #[serde(default)]
    pub parallel_workers: Option<usize>,
}

impl BatchJob {
    /// Resolved worker count for scheduling: omitted → max usable host threads; `Some(0)` → `0` (sequential); `Some(n)` → `n`.
    pub fn resolved_parallel_workers(&self) -> usize {
        match self.parallel_workers {
            None => default_max_parallel_worker_threads(),
            Some(0) => 0,
            Some(n) => n,
        }
    }
}

/// Best-effort logical CPU / online processor count for “use all threads” batch defaults.
fn default_max_parallel_worker_threads() -> usize {
    #[cfg(unix)]
    {
        // Prefer OS online processor count; `available_parallelism` may be cgroup-capped lower.
        let n = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
        if n > 0 {
            return (n as usize).max(1);
        }
    }
    std::thread::available_parallelism()
        .map(|x| x.get())
        .unwrap_or(1)
        .max(1)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BatchMode {
    /// Run a list of scenario files as-is.
    Scenarios { scenarios: Vec<PathBuf> },
    /// Run the same base scenario with per-variant overrides (useful for optimizations / sweeps).
    Sweep {
        base_scenario: PathBuf,
        /// Hand-written variants (each `[[mode.variants]]` in TOML).
        #[serde(default)]
        variants: Vec<SweepVariant>,
        /// Optional Cartesian product of per-entity axes — expands to `∏ |axis|` total runs (capped, see `MAX_BATCH_RUNS`).
        /// Mutually exclusive with a non-empty `variants` list.
        #[serde(default)]
        cartesian: Option<SweepCartesian>,
    },
    /// Round-robin tournament: run every pair of competitors into fixed entity slots.
    RoundRobin {
        base_scenario: PathBuf,
        /// Entity IDs in the scenario to fill with competitor AIs (usually two).
        slots: Vec<i32>,
        competitors: Vec<Competitor>,
        /// Repeat each pairing this many times.
        #[serde(default)]
        repeat: u32,
        /// Also run swapped sides (A↔B) for each pairing.
        #[serde(default)]
        swap_sides: bool,
    },
    /// Run the same base scenario against many enemy variants (useful for benchmarking one AI).
    VersusManyEnemies {
        base_scenario: PathBuf,
        #[serde(default)]
        hero_overrides: Vec<EntityOverride>,
        enemies: Vec<SweepVariant>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Competitor {
    pub name: String,
    pub ai: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepVariant {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub overrides: Vec<EntityOverride>,
}

/// Cartesian sweep: each [`EntityCartesianBlock`] contributes one axis bundle; the **product across blocks**
/// is combined into one run (multiple entities updated per variant). Within a block, all non-empty axes are multiplied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepCartesian {
    /// Label prefix for runs (`{prefix}_00042`). Default `cart`.
    #[serde(default)]
    pub name_prefix: Option<String>,
    pub blocks: Vec<EntityCartesianBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityCartesianBlock {
    pub entity_id: i32,
    /// Keys are `extra` stat names; values are lists to multiply (e.g. `life = [1000, 2000]`).
    #[serde(default)]
    pub extra_axes: BTreeMap<String, Vec<Value>>,
    /// Each inner array is one `weapons` replacement option. Omitted or `[]` = do not sweep weapons.
    #[serde(default)]
    pub weapons_axis: Vec<Vec<i32>>,
    /// Same for chips.
    #[serde(default)]
    pub chips_axis: Vec<Vec<i32>>,
    /// Sweep `level`; empty = do not sweep.
    #[serde(default)]
    pub level_axis: Vec<i32>,
    /// Sweep starting `cell`; empty = do not sweep.
    #[serde(default)]
    pub cell_axis: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityOverride {
    pub entity_id: i32,
    /// Key/value pairs merged into the entity `extra` map (e.g. `life`, `tp`, `mp`, `strength`, `agility`, `science`, `magic`, `resistance`, `wisdom`, `power`, `cores`, `frequency`, …).
    #[serde(default)]
    pub extra: BTreeMap<String, serde_json::Value>,
    /// Optional AI path override (relative to scenario dir or generator root).
    #[serde(default)]
    pub ai: Option<String>,
    /// When set, replaces the entity’s weapon **item** ids (same as scenario JSON `weapons`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapons: Option<Vec<i32>>,
    /// When set, replaces the entity’s chip **item** ids (same as scenario JSON `chips`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chips: Option<Vec<i32>>,
    /// When set, replaces the entity’s `level` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<i32>,
    /// When set, replaces the entity’s starting `cell`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchResult {
    pub outcomes: Vec<Outcome>,
    /// Human-readable id for each run, aligned with `outcomes` (scenario path, variant name, matchup, …).
    pub run_labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<BatchSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchSummary {
    pub total: usize,
    pub winners: BTreeMap<i32, usize>,
    pub outcome_hashes_sha1: BTreeSet<String>,
}

#[derive(Debug)]
pub struct BatchRunner {
    pub generator: Generator,
    /// When true (default), print run progress to stderr during `run` / `run_to_jsonl`.
    pub show_progress: bool,
}

impl Default for BatchRunner {
    fn default() -> Self {
        Self {
            generator: Generator::new(),
            show_progress: true,
        }
    }
}

impl BatchRunner {
    pub fn load_job_from_file(path: impl AsRef<Path>) -> miette::Result<BatchJob> {
        let path = path.as_ref();
        let src = std::fs::read_to_string(path)
            .map_err(|e| miette::miette!("failed to read batch job `{}`: {e}", path.display()))?;
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "json" || ext.is_empty() {
            return serde_json::from_str::<BatchJob>(&src)
                .map_err(|e| miette::miette!("failed to parse batch job json: {e}"));
        }
        if ext == "toml" {
            #[cfg(feature = "toml")]
            {
                let tv: toml::Value =
                    toml::from_str(&src).map_err(|e| miette::miette!("failed to parse batch job toml: {e}"))?;
                let jv = crate::util::toml_bridge::toml_to_json(&tv);
                return serde_json::from_value::<BatchJob>(jv)
                    .map_err(|e| miette::miette!("failed to decode batch job from toml: {e}"));
            }
            #[cfg(not(feature = "toml"))]
            {
                return Err(miette::miette!(
                    "batch job `{}` is TOML but this binary was built without TOML support (enable cargo feature `toml`)",
                    path.display()
                ));
            }
        }
        Err(miette::miette!(
            "unsupported batch job extension for `{}` (expected .json or .toml)",
            path.display()
        ))
    }

    pub fn run(&self, job: &BatchJob) -> miette::Result<BatchResult> {
        let plans = expand_job(job)?;
        let outcomes = run_plans_for_job(&self.generator, job, &plans, self.show_progress)?;
        let run_labels = plans.iter().map(|p| p.label.clone()).collect();
        let summary = summarize(&outcomes);
        Ok(BatchResult {
            outcomes,
            run_labels,
            summary: Some(summary),
        })
    }

    pub fn run_to_jsonl(&self, job: &BatchJob, out: impl AsRef<Path>) -> miette::Result<()> {
        let out = out.as_ref();
        let f = std::fs::File::create(out).map_err(|e| miette::miette!("failed to create `{}`: {e}", out.display()))?;
        let mut w = std::io::BufWriter::new(f);
        let write_outcome = |w: &mut std::io::BufWriter<std::fs::File>, o: &Outcome| -> miette::Result<()> {
            let line = serde_json::to_string(o).unwrap_or_else(|_| "{}".into());
            w.write_all(line.as_bytes())
                .map_err(|e| miette::miette!("failed to write `{}`: {e}", out.display()))?;
            w.write_all(b"\n")
                .map_err(|e| miette::miette!("failed to write `{}`: {e}", out.display()))?;
            Ok(())
        };

        let plans = expand_job(job)?;
        let outcomes = run_plans_for_job(&self.generator, job, &plans, self.show_progress)?;
        for o in outcomes {
            write_outcome(&mut w, &o)?;
        }
        w.flush()
            .map_err(|e| miette::miette!("failed to flush `{}`: {e}", out.display()))?;
        Ok(())
    }
}

fn apply_overrides(scenario: &mut Scenario, overrides: &[EntityOverride]) {
    for ov in overrides {
        for team in &mut scenario.entities {
            for ent in team {
                if ent.id == ov.entity_id {
                    for (k, v) in &ov.extra {
                        ent.extra.insert(k.clone(), v.clone());
                    }
                    if let Some(ai) = ov.ai.as_deref() {
                        ent.ai = Some(ai.to_string());
                    }
                    if let Some(w) = &ov.weapons {
                        ent.weapons = w.clone();
                    }
                    if let Some(c) = &ov.chips {
                        ent.chips = c.clone();
                    }
                    if let Some(level) = ov.level {
                        ent.level = Some(level);
                    }
                    if let Some(cell) = ov.cell {
                        ent.cell = Some(cell);
                    }
                }
            }
        }
    }
}

fn entity_override_ai_only(entity_id: i32, ai: impl Into<String>) -> EntityOverride {
    EntityOverride {
        entity_id,
        extra: BTreeMap::new(),
        ai: Some(ai.into()),
        weapons: None,
        chips: None,
        level: None,
        cell: None,
    }
}

fn format_cartesian_run_label(c: &SweepCartesian, idx: usize) -> String {
    let p = c.name_prefix.as_deref().unwrap_or("cart");
    format!("{p}_{idx:05}")
}

fn expand_entity_cartesian_block_count(block: &EntityCartesianBlock) -> miette::Result<usize> {
    let extra_n: usize = if block.extra_axes.is_empty() {
        1
    } else {
        block.extra_axes.values().map(|v| v.len()).product()
    };
    if extra_n == 0 {
        return Err(miette::miette!(
            "cartesian block for entity {} has an empty value list under `extra_axes`",
            block.entity_id
        ));
    }
    let w_n = if block.weapons_axis.is_empty() {
        1
    } else {
        block.weapons_axis.len()
    };
    let c_n = if block.chips_axis.is_empty() {
        1
    } else {
        block.chips_axis.len()
    };
    let lv_n = if block.level_axis.is_empty() {
        1
    } else {
        block.level_axis.len()
    };
    let cell_n = if block.cell_axis.is_empty() {
        1
    } else {
        block.cell_axis.len()
    };
    extra_n
        .checked_mul(w_n)
        .and_then(|x| x.checked_mul(c_n))
        .and_then(|x| x.checked_mul(lv_n))
        .and_then(|x| x.checked_mul(cell_n))
        .ok_or_else(|| miette::miette!("cartesian combination count overflow for entity {}", block.entity_id))
}

fn cartesian_extra_maps(axes: &BTreeMap<String, Vec<Value>>) -> Vec<BTreeMap<String, Value>> {
    if axes.is_empty() {
        return vec![BTreeMap::new()];
    }
    let keys: Vec<String> = axes.keys().cloned().collect();
    let mut acc = vec![BTreeMap::new()];
    for k in keys {
        let vals = &axes[&k];
        let mut next = Vec::with_capacity(acc.len() * vals.len());
        for base in &acc {
            for v in vals {
                let mut m = base.clone();
                m.insert(k.clone(), v.clone());
                next.push(m);
            }
        }
        acc = next;
    }
    acc
}

fn expand_entity_cartesian_block(block: &EntityCartesianBlock) -> miette::Result<Vec<EntityOverride>> {
    for (k, vals) in &block.extra_axes {
        if vals.is_empty() {
            return Err(miette::miette!(
                "cartesian `extra_axes.{k}` for entity {} must list at least one value",
                block.entity_id
            ));
        }
    }
    if !block.weapons_axis.is_empty() {
        for (i, w) in block.weapons_axis.iter().enumerate() {
            if w.is_empty() {
                return Err(miette::miette!(
                    "cartesian `weapons_axis[{i}]` for entity {} must be a non-empty weapon id list",
                    block.entity_id
                ));
            }
        }
    }
    if !block.chips_axis.is_empty() {
        for (i, ch) in block.chips_axis.iter().enumerate() {
            if ch.is_empty() {
                return Err(miette::miette!(
                    "cartesian `chips_axis[{i}]` for entity {} must be a non-empty chip id list",
                    block.entity_id
                ));
            }
        }
    }

    let extras = cartesian_extra_maps(&block.extra_axes);
    let weapons_choices: Vec<Option<Vec<i32>>> = if block.weapons_axis.is_empty() {
        vec![None]
    } else {
        block.weapons_axis.clone().into_iter().map(Some).collect()
    };
    let chips_choices: Vec<Option<Vec<i32>>> = if block.chips_axis.is_empty() {
        vec![None]
    } else {
        block.chips_axis.clone().into_iter().map(Some).collect()
    };
    let level_choices: Vec<Option<i32>> = if block.level_axis.is_empty() {
        vec![None]
    } else {
        block.level_axis.iter().copied().map(Some).collect()
    };
    let cell_choices: Vec<Option<i32>> = if block.cell_axis.is_empty() {
        vec![None]
    } else {
        block.cell_axis.iter().copied().map(Some).collect()
    };

    let mut out = Vec::new();
    for extra in extras {
        for w in &weapons_choices {
            for c in &chips_choices {
                for lv in &level_choices {
                    for cl in &cell_choices {
                        out.push(EntityOverride {
                            entity_id: block.entity_id,
                            extra: extra.clone(),
                            ai: None,
                            weapons: w.clone(),
                            chips: c.clone(),
                            level: *lv,
                            cell: *cl,
                        });
                    }
                }
            }
        }
    }
    Ok(out)
}

fn cartesian_merge_override_rows(blocks: Vec<Vec<EntityOverride>>) -> miette::Result<Vec<Vec<EntityOverride>>> {
    let mut rows: Vec<Vec<EntityOverride>> = vec![vec![]];
    for choices in blocks {
        if choices.is_empty() {
            return Err(miette::miette!("internal: empty cartesian block"));
        }
        let mut next = Vec::with_capacity(rows.len() * choices.len());
        for prefix in rows {
            for one in &choices {
                let mut row = prefix.clone();
                row.push(one.clone());
                next.push(row);
            }
        }
        rows = next;
    }
    Ok(rows)
}

fn expand_sweep_cartesian(c: &SweepCartesian) -> miette::Result<Vec<Vec<EntityOverride>>> {
    if c.blocks.is_empty() {
        return Err(miette::miette!("`cartesian.blocks` must not be empty"));
    }
    let mut total = 1usize;
    for b in &c.blocks {
        let bn = expand_entity_cartesian_block_count(b)?;
        total = total
            .checked_mul(bn)
            .ok_or_else(|| miette::miette!("cartesian variant count overflow"))?;
    }
    if total == 0 {
        return Err(miette::miette!("cartesian product is empty"));
    }
    if total > MAX_BATCH_RUNS {
        return Err(miette::miette!(
            "Sweep cartesian expands to {total} runs (max {MAX_BATCH_RUNS}). Shrink axes or split jobs."
        ));
    }

    let per_block: Vec<Vec<EntityOverride>> = c
        .blocks
        .iter()
        .map(expand_entity_cartesian_block)
        .collect::<Result<Vec<_>, _>>()?;
    cartesian_merge_override_rows(per_block)
}

#[derive(Debug, Clone)]
struct RunPlan {
    scenario: Scenario,
    scenario_dir: PathBuf,
    generator_root: PathBuf,
    label: String,
}

fn expand_job(job: &BatchJob) -> miette::Result<Vec<RunPlan>> {
    let mut out: Vec<RunPlan> = Vec::new();
    match &job.mode {
        BatchMode::Scenarios { scenarios } => {
            for p in scenarios {
                let (scenario, scenario_dir, generator_root) = load_scenario_from_file(p)?;
                let label = p.to_string_lossy().to_string();
                out.push(RunPlan {
                    scenario,
                    scenario_dir,
                    generator_root,
                    label,
                });
            }
        }
        BatchMode::Sweep {
            base_scenario,
            variants,
            cartesian,
        } => {
            let (base, scenario_dir, generator_root) = load_scenario_from_file(base_scenario)?;
            let has_variants = !variants.is_empty();
            let has_cart = cartesian
                .as_ref()
                .is_some_and(|c| !c.blocks.is_empty());
            if has_variants && has_cart {
                return Err(miette::miette!(
                    "Sweep mode: set either `variants` or `cartesian`, not both (remove one)"
                ));
            }
            if has_cart {
                let c = cartesian.as_ref().expect("has_cart");
                let rows = expand_sweep_cartesian(c)?;
                for (idx, overrides) in rows.into_iter().enumerate() {
                    let mut scenario = base.clone();
                    apply_overrides(&mut scenario, &overrides);
                    let label = format_cartesian_run_label(c, idx);
                    out.push(RunPlan {
                        scenario,
                        scenario_dir: scenario_dir.clone(),
                        generator_root: generator_root.clone(),
                        label,
                    });
                }
            } else if has_variants {
                for (idx, var) in variants.iter().enumerate() {
                    let mut scenario = base.clone();
                    apply_overrides(&mut scenario, &var.overrides);
                    let label = var
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("variant_{idx}"));
                    out.push(RunPlan {
                        scenario,
                        scenario_dir: scenario_dir.clone(),
                        generator_root: generator_root.clone(),
                        label,
                    });
                }
            } else {
                return Err(miette::miette!(
                    "Sweep mode needs non-empty `variants` or `cartesian` with at least one block"
                ));
            }
        }
        BatchMode::VersusManyEnemies {
            base_scenario,
            hero_overrides,
            enemies,
        } => {
            let (base, scenario_dir, generator_root) = load_scenario_from_file(base_scenario)?;
            for (idx, enemy) in enemies.iter().enumerate() {
                let mut scenario = base.clone();
                apply_overrides(&mut scenario, hero_overrides);
                apply_overrides(&mut scenario, &enemy.overrides);
                let label = enemy
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("enemy_{idx}"));
                out.push(RunPlan {
                    scenario,
                    scenario_dir: scenario_dir.clone(),
                    generator_root: generator_root.clone(),
                    label,
                });
            }
        }
        BatchMode::RoundRobin {
            base_scenario,
            slots,
            competitors,
            repeat,
            swap_sides,
        } => {
            let (base, scenario_dir, generator_root) = load_scenario_from_file(base_scenario)?;
            if slots.len() < 2 {
                return Err(miette::miette!("RoundRobin requires at least 2 slots"));
            }
            let a_slot = slots[0];
            let b_slot = slots[1];
            let reps = (*repeat).max(1);
            for i in 0..competitors.len() {
                for j in (i + 1)..competitors.len() {
                    for r in 0..reps {
                        // i vs j
                        let mut scenario = base.clone();
                        apply_overrides(
                            &mut scenario,
                            &[
                                entity_override_ai_only(a_slot, competitors[i].ai.to_string_lossy().to_string()),
                                entity_override_ai_only(b_slot, competitors[j].ai.to_string_lossy().to_string()),
                            ],
                        );
                        let mut label = if reps > 1 {
                            format!(
                                "{} vs {} (#{})",
                                competitors[i].name, competitors[j].name, r + 1
                            )
                        } else {
                            format!("{} vs {}", competitors[i].name, competitors[j].name)
                        };
                        out.push(RunPlan {
                            scenario,
                            scenario_dir: scenario_dir.clone(),
                            generator_root: generator_root.clone(),
                            label,
                        });

                        if *swap_sides {
                            let mut scenario = base.clone();
                            apply_overrides(
                                &mut scenario,
                                &[
                                    entity_override_ai_only(a_slot, competitors[j].ai.to_string_lossy().to_string()),
                                    entity_override_ai_only(b_slot, competitors[i].ai.to_string_lossy().to_string()),
                                ],
                            );
                            label = if reps > 1 {
                                format!(
                                    "{} vs {} (#{}, swap sides)",
                                    competitors[i].name, competitors[j].name, r + 1
                                )
                            } else {
                                format!("{} vs {} (swap sides)", competitors[i].name, competitors[j].name)
                            };
                            out.push(RunPlan {
                                scenario,
                                scenario_dir: scenario_dir.clone(),
                                generator_root: generator_root.clone(),
                                label,
                            });
                        }
                    }
                }
            }
        }
    }

    let use_random_seeds = !job.random_seeds.is_empty();
    if use_random_seeds && job.seed_schedule_base.is_some() {
        return Err(miette::miette!(
            "use only one of `random_seeds` or `seed_schedule_base`, not both"
        ));
    }

    if use_random_seeds {
        let seeds = &job.random_seeds;
        let n = out
            .len()
            .checked_mul(seeds.len())
            .ok_or_else(|| miette::miette!("random_seeds expansion overflow"))?;
        if n > MAX_BATCH_RUNS {
            return Err(miette::miette!(
                "`random_seeds` would produce {n} runs (max {MAX_BATCH_RUNS}). Use fewer seeds or fewer plans."
            ));
        }
        let mut expanded: Vec<RunPlan> = Vec::with_capacity(n);
        for p in out {
            for s in seeds {
                let mut q = p.clone();
                q.scenario.random_seed = Some(*s);
                q.label = format!("{}_seed{}", p.label, s);
                expanded.push(q);
            }
        }
        out = expanded;
    } else if let Some(base) = job.seed_schedule_base {
        for (i, p) in out.iter_mut().enumerate() {
            let seed = base.saturating_add(i as i32);
            p.scenario.random_seed = Some(seed);
        }
    }

    Ok(out)
}

fn run_plans_for_job(
    generator: &Generator,
    job: &BatchJob,
    plans: &[RunPlan],
    show_progress: bool,
) -> miette::Result<Vec<Outcome>> {
    let workers = job.resolved_parallel_workers();
    let use_parallel = workers > 1 && plans.len() > 1 && generator.register_manager.is_none();
    if show_progress && !plans.is_empty() {
        if use_parallel {
            let w = workers.max(1).min(plans.len());
            eprintln!("[batch] {} runs · parallel ({} workers)", plans.len(), w);
        } else {
            eprintln!("[batch] {} runs · sequential", plans.len());
        }
    }
    if !use_parallel {
        run_plans_sequential(generator, plans, show_progress)
    } else {
        run_plans_parallel(generator, plans, workers, show_progress)
    }
}

fn run_plans_sequential(
    generator: &Generator,
    plans: &[RunPlan],
    show_progress: bool,
) -> miette::Result<Vec<Outcome>> {
    let n = plans.len();
    let mut out = Vec::with_capacity(n);
    for (i, p) in plans.iter().enumerate() {
        if show_progress {
            eprintln!(
                "[batch {}/{}] running: {}",
                i + 1,
                n,
                progress_label(&p.label)
            );
        }
        let o = generator.run_scenario(&p.scenario, &p.scenario_dir, &p.generator_root)?;
        if show_progress {
            eprintln!(
                "[batch {}/{}] done: {} — {} turns, winning team {}",
                i + 1,
                n,
                progress_label(&p.label),
                o.duration,
                o.winner
            );
        }
        out.push(o);
    }
    Ok(out)
}

fn run_plans_parallel(
    generator: &Generator,
    plans: &[RunPlan],
    workers: usize,
    show_progress: bool,
) -> miette::Result<Vec<Outcome>> {
    // Important: `Generator` holds an `Rc<dyn RegisterManager>` which is not Send/Sync.
    // For parallel runs we therefore clone a "thread-safe subset" (no registers).
    let workers = workers.max(1).min(plans.len().max(1));

    let verbose = generator.verbose;
    let signature_files = generator.signature_files.clone();
    let trace_entity = generator.trace_entity;

    let (work_tx, work_rx) = mpsc::channel::<(usize, RunPlan)>();
    let (res_tx, res_rx) = mpsc::channel::<(usize, miette::Result<Outcome>)>();
    let work_rx = std::sync::Arc::new(std::sync::Mutex::new(work_rx));

    // Spawn worker threads.
    let mut handles = Vec::new();
    for _ in 0..workers {
        let rx = work_rx.clone();
        let tx = res_tx.clone();
        let sig = signature_files.clone();
        handles.push(std::thread::spawn(move || {
            let g = Generator {
                verbose,
                signature_files: sig,
                register_manager: None,
                trace_entity,
            };
            loop {
                let msg = {
                    let guard = rx.lock().expect("work_rx poisoned");
                    guard.recv()
                };
                let Ok((i, p)) = msg else { break };
                let r = g.run_scenario(&p.scenario, &p.scenario_dir, &p.generator_root);
                let _ = tx.send((i, r));
            }
        }));
    }

    // Send work.
    for (i, p) in plans.iter().cloned().enumerate() {
        work_tx
            .send((i, p))
            .map_err(|e| miette::miette!("failed to send work to worker: {e}"))?;
    }
    drop(work_tx);
    drop(res_tx);

    // Collect results deterministically by index.
    let mut tmp: Vec<Option<Outcome>> = vec![None; plans.len()];
    let total = plans.len();
    for k in 0..total {
        let (i, r) = res_rx.recv().map_err(|e| miette::miette!("{e}"))?;
        let o = r?;
        if show_progress {
            let label = plans
                .get(i)
                .map(|p| progress_label(&p.label))
                .unwrap_or_else(|| "?".into());
            eprintln!(
                "[batch {}/{}] done #{}: {} — {} turns, winning team {}",
                k + 1,
                total,
                i,
                label,
                o.duration,
                o.winner
            );
        }
        tmp[i] = Some(o);
    }

    for h in handles {
        let _ = h.join();
    }

    Ok(tmp.into_iter().map(|o| o.expect("filled")).collect())
}

/// One-line, stderr-safe label (no newlines; capped length).
fn progress_label(s: &str) -> String {
    let one_line: String = s
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    const MAX: usize = 72;
    if one_line.chars().count() > MAX {
        let t: String = one_line.chars().take(MAX.saturating_sub(1)).collect();
        format!("{t}…")
    } else {
        one_line
    }
}

fn summarize(outcomes: &[Outcome]) -> BatchSummary {
    let mut winners: BTreeMap<i32, usize> = BTreeMap::new();
    let mut hashes: BTreeSet<String> = BTreeSet::new();
    for o in outcomes {
        *winners.entry(o.winner).or_insert(0) += 1;
        if let Some(h) = o.logs.get("outcome_hash_sha1").and_then(|v| v.as_str()) {
            hashes.insert(h.to_string());
        }
    }
    BatchSummary {
        total: outcomes.len(),
        winners,
        outcome_hashes_sha1: hashes,
    }
}

fn load_scenario_from_file(path: &Path) -> miette::Result<(Scenario, PathBuf, PathBuf)> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| miette::miette!("failed to read scenario `{}`: {e}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let scenario: Scenario = if ext == "json" || ext.is_empty() {
        serde_json::from_str(&src).map_err(|e| miette::miette!("failed to parse scenario json: {e}"))?
    } else if ext == "toml" {
        #[cfg(feature = "toml")]
        {
            let tv: toml::Value =
                toml::from_str(&src).map_err(|e| miette::miette!("failed to parse scenario toml: {e}"))?;
            let jv = crate::util::toml_bridge::toml_to_json(&tv);
            serde_json::from_value(jv).map_err(|e| miette::miette!("failed to decode scenario from toml: {e}"))?
        }
        #[cfg(not(feature = "toml"))]
        {
            return Err(miette::miette!(
                "scenario `{}` is TOML but this binary was built without TOML support (enable cargo feature `toml`)",
                path.display()
            ));
        }
    } else {
        return Err(miette::miette!(
            "unsupported scenario file extension for `{}` (expected .json or .toml)",
            path.display()
        ));
    };

    let scenario_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let generator_root = path
        .ancestors()
        .find(|p| p.file_name().and_then(|s| s.to_str()) == Some("leek-wars-generator"))
        .unwrap_or(&scenario_dir)
        .to_path_buf();
    Ok((scenario, scenario_dir, generator_root))
}

#[cfg(test)]
mod sweep_cartesian_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cartesian_extra_maps_counts() {
        let mut m = BTreeMap::new();
        m.insert("a".into(), vec![json!(1), json!(2)]);
        m.insert("b".into(), vec![json!(10), json!(20), json!(30)]);
        let g = cartesian_extra_maps(&m);
        assert_eq!(g.len(), 6);
    }

    #[test]
    fn expand_two_blocks_multiplies() {
        let c = SweepCartesian {
            name_prefix: Some("t".into()),
            blocks: vec![
                EntityCartesianBlock {
                    entity_id: 1,
                    extra_axes: [("life".into(), vec![json!(1), json!(2)])].into_iter().collect(),
                    weapons_axis: vec![],
                    chips_axis: vec![],
                    level_axis: vec![],
                    cell_axis: vec![],
                },
                EntityCartesianBlock {
                    entity_id: 2,
                    extra_axes: [("life".into(), vec![json!(9), json!(8), json!(7)])].into_iter().collect(),
                    weapons_axis: vec![],
                    chips_axis: vec![],
                    level_axis: vec![],
                    cell_axis: vec![],
                },
            ],
        };
        let rows = expand_sweep_cartesian(&c).unwrap();
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].len(), 2);
    }

    #[test]
    fn weapons_axis_multiplies_with_extra() {
        let c = SweepCartesian {
            name_prefix: None,
            blocks: vec![EntityCartesianBlock {
                entity_id: 12,
                extra_axes: [("tp".into(), vec![json!(10), json!(20)])].into_iter().collect(),
                weapons_axis: vec![vec![37], vec![37, 47]],
                chips_axis: vec![],
                level_axis: vec![],
                cell_axis: vec![],
            }],
        };
        let rows = expand_sweep_cartesian(&c).unwrap();
        assert_eq!(rows.len(), 4);
    }
}
