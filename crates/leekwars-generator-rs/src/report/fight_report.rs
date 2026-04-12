//! Human-readable rendering of generator outcome JSON (same shape as the Java generator).
//! Action opcode names follow `com.leekwars.generator.action.Action` and the Leek Wars web client.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::report::enums::EffectType;

/// Resolved names from `leek-wars-generator/data/weapons.json` (and `chips.json` when present).
#[derive(Debug, Default, Clone)]
pub struct GameNames {
    /// Weapon template id → English id name (e.g. `pistol`).
    weapon_template: HashMap<i64, String>,
    /// Weapon item id → name.
    weapon_item: HashMap<i64, String>,
    chip_template: HashMap<i64, String>,
    chip_item: HashMap<i64, String>,
}

impl GameNames {
    /// Load from a `data` directory that contains `weapons.json`. `chips.json` is optional.
    pub fn load_from_data_dir(data_dir: &Path) -> Self {
        let mut g = GameNames::default();
        let weapons_path = data_dir.join("weapons.json");
        if weapons_path.is_file() {
            if let Ok(text) = std::fs::read_to_string(&weapons_path) {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    if let Some(obj) = v.as_object() {
                        for (_, wv) in obj {
                            let Some(wo) = wv.as_object() else { continue };
                            let name = wo
                                .get("name")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?")
                                .to_string();
                            if let Some(t) = wo.get("template").and_then(|x| x.as_i64()) {
                                g.weapon_template.insert(t, name.clone());
                            }
                            if let Some(item) = wo.get("item").and_then(|x| x.as_i64()) {
                                g.weapon_item.insert(item, name);
                            }
                        }
                    }
                }
            }
        }
        let chips_path = data_dir.join("chips.json");
        if chips_path.is_file() {
            if let Ok(text) = std::fs::read_to_string(&chips_path) {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    if let Some(obj) = v.as_object() {
                        for (_, cv) in obj {
                            let Some(co) = cv.as_object() else { continue };
                            let name = co
                                .get("name")
                                .and_then(|x| x.as_str())
                                .unwrap_or("?")
                                .to_string();
                            if let Some(t) = co.get("template").and_then(|x| x.as_i64()) {
                                g.chip_template.insert(t, name.clone());
                            }
                            if let Some(item) = co.get("item").and_then(|x| x.as_i64()) {
                                g.chip_item.insert(item, name);
                            }
                        }
                    }
                }
            }
        }
        g
    }

    fn weapon_by_template(&self, template: i64) -> Option<&str> {
        self.weapon_template.get(&template).map(|s| s.as_str())
    }

    fn weapon_by_item(&self, item: i64) -> Option<&str> {
        self.weapon_item.get(&item).map(|s| s.as_str())
    }

    fn chip_by_template(&self, template: i64) -> Option<&str> {
        self.chip_template.get(&template).map(|s| s.as_str())
    }

    fn chip_by_item(&self, item: i64) -> Option<&str> {
        self.chip_item.get(&item).map(|s| s.as_str())
    }

    fn format_weapon_template(&self, template: i64) -> String {
        match self.weapon_by_template(template) {
            Some(n) => format!("{n} (weapon template {template})"),
            None => format!("weapon template {template}"),
        }
    }

    fn format_weapon_item(&self, item: i64) -> String {
        match self.weapon_by_item(item) {
            Some(n) => format!("{n} (item {item})"),
            None => format!("weapon item {item}"),
        }
    }

    fn format_chip_template(&self, template: i64) -> String {
        match self.chip_by_template(template) {
            Some(n) => format!("{n} (chip template {template})"),
            None => format!("chip template {template}"),
        }
    }

    fn format_chip_item(&self, item: i64) -> String {
        match self.chip_by_item(item) {
            Some(n) => format!("{n} (item {item})"),
            None => format!("chip item {item}"),
        }
    }

    /// Chip id from scenario `chips` array (usually **item** id); falls back to template lookup.
    fn format_chip_slot_id(&self, id: i64) -> String {
        if let Some(n) = self.chip_by_item(id) {
            return format!("{n} (item {id})");
        }
        self.format_chip_template(id)
    }
}

fn resolve_data_dir_from_ancestor(anc: &Path) -> Option<PathBuf> {
    let nested = anc.join("leek-wars-generator/data/weapons.json");
    if nested.is_file() {
        return Some(anc.join("leek-wars-generator/data"));
    }
    let local = anc.join("data/weapons.json");
    if local.is_file() {
        return Some(anc.join("data"));
    }
    None
}

