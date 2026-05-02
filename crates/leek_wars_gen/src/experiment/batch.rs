//! Execute planned experiments (parallel workers, cache, optional Java spot-check).

use super::aggregate::ExperimentAggregate;
use super::cache::{cache_key_for_task, cache_outcome_path};
use super::metrics::RunMetrics;
use super::planner::RunTask;
use super::spec::ExperimentSpec;
use crate::engine::{resolve_generator_jar, JavaEngineConfig, RunRequest};
use crate::fight::{run_scenario_path_with_options, FightRunOptions, TraceConfig};
use crate::harness::{run_scenario_harness, CompareMode, HarnessRunConfig};
use crate::error::GenError;
use rand::Rng;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::Arc;

const TRACE_SCHEMA: &str = "leekgen_trace_v1";

#[derive(Debug, Serialize)]
pub struct RunRecord {
    pub run_id: usize,
    pub arm: String,
    pub seed: i32,
    pub tunables: serde_json::Value,
    pub winner: Option<i64>,
    pub duration: Option<i64>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub outcome_path: String,
    pub cache_hit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_verify: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Manifest {
    pub spec_digest: String,
    pub engine_version: String,
    pub generator_root: String,
    pub n_tasks: usize,
    pub jobs: usize,
    pub records: Vec<RunRecord>,
}

/// Result of [`execute_run_task`] for optimizers / tests.
#[derive(Debug, Clone, Serialize)]
pub struct ExecuteRunOutput {
    pub metrics: RunMetrics,
    pub outcome_json: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_events: Option<Vec<crate::fight::TraceEvent>>,
}

/// Run a single task (Rust engine). Does not write manifest; for library callers.
///
/// `ai_scripts_root`: optional parent of `leekwars-ai/` when scripts are not under `generator_root`.
pub fn execute_run_task(
    task: &RunTask,
    generator_root: &Path,
    trace: Option<TraceConfig>,
    ai_scripts_root: Option<&Path>,
) -> Result<ExecuteRunOutput, GenError> {
    let tmp_root = std::env::temp_dir().join(format!(
        "leek_exp_{}_{}",
        task.run_id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&tmp_root).map_err(GenError::from)?;
    let scenario_file = tmp_root.join("scenario.json");
    let s = serde_json::to_string_pretty(&task.scenario_value).map_err(GenError::ScenarioJson)?;
    std::fs::write(&scenario_file, s).map_err(GenError::from)?;
    let overlay_dir = tmp_root.join("overlay");
    let overlay_opt = if task.overlay_sources.is_empty() {
        None
    } else {
        for (rel, content) in &task.overlay_sources {
            let p = overlay_dir.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(GenError::from)?;
            }
            std::fs::write(&p, content).map_err(GenError::from)?;
        }
        Some(overlay_dir.as_path())
    };
    let opts = FightRunOptions {
        trace: trace.filter(|t| t.enabled),
        ai_scripts_root: ai_scripts_root.map(Path::to_path_buf),
    };
    let out = run_scenario_path_with_options(&scenario_file, generator_root, overlay_opt, opts);
    let _ = std::fs::remove_dir_all(&tmp_root);
    let out = out?;
    let metrics = RunMetrics::from_outcome_json(&out.outcome_json)?;
    Ok(ExecuteRunOutput {
        metrics,
        outcome_json: out.outcome_json,
        trace_events: out.trace_events,
    })
}

fn digest_spec(path: &Path) -> Result<String, GenError> {
    let raw = std::fs::read(path)?;
    let mut h = sha2::Sha256::new();
    use sha2::Digest;
    h.update(&raw);
    Ok(hex::encode(h.finalize()))
}

fn maybe_java_spot_check(
    task: &RunTask,
    scenario_path: &Path,
    generator_root: &Path,
    rust_json: &str,
    rate: f64,
) -> Option<String> {
    if rate <= 0.0 || rand::thread_rng().gen::<f64>() >= rate {
        return None;
    }
    let _ = rust_json;
    if !task.overlay_sources.is_empty() {
        return Some("skipped: java verify with AI overlay not supported".into());
    }
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(e) => return Some(format!("java verify: {e}")),
    };
    let java = JavaEngineConfig {
        jar,
        cwd: generator_root.to_path_buf(),
        java_bin: PathBuf::from("java"),
    };
    let req = RunRequest {
        file: scenario_path.to_path_buf(),
        ..Default::default()
    };
    let cfg = HarnessRunConfig {
        java: java.clone(),
        mode: CompareMode::WinnerDuration,
        warmup: 0,
        iterations: 1,
        run_java: true,
        run_rust: true,
        runtime_cwd: None,
    };
    match run_scenario_harness(&req, &cfg) {
        Ok(rep) => {
            use crate::harness::CompareResult;
            let ok = matches!(
                rep.compare,
                CompareResult::FullMatch | CompareResult::WinnerDurationMatch
            );
            if ok {
                Some("java_ok".into())
            } else {
                Some(format!("java_mismatch: {:?}", rep.compare))
            }
        }
        Err(e) => Some(format!("java_verify_error: {e}")),
    }
}

struct WorkerEnv {
    generator_root: PathBuf,
    output_dir: PathBuf,
    cache_dir: PathBuf,
    java_verify_rate: f64,
    trace_cfg: Option<TraceConfig>,
}

