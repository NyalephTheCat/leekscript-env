use super::chips::load_chips_json;
use super::effects::apply_start_turn_effects;
use super::host::FightHost;
use super::rng::JavaCompatRng;
use super::sig_globals;
use super::summons::load_summons_json;
use super::weapons::load_weapons_json;
use super::trace::TraceEvent;
use super::world::{FightWorld, TraceSink};
use crate::engine::JavaFightBootstrap;
use crate::fight::java_bootstrap::compute_java_fight_bootstrap;
use crate::error::GenError;
use crate::scenario::Scenario;
use leekscript_run::{
    compile_source, CompileOptions, DebugSourceContext, HirStmt, InterpretSession,
};

include!(concat!(env!("OUT_DIR"), "/extra_natives.rs"));
use leekscript_signatures::SignatureFile;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::trace::TraceConfig;

type CachedAi = (leekscript_run::HirFile, u8, Option<bool>);

/// Options for a single fight run (trace, future extensions).
#[derive(Debug, Clone, Default)]
pub struct FightRunOptions {
    pub trace: Option<TraceConfig>,
    /// If set, `.leek` sources resolve under this directory; chips/weapons/summons still load from `ai_base` (`data/*.json`).
    pub ai_scripts_root: Option<PathBuf>,
}

/// Outcome of [`run_scenario_path_with_options`].
#[derive(Debug, Clone)]
pub struct FightRunOutput {
    pub outcome_json: String,
    pub trace_events: Option<Vec<TraceEvent>>,
}

fn short_source_hash(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let full = h.finalize();
    hex::encode(full)[..16].to_string()
}

fn debug_source_context_for_hir(
    generator_root: &Path,
    hir: &leekscript_run::HirFile,
    default_ai: &Path,
) -> Result<DebugSourceContext, std::io::Error> {
    let generator_root = generator_root
        .canonicalize()
        .unwrap_or_else(|_| generator_root.to_path_buf());
    let default_ai = default_ai.canonicalize().unwrap_or_else(|_| default_ai.to_path_buf());
    let mut texts = std::collections::HashMap::new();
    let mut paths: Vec<PathBuf> = Vec::new();
    for p in hir.stmt_sources.iter().chain(std::iter::once(&default_ai)) {
        if p.as_os_str().is_empty() {
            continue;
        }
        let c = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        if !paths.contains(&c) {
            paths.push(c);
        }
    }
    if !paths.contains(&default_ai) {
        paths.push(default_ai);
    }
    for p in paths {
        let s = std::fs::read_to_string(&p)?;
        texts.insert(p, s.into());
    }
    Ok(DebugSourceContext {
        generator_root,
        texts,
    })
}

struct EntityAiState {
    session: InterpretSession,
    turn_stmts: Vec<HirStmt>,
    turn_files: Vec<PathBuf>,
}

/// Resolve a scenario-relative AI script path, preferring a per-run overlay tree when the file exists there.
///
/// `ai_overlay` is typically a temp directory containing mutated copies of `.leek` files; any path not
/// present in the overlay falls back to [`ai_base`]. Chips/weapons/data always load from `ai_base`.
fn apply_java_bootstrap(world: &Rc<RefCell<FightWorld>>, bootstrap: &JavaFightBootstrap) {
    let mut w = world.borrow_mut();
    for (&fid, &cell) in &bootstrap.entity_cells {
        if let Some(e) = w.entity_mut(fid) {
            e.cell = cell;
            e.spawn_cell = cell;
        }
    }
    w.obstacles = bootstrap.obstacles.clone();
    w.outcome_obstacles_json = bootstrap.outcome_obstacles.clone();
    w.map_w = bootstrap.map_w;
    w.map_h = bootstrap.map_h;
    w.map_type = bootstrap.map_type;
    w.initial_fids = bootstrap.initial_fids.clone();
    w.turn_fids = bootstrap.initial_fids.clone();
    w.rng = JavaCompatRng::from_internal_n(bootstrap.rng_internal_n);
}

fn resolve_ai_source_path(ai_base: &Path, ai_overlay: Option<&Path>, ai_rel: &str) -> PathBuf {
    if let Some(overlay) = ai_overlay {
        let p = overlay.join(ai_rel);
        if p.is_file() {
            return p;
        }
    }
    ai_base.join(ai_rel)
}

/// Run a fight like [`run_scenario_path`], but read `.leek` sources from `ai_overlay` when present
/// (see [`resolve_ai_source_path`]). Uses the same official-generator-compatible bootstrap as [`run_scenario_path`].
pub fn run_scenario_path_with_ai_overlay(
    scenario_path: &Path,
    ai_base: &Path,
    ai_overlay: Option<&Path>,
) -> Result<String, GenError> {
    Ok(
        run_scenario_path_inner(scenario_path, ai_base, ai_overlay, FightRunOptions::default())?
            .outcome_json,
    )
}