/// Walk ancestors of `start` and [`std::env::current_dir`] for game JSON (`weapons.json`).
///
/// Accepts either `…/repo/leek-wars-generator/data/` or `…/repo/` (nested path) and running
/// with cwd inside `leek-wars-generator` (`data/weapons.json` next to `generator.jar`).
pub fn find_game_data_dir(start: &Path) -> Option<PathBuf> {
    for anc in start.ancestors() {
        if let Some(d) = resolve_data_dir_from_ancestor(anc) {
            return Some(d);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for anc in cwd.ancestors() {
            if let Some(d) = resolve_data_dir_from_ancestor(anc) {
                return Some(d);
            }
        }
    }
    None
}

/// Format full outcome JSON (`fight`, `logs`, `winner`, timings, …) for terminal reading.
pub fn format_outcome_human(outcome: &Value) -> String {
    let data_dir = std::env::current_dir()
        .ok()
        .as_ref()
        .and_then(|cwd| find_game_data_dir(cwd.as_path()));
    let game = data_dir
        .as_ref()
        .map(|p| GameNames::load_from_data_dir(p))
        .unwrap_or_default();
    format_outcome_human_with_game(outcome, &game)
}

/// Like [`format_outcome_human`], but also searches for `data/` starting from `scenario_path`'s ancestors.
pub fn format_outcome_human_for_path(outcome: &Value, scenario_path: &Path) -> String {
    let data_dir = find_game_data_dir(scenario_path).or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| find_game_data_dir(&cwd))
    });
    let game = data_dir
        .as_ref()
        .map(|p| GameNames::load_from_data_dir(p))
        .unwrap_or_default();
    format_outcome_human_with_game(outcome, &game)
}

/// Same as [`format_outcome_human`], but uses pre-loaded [`GameNames`] (e.g. tests or custom data path).
pub fn format_outcome_human_with_game(outcome: &Value, game: &GameNames) -> String {
    let mut out = String::new();
    out.push_str("═══════════════════════════════════════════════════════════════\n");
    out.push_str(" Fight report\n");
    out.push_str("═══════════════════════════════════════════════════════════════\n\n");

    append_meta(&mut out, outcome);

    let fight = outcome.get("fight").cloned().unwrap_or(Value::Null);
    let names = entity_names(&fight);

    append_participants(&mut out, &fight, &names, game);
    out.push('\n');

    append_actions_timeline(&mut out, &fight, &names, game);
    out.push('\n');

    append_logs_summary(&mut out, outcome.get("logs"));

    out
}

fn append_meta(buf: &mut String, outcome: &Value) {
    buf.push_str("Outcome\n");
    match outcome.get("winner").and_then(|v| v.as_i64()) {
        Some(0) => {
            buf.push_str("  Winner: none (draw, or max turns reached with both teams alive)\n")
        }
        Some(w) => buf.push_str(&format!("  Winner: team {w}\n")),
        None => buf.push_str("  Winner: (unknown)\n"),
    }
    if let Some(d) = outcome.get("duration").and_then(|v| v.as_i64()) {
        buf.push_str(&format!("  Fight lasted: {d} turn(s)\n"));
    }
    let a = outcome
        .get("analyze_time")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let c = outcome
        .get("compilation_time")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let e = outcome
        .get("execution_time")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    buf.push_str(&format!(
        "  Timings: analyze {a} ms · compile {c} ms · execute {e} ms\n"
    ));
}

fn entity_names(fight: &Value) -> HashMap<i64, String> {
    let mut m = HashMap::new();
    let Some(leeks) = fight.get("leeks").and_then(|v| v.as_array()) else {
        return m;
    };
    for leek in leeks {
        let Some(id) = leek.get("id").and_then(|v| v.as_i64()) else {
            continue;
        };
        let name = leek
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        m.insert(id, name);
    }
    m
}

fn leek_label(names: &HashMap<i64, String>, id: i64) -> String {
    match names.get(&id) {
        Some(n) => format!("#{id} {n}"),
        None => format!("#{id}"),
    }
}

fn leek_at_cell(entity_cell: &HashMap<i64, i64>, dead: &HashSet<i64>, cell: i64) -> Option<i64> {
    entity_cell
        .iter()
        .find(|(e, c)| **c == cell && !dead.contains(e))
        .map(|(e, _)| *e)
}

