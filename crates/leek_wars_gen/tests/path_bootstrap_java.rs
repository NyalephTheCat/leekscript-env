//! Integration: pathfinding after Java bootstrap (cells + obstacles) must reach nearest enemy.

use leek_wars_gen::engine::{default_java_cwd, dump_java_fight_bootstrap, resolve_generator_jar};
use leek_wars_gen::fight::{
    astar_path_probe_script, compile_treeset_weight_probe_java, get_path_between, load_chips_json,
    load_summons_json, load_weapons_json, replay_treeset_weight_probe_polls, FightWorld,
};
use leek_wars_gen::scenario::Scenario;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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

#[test]
fn path_exists_patrick_to_nearest_enemy_after_java_bootstrap() {
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(_) => return,
    };
    let scenario_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../leek-wars-generator/test/scenario/scenario1.json");
    if !scenario_path.is_file() {
        return;
    }
    let ai_base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator");
    let raw = std::fs::read_to_string(&scenario_path).expect("read scenario");
    let sc: Scenario = serde_json::from_str(&raw).expect("parse");
    let chips = load_chips_json(&ai_base.join("data/chips.json")).expect("chips");
    let summons = load_summons_json(&ai_base.join("data/summons.json")).expect("summons");
    let weapons = load_weapons_json(&ai_base.join("data/weapons.json")).expect("weapons");
    let mut world = FightWorld::from_scenario(&sc, weapons, chips, summons);
    let cwd = default_java_cwd(&jar);
    let bootstrap =
        dump_java_fight_bootstrap(&jar, &cwd, &java_bin(), &scenario_path).expect("dump");

    for (&fid, &cell) in &bootstrap.entity_cells {
        if let Some(e) = world.entity_mut(fid) {
            e.cell = cell;
        }
    }
    world.obstacles = bootstrap.obstacles.clone();
    world.map_w = bootstrap.map_w;
    world.map_h = bootstrap.map_h;
    world.map_type = bootstrap.map_type;

    let patrick = world.entity(0).expect("patrick");
    let from = patrick.cell;
    let team = patrick.team;
    let mut best: Option<(i32, i32)> = None;
    for e in &world.entities {
        if e.dead || e.team == team {
            continue;
        }
        let d2 = leek_wars_gen::fight::map::distance2(world.map_w, from, e.cell);
        if best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, e.fid));
        }
    }
    let enemy_fid = best.map(|(_, id)| id).expect("enemy");
    let end = world.entity(enemy_fid).expect("target").cell;

    let path = get_path_between(&world, from, end, None);
    assert!(
        path.as_ref().is_some_and(|p| !p.is_empty()),
        "expected non-empty path from cell {from} to enemy fid {enemy_fid} cell {end}, got {path:?}"
    );
}

