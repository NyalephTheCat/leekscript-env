//! Expand [`super::spec::ExperimentSpec`] into concrete [`RunTask`] list.

use super::const_patch::patch_leek_constants;
use super::spec::{ArmSpec, ExperimentSpec, SeedsSpec};
use crate::error::GenError;
use crate::scenario_io::load_value;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One concrete fight to execute.
#[derive(Debug, Clone)]
pub struct RunTask {
    pub run_id: usize,
    pub arm_name: String,
    pub seed: i32,
    /// Nested: AI file rel path → const name → value.
    pub tunables: HashMap<String, HashMap<String, serde_json::Value>>,
    /// Full scenario JSON (includes `random_seed` and overrides).
    pub scenario_value: Value,
    /// Files to write under overlay root: rel path (posix) → patched source.
    pub overlay_sources: HashMap<String, String>,
}

#[must_use]
pub fn expand_seeds(spec: &SeedsSpec) -> Vec<i32> {
    match spec {
        SeedsSpec::List { list } => list.clone(),
        SeedsSpec::Range { range } => {
            let step = range.step.max(1);
            let mut v = Vec::new();
            let mut x = range.start;
            let end = range.end;
            if step > 0 {
                while x < end {
                    v.push(x);
                    x = x.saturating_add(step);
                }
            }
            v
        }
    }
}

fn cartesian_for_file(
    cmap: &HashMap<String, Vec<serde_json::Value>>,
) -> Vec<HashMap<String, serde_json::Value>> {
    let keys: Vec<String> = cmap.keys().cloned().collect();
    if keys.is_empty() {
        return vec![HashMap::new()];
    }
    let mut acc = vec![HashMap::new()];
    for k in keys {
        let vals = cmap.get(&k).cloned().unwrap_or_default();
        let mut next = Vec::new();
        for m in acc {
            for v in &vals {
                let mut m2 = m.clone();
                m2.insert(k.clone(), v.clone());
                next.push(m2);
            }
        }
        acc = next;
    }
    acc
}

/// Expand tunable grid for one arm: list of (file → const map).
#[must_use]
pub fn expand_arm_tunables(
    arm: &ArmSpec,
) -> Vec<HashMap<String, HashMap<String, serde_json::Value>>> {
    if !arm.variants.is_empty() {
        return arm.variants.clone();
    }
    if arm.tunables.is_empty() {
        return vec![HashMap::new()];
    }
    let mut file_partials: Vec<(String, Vec<HashMap<String, serde_json::Value>>)> = Vec::new();
    for (path, cmap) in &arm.tunables {
        file_partials.push((path.clone(), cartesian_for_file(cmap)));
    }
    let mut out: Vec<HashMap<String, HashMap<String, serde_json::Value>>> = vec![HashMap::new()];
    for (path, partials) in file_partials {
        let mut next = Vec::new();
        for base in &out {
            for p in &partials {
                let mut b2 = base.clone();
                b2.insert(path.clone(), p.clone());
                next.push(b2);
            }
        }
        out = next;
    }
    out
}

fn apply_ai_overrides(scenario: &mut Value, arm: &ArmSpec) -> Result<(), GenError> {
    let entities = scenario
        .get_mut("entities")
        .and_then(|e| e.as_array_mut())
        .ok_or_else(|| GenError::Message("scenario.entities must be an array".into()))?;
    for ov in &arm.ai_overrides {
        let team = entities
            .get_mut(ov.team)
            .and_then(|t| t.as_array_mut())
            .ok_or_else(|| {
                GenError::Message(format!("ai_overrides: bad team index {}", ov.team))
            })?;
        let ent = team
            .get_mut(ov.entity)
            .and_then(|e| e.as_object_mut())
            .ok_or_else(|| {
                GenError::Message(format!(
                    "ai_overrides: bad entity {} {}",
                    ov.team, ov.entity
                ))
            })?;
        ent.insert("ai".to_string(), json!(ov.ai.clone()));
    }
    Ok(())
}