fn append_participants(
    buf: &mut String,
    fight: &Value,
    names: &HashMap<i64, String>,
    game: &GameNames,
) {
    buf.push_str("Participants (starting positions)\n");
    let Some(leeks) = fight.get("leeks").and_then(|v| v.as_array()) else {
        buf.push_str("  (no leeks in fight JSON)\n");
        return;
    };
    for leek in leeks {
        let id = leek.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);
        let team = leek.get("team").and_then(|v| v.as_i64()).unwrap_or(0);
        let cell = leek
            .get("cellPos")
            .or_else(|| leek.get("cell"))
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
        let label = leek_label(names, id);
        let weapons = leek
            .get("weapons")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|w| w.as_i64())
                    .map(|item_id| game.format_weapon_item(item_id))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|s| !s.is_empty())
            .map(|s| format!(" | loadout weapons: {s}"))
            .unwrap_or_default();
        let chips = leek
            .get("chips")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.as_i64())
                    .map(|cid| game.format_chip_slot_id(cid))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|s| !s.is_empty())
            .map(|s| format!(" | chips: {s}"))
            .unwrap_or_default();
        buf.push_str(&format!(
            "  {label} — team {team}, starting cell {cell}{weapons}{chips}\n"
        ));
    }
}

struct TimelineState<'a> {
    names: &'a HashMap<i64, String>,
    game: &'a GameNames,
    active_entity: Option<i64>,
    entity_cell: HashMap<i64, i64>,
    dead: HashSet<i64>,
}

impl<'a> TimelineState<'a> {
    fn new(fight: &Value, names: &'a HashMap<i64, String>, game: &'a GameNames) -> Self {
        let mut entity_cell = HashMap::new();
        if let Some(leeks) = fight.get("leeks").and_then(|v| v.as_array()) {
            for leek in leeks {
                if let (Some(id), Some(cell)) = (
                    leek.get("id").and_then(|v| v.as_i64()),
                    leek.get("cellPos")
                        .or_else(|| leek.get("cell"))
                        .and_then(|v| v.as_i64()),
                ) {
                    entity_cell.insert(id, cell);
                }
            }
        }
        Self {
            names,
            game,
            active_entity: None,
            entity_cell,
            dead: HashSet::new(),
        }
    }

    fn apply_action(&mut self, ty: i64, a: &[Value]) {
        match ty {
            7 => {
                if let Some(id) = a.get(1).and_then(|v| v.as_i64()) {
                    self.active_entity = Some(id);
                }
            }
            8 => {
                self.active_entity = None;
            }
            10 => {
                if let (Some(leek), Some(end)) = (
                    a.get(1).and_then(|v| v.as_i64()),
                    a.get(2).and_then(|v| v.as_i64()),
                ) {
                    self.entity_cell.insert(leek, end);
                }
            }
            5 => {
                if let Some(id) = a.get(1).and_then(|v| v.as_i64()) {
                    self.dead.insert(id);
                }
            }
            9 => {
                if let (Some(target), Some(cell)) = (
                    a.get(2).and_then(|v| v.as_i64()),
                    a.get(3).and_then(|v| v.as_i64()),
                ) {
                    self.entity_cell.insert(target, cell);
                }
            }
            _ => {}
        }
    }
}

fn append_actions_timeline(
    buf: &mut String,
    fight: &Value,
    names: &HashMap<i64, String>,
    game: &GameNames,
) {
    buf.push_str("Timeline\n");
    let Some(actions) = fight.get("actions").and_then(|v| v.as_array()) else {
        buf.push_str("  (no actions)\n");
        return;
    };

    let mut state = TimelineState::new(fight, names, game);

    for (idx, act) in actions.iter().enumerate() {
        let Some(arr) = act.as_array() else {
            buf.push_str(&format!("  [{idx}] (non-array) {act}\n"));
            continue;
        };
        let ty = arr.first().and_then(|v| v.as_i64()).unwrap_or(-1);
        let line = format_action_line(&state, arr);
        if ty == 6
            && arr.len() >= 2
            && let Some(n) = arr.get(1).and_then(|v| v.as_i64())
        {
            buf.push_str(&format!(
                "\n── Turn {n} ─────────────────────────────────────────────\n"
            ));
            buf.push_str(&format!("  [{idx}] {line}\n"));
        } else {
            buf.push_str(&format!("  [{idx}] {line}\n"));
        }
        state.apply_action(ty, arr);
    }
}

