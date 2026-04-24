use crate::engine::RunRequest;
use crate::error::GenError;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Snapshot of official generator `Fight.initFight` / `State.init`: RNG, play order, map, and entity cells after procedural setup.
#[derive(Debug, Clone)]
pub struct JavaFightBootstrap {
    pub rng_internal_n: i64,
    pub initial_fids: Vec<i32>,
    pub entity_cells: HashMap<i32, i32>,
    pub obstacles: BTreeMap<i32, i32>,
    /// Obstacle object as in official-generator fight outcome `fight.map.obstacles` (`Actions.addMap`), excluding
    /// `obstacleSize <= 0` cells that remain in [`Self::obstacles`] for pathfinding.
    pub outcome_obstacles: Option<Value>,
    pub map_w: i32,
    pub map_h: i32,
    pub map_type: i32,
}

/// Run `com.leekwars.DumpStateRng` (same `generator.jar` classpath as `com.leekwars.Main`).
pub fn dump_java_fight_bootstrap(
    jar: &Path,
    cwd: &Path,
    java_bin: &Path,
    scenario_file: &Path,
) -> Result<JavaFightBootstrap, GenError> {
    if !cwd.is_dir() {
        return Err(GenError::CwdMissing(cwd.to_path_buf()));
    }
    let scenario_arg = scenario_path_arg_for_java_cwd(cwd, scenario_file);
    let mut cmd = Command::new(java_bin);
    cmd.current_dir(cwd);
    cmd.arg("-cp").arg(jar);
    cmd.arg("com.leekwars.DumpStateRng");
    cmd.arg(&scenario_arg);
    cmd.stdin(Stdio::null());
    cmd.stderr(Stdio::piped());
    cmd.stdout(Stdio::piped());
    let out = cmd.output()?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(GenError::JavaFailed {
            status: out.status.code(),
            stderr,
        });
    }
    let text = String::from_utf8(out.stdout).map_err(|_| GenError::JavaInvalidUtf8)?;
    let mut lines = text.lines();
    let n_line = lines.next().ok_or_else(|| {
        GenError::Message("DumpStateRng: empty stdout (expected internal n)".into())
    })?;
    let rng_internal_n: i64 = n_line
        .trim()
        .parse()
        .map_err(|e| GenError::Message(format!("DumpStateRng: invalid n line: {e}")))?;
    let fids_line = lines
        .next()
        .ok_or_else(|| GenError::Message("DumpStateRng: missing initial-order line".into()))?;
    let mut initial_fids = Vec::new();
    for s in fids_line.split(',') {
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        let fid = s
            .parse::<i32>()
            .map_err(|e| GenError::Message(format!("DumpStateRng: invalid fid: {e}")))?;
        initial_fids.push(fid);
    }
    let json_line = lines
        .next()
        .ok_or_else(|| GenError::Message("DumpStateRng: missing map/cells JSON line".into()))?;
    let tail: Value = serde_json::from_str(json_line.trim())
        .map_err(|e| GenError::Message(format!("DumpStateRng: invalid JSON line: {e}")))?;

    let mut entity_cells = HashMap::new();
    let cells = tail
        .get("cells")
        .and_then(|c| c.as_object())
        .ok_or_else(|| GenError::Message("DumpStateRng: missing cells object".into()))?;
    for (k, v) in cells {
        let fid = k
            .parse::<i32>()
            .map_err(|e| GenError::Message(format!("DumpStateRng: cells key: {e}")))?;
        let cell = v
            .as_i64()
            .ok_or_else(|| GenError::Message(format!("DumpStateRng: cells[{k}] not int")))?
            as i32;
        entity_cells.insert(fid, cell);
    }

    let mut obstacles = BTreeMap::new();
    if let Some(obs) = tail.get("obstacles").and_then(|o| o.as_object()) {
        for (k, v) in obs {
            let cid = k
                .parse::<i32>()
                .map_err(|e| GenError::Message(format!("DumpStateRng: obstacle key: {e}")))?;
            let stored = bootstrap_obstacle_stored_value(v)?;
            obstacles.insert(cid, stored);
        }
    }

    let map_w =
        tail.get("width")
            .and_then(|x| x.as_i64())
            .ok_or_else(|| GenError::Message("DumpStateRng: missing width".into()))? as i32;
    let map_h = tail
        .get("height")
        .and_then(|x| x.as_i64())
        .ok_or_else(|| GenError::Message("DumpStateRng: missing height".into()))?
        as i32;
    let map_type = tail.get("mapType").and_then(|x| x.as_i64()).unwrap_or(0) as i32;

    let outcome_obstacles = tail.get("outcomeObstacles").cloned();

    Ok(JavaFightBootstrap {
        rng_internal_n,
        initial_fids,
        entity_cells,
        obstacles,
        outcome_obstacles,
        map_w,
        map_h,
        map_type,
    })
}