fn apply_loadout_preset(
    scenario: &mut Value,
    preset_path: &Path,
    seed: i32,
) -> Result<(), GenError> {
    let preset = load_value(preset_path)?;
    let weapons = preset
        .get("weapons")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let chips = preset
        .get("chips")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    // Optional `{ "melee": [id...], "support": [...] }` — pick one id per stratum for diversity.
    let chip_strata = preset.get("chip_strata").and_then(|v| v.as_object());
    if weapons.is_empty() && chips.is_empty() && chip_strata.is_none() {
        return Ok(());
    }
    let mut rng = StdRng::seed_from_u64(seed as u64);
    let entities = scenario
        .get_mut("entities")
        .and_then(|e| e.as_array_mut())
        .ok_or_else(|| GenError::Message("scenario.entities must be an array".into()))?;
    for team in entities.iter_mut() {
        let Some(team_arr) = team.as_array_mut() else {
            continue;
        };
        for ent in team_arr.iter_mut() {
            let Some(obj) = ent.as_object_mut() else {
                continue;
            };
            if !weapons.is_empty() && obj.contains_key("weapons") {
                let cap = weapons.len().clamp(1, 4);
                let n = rng.gen_range(1..=cap);
                let mut idx: Vec<usize> = (0..weapons.len()).collect();
                // partial shuffle
                for i in 0..n {
                    let j = rng.gen_range(i..weapons.len());
                    idx.swap(i, j);
                }
                let chosen: Vec<Value> = idx[..n]
                    .iter()
                    .filter_map(|&i| weapons.get(i).cloned())
                    .collect();
                obj.insert("weapons".into(), Value::Array(chosen));
            }
            if obj.contains_key("chips") {
                let chosen: Vec<Value> = if let Some(strata) = chip_strata {
                    let mut v = Vec::new();
                    let mut keys: Vec<&String> = strata.keys().collect();
                    keys.sort();
                    for k in keys {
                        if let Some(arr) = strata.get(k).and_then(|x| x.as_array()) {
                            if arr.is_empty() {
                                continue;
                            }
                            let j = rng.gen_range(0..arr.len());
                            v.push(arr[j].clone());
                        }
                    }
                    if v.is_empty() && !chips.is_empty() {
                        let max_chips = chips.len().clamp(1, 12);
                        let n = rng.gen_range(1..=max_chips);
                        let mut idx: Vec<usize> = (0..chips.len()).collect();
                        for i in 0..n.min(chips.len()) {
                            let j = rng.gen_range(i..chips.len());
                            idx.swap(i, j);
                        }
                        idx[..n.min(chips.len())]
                            .iter()
                            .filter_map(|&i| chips.get(i).cloned())
                            .collect()
                    } else {
                        v
                    }
                } else if !chips.is_empty() {
                    let max_chips = chips.len().clamp(1, 12);
                    let n = rng.gen_range(1..=max_chips);
                    let mut idx: Vec<usize> = (0..chips.len()).collect();
                    for i in 0..n.min(chips.len()) {
                        let j = rng.gen_range(i..chips.len());
                        idx.swap(i, j);
                    }
                    idx[..n.min(chips.len())]
                        .iter()
                        .filter_map(|&i| chips.get(i).cloned())
                        .collect()
                } else {
                    Vec::new()
                };
                if !chosen.is_empty() {
                    obj.insert("chips".into(), Value::Array(chosen));
                }
            }
        }
    }
    Ok(())
}

/// Build overlay file contents for this tunable combo.
pub fn build_overlay_sources(
    generator_root: &Path,
    tunables: &HashMap<String, HashMap<String, serde_json::Value>>,
) -> Result<HashMap<String, String>, GenError> {
    let mut out = HashMap::new();
    for (rel, cmap) in tunables {
        let path = generator_root.join(rel);
        let src = std::fs::read_to_string(&path)
            .map_err(|e| GenError::Message(format!("read {}: {e}", path.display())))?;
        let mut flat: HashMap<String, serde_json::Value> = HashMap::new();
        for (k, v) in cmap {
            flat.insert(k.clone(), v.clone());
        }
        let patched = patch_leek_constants(&src, &flat)?;
        out.insert(rel.replace('\\', "/"), patched);
    }
    Ok(out)
}

