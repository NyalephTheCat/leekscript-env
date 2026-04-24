//! Timed **official `generator.jar` vs Rust** scenario runs and structured outcome comparison.
//!
//! Use this for parity work (`CompareMode::FullNormalized` aims at byte-identical JSON aside from
//! top-level `*_time` keys) and for rough wall-clock benchmarking (note: JVM startup is included
//! in official-generator timings unless you warm up heavily).

use crate::engine::{JavaEngine, JavaEngineConfig, RunRequest, RustEngine};
use crate::error::GenError;
use crate::parity;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Non-recursive list of `*.json` files under `dir` (absolute, or relative to `cwd`).
///
/// Paths are returned relative to `cwd` when `strip_prefix` succeeds (the usual layout:
/// `cwd` = `leek-wars-generator/`, `dir` = `test/scenario` → `test/scenario/foo.json`).
pub fn discover_scenario_json_files(cwd: &Path, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let base = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        cwd.join(dir)
    };
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&base)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let rel = match p.strip_prefix(cwd) {
            Ok(r) => r.to_path_buf(),
            Err(_) => p,
        };
        paths.push(rel);
    }
    paths.sort();
    Ok(paths)
}

fn collect_scenario_json_files_recursive(base: &Path, cwd: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(base)? {
        let entry = entry?;
        let p = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            collect_scenario_json_files_recursive(&p, cwd, out)?;
        } else if ft.is_file() && p.extension().and_then(|e| e.to_str()) == Some("json") {
            let rel = match p.strip_prefix(cwd) {
                Ok(r) => r.to_path_buf(),
                Err(_) => p,
            };
            out.push(rel);
        }
    }
    Ok(())
}