fn bootstrap_obstacle_stored_value(v: &Value) -> Result<i32, GenError> {
    if let Some(i) = v.as_i64() {
        return Ok(i as i32);
    }
    if let Some(arr) = v.as_array() {
        if let Some(s) = arr.get(1).and_then(|x| x.as_i64()) {
            return Ok(s as i32);
        }
        if let Some(s) = arr.get(0).and_then(|x| x.as_i64()) {
            return Ok(s as i32);
        }
    }
    Err(GenError::Message(format!(
        "DumpStateRng: unsupported obstacle value {v}"
    )))
}

/// Argument path for `DumpStateRng` / JVM `Main`: relative to `cwd` when possible.
///
/// `run_scenario_path` may build `ai_base.join("test/scenario/…")` as `leek-wars-generator/test/…`
/// while [`default_java_cwd`] is the absolute `…/leek-wars-generator` directory. A plain [`Path::strip_prefix`] then fails and we would pass `leek-wars-generator/test/…` into a process
/// whose `current_dir` is already `…/leek-wars-generator`, doubling the segment and breaking the sync.
fn scenario_path_arg_for_java_cwd(cwd: &Path, scenario_file: &Path) -> PathBuf {
    if let (Ok(cwd_abs), Ok(scen_abs)) = (cwd.canonicalize(), scenario_file.canonicalize()) {
        if let Ok(rel) = scen_abs.strip_prefix(&cwd_abs) {
            return rel.to_path_buf();
        }
    }
    if let Ok(rel) = scenario_file.strip_prefix(cwd) {
        return rel.to_path_buf();
    }
    if let (Some(cwd_leaf), Some(first)) = (cwd.file_name(), scenario_file.components().next()) {
        use std::path::Component;
        if let Component::Normal(a) = first {
            if a == cwd_leaf {
                return scenario_file.iter().skip(1).collect();
            }
        }
    }
    scenario_file.to_path_buf()
}

#[derive(Debug, Clone)]
pub struct JavaEngineConfig {
    pub jar: PathBuf,
    /// Working directory for relative paths in scenarios (AI files, includes). Official usage is `leek-wars-generator/`.
    pub cwd: PathBuf,
    /// Host JVM launcher binary; default `java` from `PATH`.
    pub java_bin: PathBuf,
}

pub struct JavaEngine {
    cfg: JavaEngineConfig,
}

impl JavaEngine {
    pub fn new(cfg: JavaEngineConfig) -> Self {
        Self { cfg }
    }

    pub fn run(&self, req: &RunRequest) -> Result<String, GenError> {
        let argv = req.java_argv();
        let mut cmd = Command::new(&self.cfg.java_bin);
        cmd.current_dir(&self.cfg.cwd);
        cmd.arg("-jar").arg(&self.cfg.jar);
        for a in &argv {
            cmd.arg(a);
        }
        cmd.stdin(Stdio::null());
        cmd.stderr(Stdio::piped());
        cmd.stdout(Stdio::piped());
        let out = cmd.output()?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            return Err(GenError::JavaFailed {
                status: out.status.code(),
                stderr,
            });
        }
        String::from_utf8(out.stdout).map_err(|_| GenError::JavaInvalidUtf8)
    }
}

/// Pick a sensible default cwd: `LEEK_GENERATOR_CWD`, else parent of `generator.jar` (usually `leek-wars-generator/`).
pub fn default_java_cwd(jar: &Path) -> PathBuf {
    if let Ok(c) = std::env::var("LEEK_GENERATOR_CWD") {
        return PathBuf::from(c);
    }
    jar.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}
