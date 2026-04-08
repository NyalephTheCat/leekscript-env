use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use serde::{Deserialize, Serialize};

use crate::{Generator, Outcome, Scenario};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    pub mode: BatchMode,
    /// If set, each generated run gets a deterministic seed:
    /// `scenario.random_seed = base + run_index`.
    #[serde(default)]
    pub seed_schedule_base: Option<i32>,

    /// If set, run fights in parallel with at most this many worker threads.
    /// `None`/`Some(0)` means sequential.
    #[serde(default)]
    pub parallel_workers: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BatchMode {
    /// Run a list of scenario files as-is.
    Scenarios { scenarios: Vec<PathBuf> },
    /// Run the same base scenario with per-variant overrides (useful for optimizations / sweeps).
    Sweep {
        base_scenario: PathBuf,
        variants: Vec<SweepVariant>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityOverride {
    pub entity_id: i32,
    /// Key/value pairs merged into the entity `extra` map (e.g. `life`, `tp`, `mp`, `strength`).
    #[serde(default)]
    pub extra: BTreeMap<String, serde_json::Value>,
    /// Optional AI path override (relative to scenario dir or generator root).
    #[serde(default)]
    pub ai: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchResult {
    pub outcomes: Vec<Outcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<BatchSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchSummary {
    pub total: usize,
    pub winners: BTreeMap<i32, usize>,
    pub outcome_hashes_sha1: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub struct BatchRunner {
    pub generator: Generator,
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
                let jv = crate::toml_bridge::toml_to_json(&tv);
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
        let workers = job.parallel_workers.unwrap_or(0);
        let outcomes = if workers <= 1
            || plans.len() <= 1
            || self.generator.register_manager.is_some()
        {
            run_plans_sequential(&self.generator, &plans)?
        } else {
            run_plans_parallel(&self.generator, &plans, workers)?
        };
        let summary = summarize(&outcomes);
        Ok(BatchResult {
            outcomes,
            summary: Some(summary),
        })
    }

    pub fn run_to_jsonl(&self, job: &BatchJob, out: impl AsRef<Path>) -> miette::Result<()> {
        let out = out.as_ref();
        let f = std::fs::File::create(out)
            .map_err(|e| miette::miette!("failed to create `{}`: {e}", out.display()))?;
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
        for p in plans {
            let o = self
                .generator
                .run_scenario(&p.scenario, &p.scenario_dir, &p.generator_root)?;
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
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct RunPlan {
    scenario: Scenario,
    scenario_dir: PathBuf,
    generator_root: PathBuf,
}

fn expand_job(job: &BatchJob) -> miette::Result<Vec<RunPlan>> {
    let mut out: Vec<RunPlan> = Vec::new();
    match &job.mode {
        BatchMode::Scenarios { scenarios } => {
            for p in scenarios {
                let (scenario, scenario_dir, generator_root) = load_scenario_from_file(p)?;
                out.push(RunPlan {
                    scenario,
                    scenario_dir,
                    generator_root,
                });
            }
        }
        BatchMode::Sweep {
            base_scenario,
            variants,
        } => {
            let (base, scenario_dir, generator_root) = load_scenario_from_file(base_scenario)?;
            for var in variants {
                let mut scenario = base.clone();
                apply_overrides(&mut scenario, &var.overrides);
                out.push(RunPlan {
                    scenario,
                    scenario_dir: scenario_dir.clone(),
                    generator_root: generator_root.clone(),
                });
            }
        }
        BatchMode::VersusManyEnemies {
            base_scenario,
            hero_overrides,
            enemies,
        } => {
            let (base, scenario_dir, generator_root) = load_scenario_from_file(base_scenario)?;
            for enemy in enemies {
                let mut scenario = base.clone();
                apply_overrides(&mut scenario, hero_overrides);
                apply_overrides(&mut scenario, &enemy.overrides);
                out.push(RunPlan {
                    scenario,
                    scenario_dir: scenario_dir.clone(),
                    generator_root: generator_root.clone(),
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
                                EntityOverride {
                                    entity_id: a_slot,
                                    extra: BTreeMap::new(),
                                    ai: Some(competitors[i].ai.to_string_lossy().to_string()),
                                },
                                EntityOverride {
                                    entity_id: b_slot,
                                    extra: BTreeMap::new(),
                                    ai: Some(competitors[j].ai.to_string_lossy().to_string()),
                                },
                            ],
                        );
                        let _ = r;
                        out.push(RunPlan {
                            scenario,
                            scenario_dir: scenario_dir.clone(),
                            generator_root: generator_root.clone(),
                        });

                        if *swap_sides {
                            let mut scenario = base.clone();
                            apply_overrides(
                                &mut scenario,
                                &[
                                    EntityOverride {
                                        entity_id: a_slot,
                                        extra: BTreeMap::new(),
                                        ai: Some(competitors[j].ai.to_string_lossy().to_string()),
                                    },
                                    EntityOverride {
                                        entity_id: b_slot,
                                        extra: BTreeMap::new(),
                                        ai: Some(competitors[i].ai.to_string_lossy().to_string()),
                                    },
                                ],
                            );
                            out.push(RunPlan {
                                scenario,
                                scenario_dir: scenario_dir.clone(),
                                generator_root: generator_root.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    // Deterministic seed schedule for all plans.
    if let Some(base) = job.seed_schedule_base {
        for (i, p) in out.iter_mut().enumerate() {
            let seed = base.saturating_add(i as i32);
            p.scenario.random_seed = Some(seed);
        }
    }
    Ok(out)
}

fn run_plans_sequential(generator: &Generator, plans: &[RunPlan]) -> miette::Result<Vec<Outcome>> {
    let mut out = Vec::with_capacity(plans.len());
    for p in plans {
        out.push(
            generator.run_scenario(&p.scenario, &p.scenario_dir, &p.generator_root)?,
        );
    }
    Ok(out)
}

fn run_plans_parallel(
    generator: &Generator,
    plans: &[RunPlan],
    workers: usize,
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
    for _ in 0..plans.len() {
        let (i, r) = res_rx.recv().map_err(|e| miette::miette!("{e}"))?;
        tmp[i] = Some(r?);
    }

    for h in handles {
        let _ = h.join();
    }

    Ok(tmp.into_iter().map(|o| o.expect("filled")).collect())
}

fn summarize(outcomes: &[Outcome]) -> BatchSummary {
    let mut winners: BTreeMap<i32, usize> = BTreeMap::new();
    let mut hashes: BTreeSet<String> = BTreeSet::new();
    for o in outcomes {
        *winners.entry(o.winner).or_insert(0) += 1;
        if let Some(h) = o
            .logs
            .get("outcome_hash_sha1")
            .and_then(|v| v.as_str())
        {
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
            let jv = crate::toml_bridge::toml_to_json(&tv);
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