fn hit_word(ok: i64) -> &'static str {
    match ok {
        0 => "miss / failure",
        1 => "hit / success",
        _ => "unknown result code",
    }
}

fn effect_type_name(id: i64) -> String {
    EffectType::from_i64(id)
        .map(|e| e.name().to_string())
        .unwrap_or_else(|| format!("EFFECT_TYPE_{id}"))
}

fn format_action_line(state: &TimelineState, a: &[Value]) -> String {
    let names = state.names;
    let game = state.game;
    let ty = match a.first().and_then(|v| v.as_i64()) {
        Some(t) => t,
        None => return format!("(empty) {}", serde_json::to_string(a).unwrap_or_default()),
    };

    match ty {
        0 => "START_FIGHT — combat begins".to_string(),
        4 => "END_FIGHT — combat over".to_string(),
        5 => {
            let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let killer = a.get(2).and_then(|v| v.as_i64());
            match killer {
                Some(k) if k >= 0 => format!(
                    "PLAYER_DEAD — {} eliminated (killing blow from {})",
                    leek_label(names, id),
                    leek_label(names, k)
                ),
                _ => format!("PLAYER_DEAD — {} eliminated", leek_label(names, id)),
            }
        }
        6 => {
            let n = a.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
            format!("NEW_TURN — turn {n} begins")
        }
        7 => {
            let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            format!("LEEK_TURN — {} acts", leek_label(names, id))
        }
        8 => {
            let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let tp = a.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
            let pm = a.get(3).and_then(|v| v.as_i64()).unwrap_or(0);
            format!(
                "END_TURN — {} ends turn (TP left: {tp}, MP left: {pm})",
                leek_label(names, id)
            )
        }
        9 => {
            let owner = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let target = a.get(2).and_then(|v| v.as_i64()).unwrap_or(-1);
            let cell = a.get(3).and_then(|v| v.as_i64()).unwrap_or(-1);
            let result = a.get(4).and_then(|v| v.as_i64()).unwrap_or(0);
            format!(
                "SUMMON — {} summons {} on cell {cell} (outcome code {result})",
                leek_label(names, owner),
                leek_label(names, target)
            )
        }
        10 => {
            let leek = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let end = a.get(2).and_then(|v| v.as_i64()).unwrap_or(-1);
            let path = a
                .get(3)
                .and_then(|v| v.as_array())
                .map(|p| {
                    p.iter()
                        .filter_map(|c| c.as_i64())
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(" → ")
                })
                .unwrap_or_default();
            format!(
                "MOVE_TO — {} moves to cell {end} (path: {path})",
                leek_label(names, leek)
            )
        }
        11 => {
            let caster = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let target = a.get(2).and_then(|v| v.as_i64()).unwrap_or(-1);
            format!(
                "KILL — {} receives fatal effect (logged caster id {}, target {})",
                leek_label(names, target),
                caster,
                target
            )
        }
        12 => {
            let chip = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let cell = a.get(2).and_then(|v| v.as_i64()).unwrap_or(-1);
            let ok = a.get(3).and_then(|v| v.as_i64()).unwrap_or(0);
            let actor = state
                .active_entity
                .map(|id| leek_label(names, id))
                .unwrap_or_else(|| "(unknown actor)".to_string());
            let target_hint = leek_at_cell(&state.entity_cell, &state.dead, cell)
                .map(|id| format!("occupant: {}", leek_label(names, id)))
                .unwrap_or_else(|| "cell appears empty (living)".to_string());
            format!(
                "USE_CHIP — {actor} uses {} on cell {cell} ({}) [{}]",
                game.format_chip_template(chip),
                target_hint,
                hit_word(ok)
            )
        }
        13 => {
            let weapon = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let actor = state
                .active_entity
                .map(|id| leek_label(names, id))
                .unwrap_or_else(|| "(unknown actor)".to_string());
            format!(
                "SET_WEAPON — {actor} equips {}",
                game.format_weapon_template(weapon)
            )
        }
        16 => {
            let cell = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let ok = a.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
            let actor = state
                .active_entity
                .map(|id| leek_label(names, id))
                .unwrap_or_else(|| "(unknown actor)".to_string());
            let target_hint = leek_at_cell(&state.entity_cell, &state.dead, cell)
                .map(|id| format!("target: {}", leek_label(names, id)))
                .unwrap_or_else(|| "no living leek on that cell".to_string());
            format!(
                "USE_WEAPON — {actor} attacks cell {cell} ({target_hint}) [{}]",
                hit_word(ok)
            )
        }
        100 => {
            let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let v = a.get(2).and_then(|x| x.as_i64()).unwrap_or(0);
            format!("TP_LOST — {} loses {v} TP", leek_label(names, id))
        }
        101 => {
            let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let life = a.get(2).and_then(|x| x.as_i64()).unwrap_or(0);
            let erosion = a.get(3).and_then(|x| x.as_i64()).unwrap_or(0);
            format!(
                "LIFE_LOST — {} loses {life} HP (erosion: {erosion})",
                leek_label(names, id)
            )
        }
        102 => {
            let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let v = a.get(2).and_then(|x| x.as_i64()).unwrap_or(0);
            format!("MP_LOST — {} loses {v} MP", leek_label(names, id))
        }
        103 => {
            let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let v = a.get(2).and_then(|x| x.as_i64()).unwrap_or(0);
            format!("CARE — {} heals +{v} HP", leek_label(names, id))
        }
        104 => {
            let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let v = a.get(2).and_then(|x| x.as_i64()).unwrap_or(0);
            format!("BOOST_VITA — {} max life +{v}", leek_label(names, id))
        }
        105 => {
            let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            format!("RESURRECT — {}", leek_label(names, id))
        }
        106 => {
            let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let v = a.get(2).and_then(|x| x.as_i64()).unwrap_or(0);
            format!(
                "LOSE_STRENGTH — {} loses {v} strength",
                leek_label(names, id)
            )
        }
        107 | 108 | 109 | 110 | 111 | 112 => damage_kind(ty, names, a),
        201 => "LAMA".to_string(),
        203 => {
            let msg = a.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let preview: String = msg.chars().take(120).collect();
            let ell = if msg.chars().count() > 120 { "…" } else { "" };
            let speaker = state
                .active_entity
                .map(|id| leek_label(names, id))
                .unwrap_or_else(|| "?".to_string());
            format!("SAY — {speaker}: \"{preview}{ell}\"")
        }
        205 => {
            let cell = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
            let color = a.get(2).and_then(|v| v.as_str()).unwrap_or("?");
            format!("SHOW_CELL — highlight cell {cell} ({color})")
        }
        301 | 302 => {
            let is_weapon = ty == 301;
            let item = a.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
            let eff_instance = a.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
            let caster = a.get(3).and_then(|v| v.as_i64()).unwrap_or(-1);
            let target = a.get(4).and_then(|v| v.as_i64()).unwrap_or(-1);
            let effect_id = a.get(5).and_then(|v| v.as_i64()).unwrap_or(0);
            let value = a.get(6).and_then(|v| v.as_i64()).unwrap_or(0);
            let turns = a.get(7).and_then(|v| v.as_i64()).unwrap_or(0);
            let mods = a.get(8).and_then(|v| v.as_i64()).unwrap_or(0);
            let source = if is_weapon {
                game.format_weapon_item(item)
            } else {
                game.format_chip_item(item)
            };
            let et = effect_type_name(effect_id);
            let suffix = if mods != 0 {
                format!(", modifiers {mods}")
            } else {
                String::new()
            };
            let kind = if is_weapon {
                "ADD_WEAPON_EFFECT"
            } else {
                "ADD_CHIP_EFFECT"
            };
            format!(
                "{kind} — {source} applies {et} (#{effect_id}) to {} from {} (value {value}, turns {turns}, effect instance #{eff_instance}{suffix})",
                leek_label(names, target),
                leek_label(names, caster),
            )
        }
        303 => format!(
            "REMOVE_EFFECT — raw {}",
            serde_json::to_string(a).unwrap_or_default()
        ),
        304 => format!(
            "UPDATE_EFFECT — raw {}",
            serde_json::to_string(a).unwrap_or_default()
        ),
        306 => format!(
            "REDUCE_EFFECTS — raw {}",
            serde_json::to_string(a).unwrap_or_default()
        ),
        307 => format!(
            "REMOVE_POISONS — raw {}",
            serde_json::to_string(a).unwrap_or_default()
        ),
        308 => format!(
            "REMOVE_SHACKLES — raw {}",
            serde_json::to_string(a).unwrap_or_default()
        ),
        14 => format!(
            "STACK_EFFECT — raw {}",
            serde_json::to_string(a).unwrap_or_default()
        ),
        15 => format!(
            "OPEN_CHEST — raw {}",
            serde_json::to_string(a).unwrap_or_default()
        ),
        1002 => format!("BUG — raw {}", serde_json::to_string(a).unwrap_or_default()),
        _ => format!(
            "opcode {ty} (raw) {}",
            serde_json::to_string(a).unwrap_or_default()
        ),
    }
}