fn java_treeset_probe_poll_lines(script: &str, cp: &Path) -> Result<Vec<String>, String> {
    let mut child = Command::new("java")
        .arg("-cp")
        .arg(cp)
        .arg("TreeSetWeightProbe")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("java: {e}"))?;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(script.as_bytes())
        .map_err(|e| format!("stdin: {e}"))?;
    let out = child.wait_with_output().map_err(|e| format!("wait: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "java stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

/// For the Yolo snapshot, `OpenJDK` must replay our recorded `u`/`i`/`p` script the same as [`JavaWeightTree`].
#[test]
fn scenario1_yolo_astar_probe_script_matches_openjdk_treeset() {
    let Ok(cp) = compile_treeset_weight_probe_java() else {
        return;
    };
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(_) => return,
    };
    let scenario_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../leek-wars-generator/test/scenario/scenario1.json");
    if !scenario_path.is_file() {
        return;
    }
    let ai_base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator");
    let raw = std::fs::read_to_string(&scenario_path).expect("read scenario");
    let sc: Scenario = serde_json::from_str(&raw).expect("parse");
    let chips = load_chips_json(&ai_base.join("data/chips.json")).expect("chips");
    let summons = load_summons_json(&ai_base.join("data/summons.json")).expect("summons");
    let weapons = load_weapons_json(&ai_base.join("data/weapons.json")).expect("weapons");
    let mut world = FightWorld::from_scenario(&sc, weapons, chips, summons);
    let cwd = default_java_cwd(&jar);
    let bootstrap =
        dump_java_fight_bootstrap(&jar, &cwd, &java_bin(), &scenario_path).expect("dump");

    for (&fid, &cell) in &bootstrap.entity_cells {
        if let Some(e) = world.entity_mut(fid) {
            e.cell = cell;
        }
    }
    world.obstacles = bootstrap.obstacles.clone();
    world.map_w = bootstrap.map_w;
    world.map_h = bootstrap.map_h;
    world.map_type = bootstrap.map_type;

    if let Some(e) = world.entity_mut(0) {
        e.cell = 109;
    }
    if let Some(e) = world.entity_mut(3) {
        e.cell = 116;
    }

    let yolo_cell = world.entity(1).expect("yolo").cell;
    assert_eq!(yolo_cell, 492);
    let target = world.entity(3).expect("bob").cell;
    assert_eq!(target, 116);

    let (_path, script) = astar_path_probe_script(&world, yolo_cell, &[target], None);
    let r = replay_treeset_weight_probe_polls(&script);
    let j = java_treeset_probe_poll_lines(&script, &cp).expect("openjdk replay");
    assert_eq!(
        j, r,
        "OpenJDK TreeSet poll order must match JavaWeightTree for Rust-recorded A* script"
    );
}

/// Fight state immediately before Yolo's (`fid` 1) first move in `scenario1.json` (Java `generator.jar` reference).
/// Requires `DumpStateRng` obstacle JSON to include every unwalkable cell (not only `obstacleSize > 0`).
#[test]
fn scenario1_yolo_first_move_path_matches_java_astar() {
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(_) => return,
    };
    let scenario_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../leek-wars-generator/test/scenario/scenario1.json");
    if !scenario_path.is_file() {
        return;
    }
    let ai_base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator");
    let raw = std::fs::read_to_string(&scenario_path).expect("read scenario");
    let sc: Scenario = serde_json::from_str(&raw).expect("parse");
    let chips = load_chips_json(&ai_base.join("data/chips.json")).expect("chips");
    let summons = load_summons_json(&ai_base.join("data/summons.json")).expect("summons");
    let weapons = load_weapons_json(&ai_base.join("data/weapons.json")).expect("weapons");
    let mut world = FightWorld::from_scenario(&sc, weapons, chips, summons);
    let cwd = default_java_cwd(&jar);
    let bootstrap =
        dump_java_fight_bootstrap(&jar, &cwd, &java_bin(), &scenario_path).expect("dump");

    for (&fid, &cell) in &bootstrap.entity_cells {
        if let Some(e) = world.entity_mut(fid) {
            e.cell = cell;
        }
    }
    world.obstacles = bootstrap.obstacles.clone();
    world.map_w = bootstrap.map_w;
    world.map_h = bootstrap.map_h;
    world.map_type = bootstrap.map_type;

    // After Patrick (`fid` 0) and Bob (`fid` 3) moved; Yolo (`fid` 1) and Boss (`fid` 2) unchanged.
    if let Some(e) = world.entity_mut(0) {
        e.cell = 109;
    }
    if let Some(e) = world.entity_mut(3) {
        e.cell = 116;
    }

    let yolo_cell = world.entity(1).expect("yolo").cell;
    assert_eq!(yolo_cell, 492, "unexpected bootstrap cell for fid 1");
    let target = world.entity(3).expect("bob").cell;
    assert_eq!(target, 116);

    let path = get_path_between(&world, yolo_cell, target, None).expect("path");
    // Full `Map.getAStarPath` output from `com.leekwars.DumpYoloAStar` (not MP-truncated fight log).
    let java = vec![
        475, 458, 441, 459, 442, 425, 408, 391, 374, 357, 340, 322, 305, 288, 271, 254, 237, 220,
        203, 186, 169, 151, 134,
    ];
    assert_eq!(
        path, java,
        "A* path must match Java generator.jar for this fight snapshot"
    );
}