/// Like [`discover_scenario_json_files`], but walks subdirectories (e.g. `test/scenario/generated/`).
pub fn discover_scenario_json_files_recursive(cwd: &Path, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let base = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        cwd.join(dir)
    };
    let mut paths = Vec::new();
    if base.is_dir() {
        collect_scenario_json_files_recursive(&base, cwd, &mut paths)?;
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// `test/scenario/*.json` basenames that omit fields required by both the official generator `Scenario.fromFile` and Rust
/// [`crate::scenario::Scenario`] (e.g. `battleroyale.json` entities without `life`).
pub const INCOMPLETE_SCENARIO_BASELINES: &[&str] = &["battleroyale.json"];

/// Subset of action codes used while the Rust engine still omits many log lines (legacy parity test).
pub fn minimal_action_code_filter() -> BTreeSet<i64> {
    [0_i64, 5, 6, 7, 8, 10, 12, 13, 16, 101]
        .into_iter()
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompareMode {
    /// [`parity::normalize_outcome_json`] then full structural equality (`fight.actions`, `fight.ops`, …).
    #[default]
    FullNormalized,
    /// Only `winner` and `duration` top-level keys.
    WinnerDuration,
    /// Filtered action codes; **official generator** (needle) must be a **subsequence** of **Rust** (haystack; reference).
    ActionsMinimal,
    /// Exact `fight.actions` array equality after top-level normalization (timing fields removed).
    ActionsExact,
    /// Exact `fight.ops` array equality after top-level normalization (timing fields removed).
    OpsExact,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimingSummary {
    pub iterations: usize,
    pub min_ms: f64,
    pub median_ms: f64,
    pub mean_ms: f64,
    pub samples_ms: Vec<f64>,
}

impl TimingSummary {
    pub fn from_samples(mut samples: Vec<f64>) -> Self {
        let n = samples.len();
        if n == 0 {
            return Self {
                iterations: 0,
                min_ms: 0.0,
                median_ms: 0.0,
                mean_ms: 0.0,
                samples_ms: vec![],
            };
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let min_ms = samples[0];
        let median_ms = if n % 2 == 1 {
            samples[n / 2]
        } else {
            (samples[n / 2 - 1] + samples[n / 2]) / 2.0
        };
        let mean_ms = samples.iter().sum::<f64>() / n as f64;
        Self {
            iterations: n,
            min_ms,
            median_ms,
            mean_ms,
            samples_ms: samples,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompareResult {
    FullMatch,
    FullMismatch {
        note: String,
        /// Structural diff of normalized JSON (`generator` vs `rust`) when comparing full outcomes.
        #[serde(skip_serializing_if = "Option::is_none")]
        normalized_diff: Option<String>,
    },
    WinnerDurationMatch,
    WinnerDurationMismatch {
        java_winner: Value,
        rust_winner: Value,
        java_duration: Value,
        rust_duration: Value,
    },
    ActionsSubsequenceOk,
    ActionsSubsequenceFail {
        java_codes: Vec<i64>,
        rust_codes: Vec<i64>,
    },
    ActionsExactMatch,
    ActionsExactMismatch {
        java_len: usize,
        rust_len: usize,
        /// First index where actions differ (if any).
        first_diff_index: Option<usize>,
    },
    OpsExactMatch,
    OpsExactMismatch {
        java_len: usize,
        rust_len: usize,
        /// First index where ops differ (if any).
        first_diff_index: Option<usize>,
    },
    /// One or both engines did not return an outcome (runtime / compile / I/O). Compare stderr-style text here.
    EngineRunMismatch {
        java_error: Option<String>,
        rust_error: Option<String>,
    },
    /// Outcome stdout was not JSON (or could not be compared) for one or both sides.
    OutcomeNotJson {
        java: Option<String>,
        rust: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioHarnessReport {
    pub scenario: PathBuf,
    pub mode: String,
    pub java: Option<TimingSummary>,
    pub rust: TimingSummary,
    /// `median_java_ms / median_rust_ms` when both sides ran (>1 ⇒ official generator slower wall-clock).
    pub java_over_rust_median: Option<f64>,
    pub compare: CompareResult,
    pub last_java_json: Option<String>,
    pub last_rust_json: String,
    /// Set when the official generator run failed (stdout not produced). See [`CompareResult::EngineRunMismatch`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_error: Option<String>,
    /// Set when the Rust engine run failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_error: Option<String>,
}

impl ScenarioHarnessReport {
    /// `true` when the chosen compare mode detected a real mismatch (not “engine skipped”).
    pub fn comparison_failed(&self) -> bool {
        match &self.compare {
            CompareResult::FullMatch
            | CompareResult::WinnerDurationMatch
            | CompareResult::ActionsSubsequenceOk
            | CompareResult::ActionsExactMatch
            | CompareResult::OpsExactMatch => false,
            CompareResult::FullMismatch { note, .. } if note.contains("no A/B comparison") => false,
            _ => true,
        }
    }

    /// `true` when a configured engine exited with an error before producing an outcome.
    pub fn engine_run_failed(&self) -> bool {
        self.java_error.is_some() || self.rust_error.is_some()
    }

    /// Human-readable combined engine errors for logs and fuzz [`crate::error::GenError::Message`].
    pub fn engine_errors_display(&self) -> Option<String> {
        if self.java_error.is_none() && self.rust_error.is_none() {
            return None;
        }
        let mut s = String::new();
        if let Some(j) = &self.java_error {
            s.push_str("=== Java (official generator) ===\n");
            s.push_str(j);
            s.push('\n');
        }
        if let Some(r) = &self.rust_error {
            if !s.is_empty() {
                s.push('\n');
            }
            s.push_str("=== Rust ===\n");
            s.push_str(r);
        }
        Some(s)
    }
}

#[derive(Debug, Clone)]
pub struct HarnessRunConfig {
    pub java: JavaEngineConfig,
    pub mode: CompareMode,
    pub warmup: usize,
    pub iterations: usize,
    /// When false, skip the official generator run (timing + compare only on Rust if `skip_rust` is false).
    pub run_java: bool,
    pub run_rust: bool,
    /// When set, the official generator and Rust use this directory as JVM cwd / AI base instead of [`JavaEngineConfig::cwd`]
    /// (e.g. temp sandbox with mutated `.leek` files for fuzz + parity).
    pub runtime_cwd: Option<PathBuf>,
}

fn extract_action_codes(outcome: &Value) -> Vec<i64> {
    let mut v = Vec::new();
    let actions = outcome
        .get("fight")
        .and_then(|f| f.get("actions"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();
    for a in actions {
        let code = a
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|x| x.as_i64());
        if let Some(c) = code {
            v.push(c);
        }
    }
    v
}

fn is_subsequence(needle: &[i64], haystack: &[i64]) -> bool {
    let mut i = 0usize;
    for &h in haystack {
        if i < needle.len() && needle[i] == h {
            i += 1;
        }
        if i == needle.len() {
            return true;
        }
    }
    i == needle.len()
}

fn outcome_json_parse_errors(java: &str, rust: &str) -> Option<(Option<String>, Option<String>)> {
    let je = if java.trim().is_empty() {
        Some("empty stdout".into())
    } else {
        serde_json::from_str::<Value>(java).err().map(|e| e.to_string())
    };
    let re = if rust.trim().is_empty() {
        Some("empty stdout".into())
    } else {
        serde_json::from_str::<Value>(rust).err().map(|e| e.to_string())
    };
    if je.is_none() && re.is_none() {
        None
    } else {
        Some((je, re))
    }
}

/// Compare two raw outcome JSON strings according to `mode`.
///
/// Invalid JSON is reported as [`CompareResult::OutcomeNotJson`] instead of failing the harness.
pub fn compare_outcomes(mode: CompareMode, java: &str, rust: &str) -> CompareResult {
    match mode {
        CompareMode::FullNormalized => {
            if let Some((je, re)) = outcome_json_parse_errors(java, rust) {
                return CompareResult::OutcomeNotJson { java: je, rust: re };
            }
            match parity::outcomes_equal_ignore_timing(java, rust) {
                Ok(true) => CompareResult::FullMatch,
                Ok(false) => {
                    let normalized_diff = parity::diff_normalized_outcomes(java, rust).ok();
                    CompareResult::FullMismatch {
                        note: "normalized outcome JSON differs".into(),
                        normalized_diff,
                    }
                }
                Err(e) => CompareResult::OutcomeNotJson {
                    java: Some(e.to_string()),
                    rust: None,
                },
            }
        }
        CompareMode::WinnerDuration => {
            if let Some((je, re)) = outcome_json_parse_errors(java, rust) {
                return CompareResult::OutcomeNotJson { java: je, rust: re };
            }
            let j: Value = serde_json::from_str(java).expect("validated JSON");
            let r: Value = serde_json::from_str(rust).expect("validated JSON");
            if j.get("winner") == r.get("winner") && j.get("duration") == r.get("duration") {
                CompareResult::WinnerDurationMatch
            } else {
                CompareResult::WinnerDurationMismatch {
                    java_winner: j.get("winner").cloned().unwrap_or(Value::Null),
                    rust_winner: r.get("winner").cloned().unwrap_or(Value::Null),
                    java_duration: j.get("duration").cloned().unwrap_or(Value::Null),
                    rust_duration: r.get("duration").cloned().unwrap_or(Value::Null),
                }
            }
        }
        CompareMode::ActionsMinimal => {
            if let Some((je, re)) = outcome_json_parse_errors(java, rust) {
                return CompareResult::OutcomeNotJson { java: je, rust: re };
            }
            let j: Value = serde_json::from_str(java).expect("validated JSON");
            let r: Value = serde_json::from_str(rust).expect("validated JSON");
            let keep = minimal_action_code_filter();
            let j_codes: Vec<i64> = extract_action_codes(&j)
                .into_iter()
                .filter(|c| keep.contains(c))
                .collect();
            let r_codes: Vec<i64> = extract_action_codes(&r)
                .into_iter()
                .filter(|c| keep.contains(c))
                .collect();
            if is_subsequence(&j_codes, &r_codes) {
                CompareResult::ActionsSubsequenceOk
            } else {
                CompareResult::ActionsSubsequenceFail {
                    java_codes: j_codes,
                    rust_codes: r_codes,
                }
            }
        }
        CompareMode::ActionsExact => {
            if let Some((je, re)) = outcome_json_parse_errors(java, rust) {
                return CompareResult::OutcomeNotJson { java: je, rust: re };
            }
            let nj = match parity::normalize_outcome_json(java) {
                Ok(v) => v,
                Err(e) => {
                    return CompareResult::OutcomeNotJson {
                        java: Some(e.to_string()),
                        rust: None,
                    };
                }
            };
            let nr = match parity::normalize_outcome_json(rust) {
                Ok(v) => v,
                Err(e) => {
                    return CompareResult::OutcomeNotJson {
                        java: None,
                        rust: Some(e.to_string()),
                    };
                }
            };
            let ja = nj
                .get("fight")
                .and_then(|f| f.get("actions"))
                .and_then(|a| a.as_array())
                .cloned()
                .unwrap_or_default();
            let ra = nr
                .get("fight")
                .and_then(|f| f.get("actions"))
                .and_then(|a| a.as_array())
                .cloned()
                .unwrap_or_default();
            if ja == ra {
                CompareResult::ActionsExactMatch
            } else {
                let n = ja.len().min(ra.len());
                let mut first = None;
                for i in 0..n {
                    if ja[i] != ra[i] {
                        first = Some(i);
                        break;
                    }
                }
                CompareResult::ActionsExactMismatch {
                    java_len: ja.len(),
                    rust_len: ra.len(),
                    first_diff_index: first,
                }
            }
        }
        CompareMode::OpsExact => {
            if let Some((je, re)) = outcome_json_parse_errors(java, rust) {
                return CompareResult::OutcomeNotJson { java: je, rust: re };
            }
            let nj = match parity::normalize_outcome_json(java) {
                Ok(v) => v,
                Err(e) => {
                    return CompareResult::OutcomeNotJson {
                        java: Some(e.to_string()),
                        rust: None,
                    };
                }
            };
            let nr = match parity::normalize_outcome_json(rust) {
                Ok(v) => v,
                Err(e) => {
                    return CompareResult::OutcomeNotJson {
                        java: None,
                        rust: Some(e.to_string()),
                    };
                }
            };
            let jo = nj
                .get("fight")
                .and_then(|f| f.get("ops"))
                .and_then(|a| a.as_array())
                .cloned()
                .unwrap_or_default();
            let ro = nr
                .get("fight")
                .and_then(|f| f.get("ops"))
                .and_then(|a| a.as_array())
                .cloned()
                .unwrap_or_default();
            if jo == ro {
                CompareResult::OpsExactMatch
            } else {
                let n = jo.len().min(ro.len());
                let mut first = None;
                for i in 0..n {
                    if jo[i] != ro[i] {
                        first = Some(i);
                        break;
                    }
                }
                CompareResult::OpsExactMismatch {
                    java_len: jo.len(),
                    rust_len: ro.len(),
                    first_diff_index: first,
                }
            }
        }
    }
}

fn effective_harness_cwd(cfg: &HarnessRunConfig) -> PathBuf {
    cfg.runtime_cwd
        .clone()
        .unwrap_or_else(|| cfg.java.cwd.clone())
}

fn bench_java(
    cfg: &HarnessRunConfig,
    req: &RunRequest,
    warmup: usize,
    iterations: usize,
) -> Result<(String, TimingSummary), GenError> {
    let mut jc = cfg.java.clone();
    jc.cwd = effective_harness_cwd(cfg);
    let engine = JavaEngine::new(jc);
    for _ in 0..warmup {
        let _ = engine.run(req)?;
    }
    let mut samples = Vec::with_capacity(iterations.max(1));
    let mut last = String::new();
    for _ in 0..iterations.max(1) {
        let t0 = Instant::now();
        last = engine.run(req)?;
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    Ok((last, TimingSummary::from_samples(samples)))
}

fn bench_rust(
    req: &RunRequest,
    cfg: &HarnessRunConfig,
    warmup: usize,
    iterations: usize,
) -> Result<(String, TimingSummary), GenError> {
    let ai_base = effective_harness_cwd(cfg);
    let ai_base = ai_base.as_path();
    let engine = RustEngine;
    for _ in 0..warmup {
        let _ = engine.run_scenario_with_cwd(req, ai_base)?;
    }
    let mut samples = Vec::with_capacity(iterations.max(1));
    let mut last = String::new();
    for _ in 0..iterations.max(1) {
        let t0 = Instant::now();
        last = engine.run_scenario_with_cwd(req, ai_base)?;
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    Ok((last, TimingSummary::from_samples(samples)))
}

/// Run configured engines, collect timing samples, and compare last outcomes.
pub fn run_scenario_harness(
    req: &RunRequest,
    cfg: &HarnessRunConfig,
) -> Result<ScenarioHarnessReport, GenError> {
    if !cfg.run_java && !cfg.run_rust {
        return Err(GenError::Message(
            "harness: enable at least one of --java / --rust (default: both)".into(),
        ));
    }

    let mode_str = match cfg.mode {
        CompareMode::FullNormalized => "full_normalized",
        CompareMode::WinnerDuration => "winner_duration",
        CompareMode::ActionsMinimal => "actions_minimal",
        CompareMode::ActionsExact => "actions_exact",
        CompareMode::OpsExact => "ops_exact",
    };

    let (last_java, java_timings, java_error) = if cfg.run_java {
        match bench_java(cfg, req, cfg.warmup, cfg.iterations) {
            Ok((s, t)) => (Some(s), Some(t), None),
            Err(e) => (None, None, Some(e.to_string())),
        }
    } else {
        (None, None, None)
    };

    let (last_rust, rust_timings, rust_error) = if cfg.run_rust {
        match bench_rust(req, cfg, cfg.warmup, cfg.iterations) {
            Ok((s, t)) => (s, t, None),
            Err(e) => (
                String::new(),
                TimingSummary::from_samples(vec![]),
                Some(e.to_string()),
            ),
        }
    } else {
        (String::new(), TimingSummary::from_samples(vec![]), None)
    };

    let compare = match (cfg.run_java, cfg.run_rust) {
        (true, true) => {
            if java_error.is_some() || rust_error.is_some() {
                CompareResult::EngineRunMismatch {
                    java_error: java_error.clone(),
                    rust_error: rust_error.clone(),
                }
            } else {
                let j = last_java
                    .as_ref()
                    .expect("no java_error implies official generator produced stdout");
                compare_outcomes(cfg.mode, j, &last_rust)
            }
        }
        _ => CompareResult::FullMismatch {
            note: if !cfg.run_java {
                "Official generator skipped; no A/B comparison".into()
            } else {
                "Rust skipped; no A/B comparison".into()
            },
            normalized_diff: None,
        },
    };

    let java_over_rust_median = match (&java_timings, cfg.run_rust, rust_error.is_none()) {
        (Some(jt), true, true) if jt.median_ms > 0.0 && rust_timings.median_ms > 0.0 => {
            Some(jt.median_ms / rust_timings.median_ms)
        }
        _ => None,
    };

    Ok(ScenarioHarnessReport {
        scenario: req.file.clone(),
        mode: mode_str.to_string(),
        java: java_timings,
        rust: rust_timings,
        java_over_rust_median,
        compare,
        last_java_json: last_java,
        last_rust_json: last_rust,
        java_error,
        rust_error,
    })
}