/// Run a scenario JSON through the in-tree fight loop and return outcome JSON (same general shape as the official generator).
///
/// Replays official-generator `State.init` procedural map + start order in Rust (no JVM subprocess).
pub fn run_scenario_path(scenario_path: &Path, ai_base: &Path) -> Result<String, GenError> {
    Ok(
        run_scenario_path_inner(scenario_path, ai_base, None, FightRunOptions::default())?.outcome_json,
    )
}

/// Like [`run_scenario_path_with_ai_overlay`] with extra options (e.g. Rust-only fight trace).
pub fn run_scenario_path_with_options(
    scenario_path: &Path,
    ai_base: &Path,
    ai_overlay: Option<&Path>,
    options: FightRunOptions,
) -> Result<FightRunOutput, GenError> {
    run_scenario_path_inner(scenario_path, ai_base, ai_overlay, options)
}

fn run_scenario_path_inner(
    scenario_path: &Path,
    ai_base: &Path,
    ai_overlay: Option<&Path>,
    options: FightRunOptions,
) -> Result<FightRunOutput, GenError> {
    let scripts_base = options
        .ai_scripts_root
        .as_deref()
        .unwrap_or(ai_base);
    let scenario_path = if scenario_path.is_absolute() {
        scenario_path.to_path_buf()
    } else {
        ai_base.join(scenario_path)
    };
    let raw = std::fs::read_to_string(&scenario_path)?;
    let sc: Scenario = serde_json::from_str(&raw)?;

    let sig = SignatureFile::load_path(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("data/wars_functions.toml"),
    )
    .map_err(|e| GenError::Message(e.to_string()))?;
    let signature_globals = sig_globals::merge_signature_globals(sig.resolve_names());

    let chips = load_chips_json(&ai_base.join("data/chips.json"))?;
    let summons = load_summons_json(&ai_base.join("data/summons.json"))?;
    let weapons = load_weapons_json(&ai_base.join("data/weapons.json"))?;
    let world = Rc::new(RefCell::new(FightWorld::from_scenario(
        &sc, weapons, chips, summons,
    )));
    {
        let bootstrap = compute_java_fight_bootstrap(&world.borrow());
        apply_java_bootstrap(&world, &bootstrap);
    }

    if let Some(tc) = options.trace.clone() {
        if tc.enabled {
            world.borrow_mut().trace = Some(TraceSink {
                config: tc,
                events: Vec::new(),
            });
        }
    }

    if world.borrow().turn_fids.is_empty() {
        return Err(GenError::Message("no entities in scenario".into()));
    }

    let mut compile_cache: HashMap<String, CachedAi> = HashMap::new();
    let mut entity_ais: HashMap<i32, EntityAiState> = HashMap::new();

    struct Engine {
        pos: usize,
        turn: i32,
    }
    impl Engine {
        fn new() -> Self {
            Self { pos: 0, turn: 1 }
        }
        fn current_alive(&self, w: &FightWorld) -> Option<i32> {
            let n = w.turn_fids.len();
            if n == 0 {
                return None;
            }
            for step in 0..n {
                let i = (self.pos + step) % n;
                let fid = w.turn_fids[i];
                if let Some(e) = w.entity(fid) {
                    if !e.dead {
                        return Some(fid);
                    }
                }
            }
            None
        }
        fn advance(&mut self, w: &FightWorld) {
            let n = w.turn_fids.len();
            if n == 0 {
                return;
            }
            self.pos += 1;
            if self.pos >= n {
                self.turn += 1;
                self.pos = 0;
            }
        }
    }

    let max_turns = world.borrow().max_turns;
    let mut eng = Engine::new();
    // Official generator: `ActionStartFight`: [START_FIGHT]
    world.borrow_mut().log_action(json!([0]));

    while !world.borrow().is_finished() && eng.turn <= max_turns {
        let Some(fid) = eng.current_alive(&world.borrow()) else {
            break;
        };
        world.borrow_mut().active_fid = fid;
        world.borrow_mut().active_turn = eng.turn;
        // Official generator: ActionEntityTurn: [LEEK_TURN, fid]
        world.borrow_mut().log_action(json!([7, fid]));
        {
            let mut w = world.borrow_mut();
            w.start_turn(fid);
            apply_start_turn_effects(&mut w, fid);
            if let Some(e) = w.entity_mut(fid) {
                e.tp = e.total_tp;
                e.mp = e.total_mp;
            }
        }

        let ai_rel = world
            .borrow()
            .entity(fid)
            .map(|e| e.ai_path.clone())
            .unwrap_or_default();
        let (ai_version, ai_strict) = world
            .borrow()
            .entity(fid)
            .map(|e| (e.ai_version, e.ai_strict))
            .unwrap_or((0, false));
        if ai_rel.is_empty() {
            // Official generator: `State.endTurn` does not advance the order when the fight ends.
            if world.borrow().is_finished() {
                break;
            }
            eng.advance(&world.borrow());
            continue;
        }
        let ai_path = resolve_ai_source_path(scripts_base, ai_overlay, &ai_rel);
        let src = std::fs::read_to_string(&ai_path)
            .map_err(|e| {
                let mut msg = format!("read AI {}: {}", ai_path.display(), e);
                if scripts_base != ai_base && ai_rel.contains("leekwars-ai") {
                    msg.push_str(&format!(
                        " (data from {}, scripts from {}; fix --ai-root / LEEKWARS_AI_ROOT if wrong)",
                        ai_base.display(),
                        scripts_base.display()
                    ));
                }
                GenError::Message(msg)
            })?;
        let src_digest = short_source_hash(src.as_bytes());
        let cache_key = format!(
            "{}::v{}::strict{}::{}",
            ai_path.to_string_lossy(),
            ai_version,
            if ai_strict { 1 } else { 0 },
            src_digest
        );

        if !compile_cache.contains_key(&cache_key) {
            let opts = CompileOptions {
                source_path: Some(ai_path.clone()),
                snippet_origin: Some(ai_path.clone()),
                signature_globals: signature_globals.clone(),
                cli_language_version: (ai_version > 0 && ai_version <= 255)
                    .then_some(ai_version as u8),
                cli_strict: Some(ai_strict),
                ..Default::default()
            };
            let unit = compile_source(cache_key.clone(), src.as_str(), &opts).map_err(|diags| {
                GenError::Message(
                    diags
                        .iter()
                        .map(|d| format!("{}: {}", d.reference, d.message))
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            })?;
            compile_cache.insert(
                cache_key.clone(),
                (unit.hir, unit.language_version, unit.strict),
            );
        }
        let (hir, language_version, _strict) = compile_cache.get(&cache_key).unwrap();

        if !entity_ais.contains_key(&fid) {
            let host = FightHost::new(Rc::clone(&world));
            let ops_limit = world.borrow().max_operations_per_entity;
            let dbg = debug_source_context_for_hir(scripts_base, hir, &ai_path).ok();
            let (mut session, turn_stmts, turn_files) =
                InterpretSession::from_hir_leek_wars_ai_with_extra_natives(
                    hir,
                    *language_version,
                    Some(Box::new(host)),
                    EXTRA_FIGHT_NATIVES,
                    ops_limit,
                    None,
                    dbg,
                    ai_path.clone(),
                )
                .map_err(|e| GenError::Message(format!("{}: {}", e.reference, e.message)))?;
            sig_globals::seed_interpret_session(&mut session);
            entity_ais.insert(
                fid,
                EntityAiState {
                    session,
                    turn_stmts,
                    turn_files,
                },
            );
        }
        if let Some(state) = entity_ais.get_mut(&fid) {
            // Official generator: `EntityAI.runTurn` catches user/runtime errors and keeps generating the fight.
            if state
                .session
                .run_leek_wars_turn_stmts(&state.turn_stmts, &state.turn_files)
                .is_err()
            {
                // Official generator: ActionAIError: [AI_ERROR, fid]
                world.borrow_mut().log_action(json!([1002, fid]));
            }
            let ops = state.session.operations_used();
            world.borrow_mut().entity_ops_totals.insert(fid, ops);
        }
        // Official generator: `EntityAI.runTurn` ends with `mSays.clear()` + `mMessages.clear()`.
        {
            let mut w = world.borrow_mut();
            w.say_inbox.remove(&fid);
            w.inbox.remove(&fid);
        }

        // Official generator: `Entity.endTurn()` (propagation, cleanup) runs before ActionEndTurn.
        world.borrow_mut().end_turn(fid);

        // Official generator: ActionEndTurn: [END_TURN, fid, tp, mp]
        {
            let w = world.borrow();
            let (tp, mp) = w.entity(fid).map(|e| (e.tp, e.mp)).unwrap_or((0, 0));
            drop(w);
            world.borrow_mut().log_action(json!([8, fid, tp, mp]));
        }

        {
            let (life, tp, mp) = world
                .borrow()
                .entity(fid)
                .map(|e| (e.life, e.tp, e.mp))
                .unwrap_or((0, 0, 0));
            world.borrow_mut().trace_event(
                eng.turn,
                fid,
                "end_entity_turn",
                Some(json!({
                    "life": life,
                    "tp": tp,
                    "mp": mp,
                })),
            );
        }

        // Official generator: `State.endTurn` does not advance the order when the fight ends.
        if world.borrow().is_finished() {
            break;
        }
        let prev_turn = eng.turn;
        eng.advance(&world.borrow());
        // Official generator: `State.endTurn`: global turn may advance past `scenario.max_turns` once more before
        // the fight loop stops; `ActionNewTurn` is logged while `order.getTurn() <= State.MAX_TURNS`
        // (64), but `Team.applyCoolDown` runs on every `order.next()` after a global turn change.
        if eng.turn != prev_turn {
            if eng.turn <= FightWorld::JAVA_MAX_TURNS {
                world.borrow_mut().log_action(json!([6, eng.turn]));
            }
            world.borrow_mut().tick_all_team_chip_cooldowns();
        }
    }

    let trace_events = world.borrow_mut().trace.take().map(|s| s.events);
    let out_val = {
        let w = world.borrow();
        let winner = w.compute_winner();
        let duration = eng.turn;
        outcome_json(&w, winner, duration)
    };

    Ok(FightRunOutput {
        outcome_json: serde_json::to_string(&out_val)?,
        trace_events,
    })
}

fn outcome_json(world: &FightWorld, winner: i32, duration: i32) -> serde_json::Value {
    // Match official generator `Actions.addEntity` / `Actions.toJSON` shapes.
    let leeks: Vec<serde_json::Value> = world
        .initial_fids
        .iter()
        .copied()
        .filter_map(|fid| world.entity(fid))
        .map(|e| {
            let hat = if e.hat > 0 {
                json!(e.hat)
            } else {
                serde_json::Value::Null
            };
            // Official generator: `Entity.getType()` returns `0` for leeks; our scenario uses `1` for leeks.
            let ty = (e.entity_type - 1).max(0);
            let mut obj = serde_json::Map::new();
            obj.insert("id".into(), json!(e.fid));
            obj.insert("level".into(), json!(e.level));
            obj.insert("skin".into(), json!(e.skin));
            obj.insert("hat".into(), hat);
            obj.insert("metal".into(), json!(e.metal));
            obj.insert("face".into(), json!(e.face));
            obj.insert("life".into(), json!(e.life));
            obj.insert("strength".into(), json!(e.strength));
            obj.insert("wisdom".into(), json!(e.wisdom));
            obj.insert("agility".into(), json!(e.agility));
            obj.insert("resistance".into(), json!(e.resistance));
            obj.insert("frequency".into(), json!(e.frequency));
            obj.insert("science".into(), json!(e.science));
            obj.insert("magic".into(), json!(e.magic));
            obj.insert("tp".into(), json!(e.total_tp));
            obj.insert("mp".into(), json!(e.total_mp));
            obj.insert("team".into(), json!(e.team + 1));
            obj.insert("name".into(), json!(e.name));
            obj.insert(
                "cellPos".into(),
                if e.spawn_cell >= 0 {
                    json!(e.spawn_cell)
                } else {
                    serde_json::Value::Null
                },
            );
            obj.insert("farmer".into(), json!(e.farmer_id));
            obj.insert("type".into(), json!(ty));
            obj.insert("orientation".into(), json!(0));
            obj.insert("summon".into(), json!(e.is_summon));
            if e.is_summon {
                obj.insert("owner".into(), json!(e.summoner_fid.unwrap_or(0)));
                obj.insert("critical".into(), json!(false));
            }
            serde_json::Value::Object(obj)
        })
        .collect();

    let dead: serde_json::Value = {
        let mut m = serde_json::Map::new();
        for e in &world.entities {
            m.insert(e.leek_id.to_string(), json!(e.dead));
        }
        serde_json::Value::Object(m)
    };
    let map_obstacles = world
        .outcome_obstacles_json
        .as_ref()
        .map(|v| v.clone())
        .unwrap_or_else(|| {
            let mut m = serde_json::Map::new();
            for (&cid, &val) in &world.obstacles {
                m.insert(cid.to_string(), json!(val));
            }
            serde_json::Value::Object(m)
        });
    let mut ops_map = serde_json::Map::new();
    let mut op_fids: Vec<i32> = world.entity_ops_totals.keys().copied().collect();
    op_fids.sort_unstable();
    for fid in op_fids {
        ops_map.insert(fid.to_string(), json!(world.entity_ops_totals[&fid]));
    }
    let ops_json = serde_json::Value::Object(ops_map);
    json!({
        "fight": {
            "leeks": leeks,
            "map": {
                "width": world.map_w,
                // Official generator: `Actions.addMap` bug: `"height"` is set to `getWidth()`.
                "height": world.map_w,
                "obstacles": map_obstacles,
                "type": world.map_type
            },
            "actions": world.actions,
            "dead": dead,
            "ops": ops_json,
        },
        "logs": world.outcome_logs_json(),
        "winner": winner,
        "duration": duration,
        "analyze_time": 0,
        "compilation_time": 0,
        "execution_time": 0,
    })
}