fn damage_kind(ty: i64, names: &HashMap<i64, String>, a: &[Value]) -> String {
    let label = match ty {
        107 => "NOVA_DAMAGE",
        108 => "DAMAGE_RETURN",
        109 => "LIFE_DAMAGE",
        110 => "POISON_DAMAGE",
        111 => "AFTEREFFECT",
        112 => "NOVA_VITALITY",
        _ => "DAMAGE",
    };
    let id = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1);
    let v1 = a.get(2).and_then(|x| x.as_i64()).unwrap_or(0);
    let v2 = a.get(3).and_then(|x| x.as_i64()).unwrap_or(0);
    format!(
        "{label} — {} (amount {v1}, secondary {v2})",
        leek_label(names, id),
    )
}

fn append_logs_summary(buf: &mut String, logs: Option<&Value>) {
    buf.push_str("Logs\n");
    let Some(logs) = logs else {
        buf.push_str("  (none)\n");
        return;
    };

    if let Some(obj) = logs.as_object() {
        if obj.contains_key("ai") || obj.contains_key("ai_run") {
            buf.push_str("  (Rust VM shape)\n");
            if let Some(ai) = obj.get("ai").and_then(|v| v.as_object()) {
                for (id, v) in ai {
                    let n = v.as_array().map(|a| a.len()).unwrap_or(1);
                    buf.push_str(&format!(
                        "  AI diagnostics · entity {id}: {n} entr(y/ies)\n"
                    ));
                }
            }
            if let Some(run) = obj.get("ai_run").and_then(|v| v.as_object()) {
                buf.push_str(&format!(
                    "  AI run snapshot (turn 1 only): {} entity(es)\n",
                    run.len()
                ));
            }
            return;
        }

        let mut total = 0usize;
        for (farmer, v) in obj {
            if let Some(inner) = v.as_object() {
                for (_action_idx, arr) in inner {
                    if let Some(a) = arr.as_array() {
                        total += a.len();
                    }
                }
                buf.push_str(&format!(
                    "  Farmer {farmer}: {} log bucket(s)\n",
                    inner.len()
                ));
            } else {
                buf.push_str(&format!("  Farmer {farmer}: (non-object)\n"));
            }
        }
        if total > 0 {
            buf.push_str(&format!("  Total farmer-log lines: {total}\n"));
        }
        return;
    }

    buf.push_str(&format!(
        "  {}\n",
        serde_json::to_string(logs).unwrap_or_default()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn formats_minimal_outcome() {
        let v = json!({
            "fight": {
                "leeks": [{"id": 1, "name": "A", "team": 1, "cellPos": 10}],
                "map": {},
                "actions": [[0], [6, 1], [7, 1], [10, 1, 20, [10, 15, 20]], [8, 1, 5, 3], [4]],
                "dead": {},
                "ops": {}
            },
            "logs": {},
            "winner": 1,
            "duration": 1,
            "analyze_time": 0,
            "compilation_time": 0,
            "execution_time": 0
        });
        let game = GameNames::default();
        let s = format_outcome_human_with_game(&v, &game);
        assert!(!s.contains("Patrick"));
        assert!(s.contains("#1 A"));
        assert!(s.contains("NEW_TURN"));
        assert!(s.contains("MOVE_TO"));
        assert!(s.contains("Winner: team 1"));
    }

    #[test]
    fn weapon_name_in_set_weapon_when_data_loaded() {
        let data_dir = find_game_data_dir(Path::new(env!("CARGO_MANIFEST_DIR")));
        let Some(dir) = data_dir else {
            return;
        };
        let game = GameNames::load_from_data_dir(&dir);
        let v = json!({
            "fight": {
                "leeks": [{"id": 1, "name": "A", "team": 1, "cellPos": 10, "weapons": [37]}],
                "map": {},
                "actions": [[7, 1], [13, 1], [8, 1, 0, 0]],
                "dead": {},
                "ops": {}
            },
            "logs": {},
            "winner": 1,
            "duration": 1,
            "analyze_time": 0,
            "compilation_time": 0,
            "execution_time": 0
        });
        let s = format_outcome_human_with_game(&v, &game);
        assert!(
            s.contains("pistol") || s.contains("weapon template 1"),
            "report: {s}"
        );
    }
}