fn run_one_task(task: RunTask, env: &WorkerEnv) -> Result<RunRecord, GenError> {
    let run_dir = env.output_dir.join("runs").join(task.run_id.to_string());
    std::fs::create_dir_all(&run_dir).map_err(GenError::from)?;
    let scenario_path = run_dir.join("scenario.json");
    let outcome_path = run_dir.join("outcome.json");
    let s = serde_json::to_string_pretty(&task.scenario_value).map_err(GenError::ScenarioJson)?;
    std::fs::write(&scenario_path, s).map_err(GenError::from)?;

    let key = cache_key_for_task(&task)?;
    let cache_file = cache_outcome_path(&env.cache_dir, &key);
    std::fs::create_dir_all(&env.cache_dir).map_err(GenError::from)?;

    let (outcome_json, cache_hit) = if cache_file.is_file() {
        let body = std::fs::read_to_string(&cache_file).map_err(GenError::from)?;
        (body, true)
    } else {
        let overlay_dir = run_dir.join("overlay");
        let overlay_opt = if task.overlay_sources.is_empty() {
            None
        } else {
            for (rel, content) in &task.overlay_sources {
                let p = overlay_dir.join(rel);
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent).map_err(GenError::from)?;
                }
                std::fs::write(&p, content).map_err(GenError::from)?;
            }
            Some(overlay_dir.as_path())
        };
        let opts = FightRunOptions {
            trace: env.trace_cfg.clone().filter(|t| t.enabled),
            ai_scripts_root: None,
        };
        let out = run_scenario_path_with_options(
            &scenario_path,
            &env.generator_root,
            overlay_opt,
            opts,
        )?;
        if let Some(events) = &out.trace_events {
            let trace_path = run_dir.join("trace.jsonl");
            let mut w = String::new();
            w.push_str(&format!(
                "{{\"schema\":\"{TRACE_SCHEMA}\",\"run_id\":{}}}\n",
                task.run_id
            ));
            for e in events {
                w.push_str(
                    &serde_json::to_string(e).map_err(|e| GenError::Message(e.to_string()))?,
                );
                w.push('\n');
            }
            std::fs::write(&trace_path, w).map_err(GenError::from)?;
        }
        std::fs::write(&cache_file, &out.outcome_json).map_err(GenError::from)?;
        std::fs::write(&outcome_path, &out.outcome_json).map_err(GenError::from)?;
        (out.outcome_json, false)
    };

    if cache_hit {
        std::fs::write(&outcome_path, &outcome_json).map_err(GenError::from)?;
    }

    let metrics = match RunMetrics::from_outcome_json(&outcome_json) {
        Ok(m) => m,
        Err(e) => RunMetrics::with_error(e.to_string()),
    };
    let ok = metrics.error.is_none();
    let java_verify = maybe_java_spot_check(
        &task,
        &scenario_path,
        &env.generator_root,
        &outcome_json,
        env.java_verify_rate,
    );

    Ok(RunRecord {
        run_id: task.run_id,
        arm: task.arm_name.clone(),
        seed: task.seed,
        tunables: serde_json::to_value(&task.tunables).unwrap_or_default(),
        winner: metrics.winner,
        duration: metrics.duration,
        ok,
        error: metrics.error.clone(),
        outcome_path: outcome_path.display().to_string(),
        cache_hit,
        java_verify,
    })
}

/// Run full experiment: parallel workers, `runs.ndjson`, `manifest.json`, terminal summary.
pub fn run_experiment(
    spec: &ExperimentSpec,
    spec_path: &Path,
    generator_root: PathBuf,
    jobs: usize,
) -> Result<Manifest, GenError> {
    if spec.arms.is_empty() {
        return Err(GenError::Message(
            "experiment requires at least one [[arms]] entry".into(),
        ));
    }
    std::fs::create_dir_all(&spec.output_dir).map_err(GenError::from)?;
    let cache_dir = spec.output_dir.join("cache");
    let tasks = super::planner::plan_experiment(spec, &generator_root)?;
    let n = tasks.len();
    let trace_cfg = spec.trace.as_ref().map(|t| TraceConfig {
        enabled: true,
        max_events: t.max_events,
    });

    let env = Arc::new(WorkerEnv {
        generator_root,
        output_dir: spec.output_dir.clone(),
        cache_dir,
        java_verify_rate: spec.java_verify_rate.clamp(0.0, 1.0),
        trace_cfg,
    });

    let jobs = jobs.max(1);
    let (tx, rx) = channel::<Result<RunRecord, GenError>>();
    std::thread::scope(|s| {
        let chunk_size = (n + jobs - 1) / jobs;
        for chunk in tasks.chunks(chunk_size.max(1)) {
            let tx = tx.clone();
            let env = Arc::clone(&env);
            let chunk: Vec<RunTask> = chunk.to_vec();
            s.spawn(move || {
                for task in chunk {
                    if tx.send(run_one_task(task, env.as_ref())).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
    });

    let mut records = Vec::new();
    for r in rx {
        records.push(r?);
    }
    records.sort_by_key(|r| r.run_id);

    let mut agg = ExperimentAggregate::default();
    for r in &records {
        agg.record(
            &r.arm,
            r.ok,
            r.winner,
            r.duration,
        );
    }
    agg.print_table();

    let ndjson_path = spec.output_dir.join("runs.ndjson");
    let mut nd = String::new();
    for r in &records {
        nd.push_str(
            &serde_json::to_string(r).map_err(|e| GenError::Message(e.to_string()))?,
        );
        nd.push('\n');
    }
    std::fs::write(&ndjson_path, nd).map_err(GenError::from)?;

    let spec_digest = digest_spec(spec_path)?;
    let manifest = Manifest {
        spec_digest,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        generator_root: env.generator_root.display().to_string(),
        n_tasks: n,
        jobs,
        records,
    };
    let manifest_path = spec.output_dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|e| GenError::Message(e.to_string()))?,
    )
    .map_err(GenError::from)?;

    Ok(manifest)
}