/// Expand full experiment to run tasks (deterministic order: arms × tunable combos × seeds).
pub fn plan_experiment(
    spec: &ExperimentSpec,
    generator_root: &Path,
) -> Result<Vec<RunTask>, GenError> {
    let scenario_path = if spec.scenario.is_absolute() {
        spec.scenario.clone()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(&spec.scenario)
    };
    let base_template = load_value(&scenario_path)?;
    let seeds = expand_seeds(&spec.seeds);
    if seeds.is_empty() {
        return Err(GenError::Message(
            "experiment needs at least one seed".into(),
        ));
    }

    let mut tasks = Vec::new();
    let mut run_id = 0usize;
    for arm in &spec.arms {
        let combos = expand_arm_tunables(arm);
        for combo in combos {
            let overlay_sources = if combo.is_empty() {
                HashMap::new()
            } else {
                build_overlay_sources(generator_root, &combo)?
            };
            for &seed in &seeds {
                let mut scenario_value = base_template.clone();
                if let Some(obj) = scenario_value.as_object_mut() {
                    obj.insert("random_seed".into(), json!(seed));
                }
                apply_ai_overrides(&mut scenario_value, arm)?;
                if let Some(ref preset) = spec.loadout_preset {
                    let p = if preset.is_absolute() {
                        preset.clone()
                    } else {
                        generator_root.join(preset)
                    };
                    apply_loadout_preset(&mut scenario_value, &p, seed)?;
                }
                tasks.push(RunTask {
                    run_id,
                    arm_name: arm.name.clone(),
                    seed,
                    tunables: combo.clone(),
                    scenario_value,
                    overlay_sources: overlay_sources.clone(),
                });
                run_id += 1;
            }
        }
    }
    Ok(tasks)
}

/// Reload base scenario from spec path (call after `plan_experiment` if you only need to refresh path).
pub fn load_base_scenario(spec: &ExperimentSpec) -> Result<Value, GenError> {
    let scenario_path = if spec.scenario.is_absolute() {
        spec.scenario.clone()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(&spec.scenario)
    };
    load_value(&scenario_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::spec::ArmSpec;
    use serde_json::json;

    #[test]
    fn expand_tunables_cartesian_two_dims() {
        let mut arm = ArmSpec {
            name: "a".into(),
            ..Default::default()
        };
        let mut inner = HashMap::new();
        inner.insert("X".into(), vec![json!(1), json!(2)]);
        inner.insert("Y".into(), vec![json!(10)]);
        arm.tunables.insert("ai/f.leek".into(), inner);
        let c = expand_arm_tunables(&arm);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn expand_variants_sparse() {
        let mut arm = ArmSpec {
            name: "b".into(),
            ..Default::default()
        };
        let mut v1 = HashMap::new();
        let mut f1 = HashMap::new();
        f1.insert("X".into(), json!(1));
        v1.insert("ai/f.leek".into(), f1);
        arm.variants.push(v1);
        let c = expand_arm_tunables(&arm);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0]["ai/f.leek"]["X"], json!(1));
    }

    #[test]
    fn chip_strata_picks_one_per_bucket() {
        let preset = json!({
            "chip_strata": {
                "a": [100, 101],
                "b": [200]
            }
        });
        let dir = std::env::temp_dir().join(format!(
            "lw_preset_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let p = dir.join("p.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&p, preset.to_string()).unwrap();
        let mut scenario = json!({
            "entities": [[{ "chips": [1, 2, 3] }]]
        });
        apply_loadout_preset(&mut scenario, &p, 42).unwrap();
        let chips = scenario["entities"][0][0]["chips"].as_array().unwrap();
        assert_eq!(chips.len(), 2);
        assert!(chips.contains(&json!(200)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
