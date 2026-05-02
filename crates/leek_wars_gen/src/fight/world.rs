use crate::scenario::{EntityInfo, Scenario};
use serde_json::json;
use std::collections::HashMap;

fn effective_int(total: Option<i32>, base: i32) -> i32 {
    total.unwrap_or(base)
}

fn effective_entity_life(e: &EntityInfo) -> i32 {
    effective_int(e.total_life, e.life)
}

use super::chips::ChipStats;
use super::map;
use super::rng::JavaCompatRng;
use super::summons::SummonTemplate;
use super::trace::{TraceConfig, TraceEvent};
use super::weapons::WeaponStats;
use leekscript_run::Value;

fn apply_stat_delta(e: &mut SimEntity, key: i32, delta: i32) {
    match key {
        1 => e.strength += delta,
        2 => e.agility += delta,
        3 => e.wisdom += delta,
        4 => e.resistance += delta,
        5 => e.science += delta,
        6 => e.magic += delta,
        7 => {
            e.total_tp += delta;
            e.tp = e.tp.min(e.total_tp);
        }
        8 => {
            e.total_mp += delta;
            e.mp = e.mp.min(e.total_mp);
        }
        9 => e.damage_return += delta,
        10 => e.absolute_shield = (e.absolute_shield + delta).max(0),
        11 => e.relative_shield = (e.relative_shield + delta).max(0),
        _ => {}
    }
}

#[derive(Debug, Clone)]
pub struct ActiveEffect {
    pub id: i32, // Effect.TYPE_*
    pub caster_fid: i32,
    pub item_id: i32,             // Attack.getItemId() (chip id / weapon item id)
    pub log_id: i32,              // ActionAddEffect id
    pub modifiers: i32,           // Effect.MODIFIER_*
    pub propagate: i32,           // distance (Attack.propagate), 0=no propagation
    pub propagate_modifiers: i32, // modifiers of TYPE_PROPAGATION parameters (stackable/not_replaceable)
    pub base_turns: i32,          // original turns passed to createEffect
    pub value1: f64,              // original value1 (for propagation re-application)
    pub value2: f64,              // original value2 (for propagation re-application)
    pub critical: bool,           // critical flag (for propagation re-application)
    pub state_id: Option<i32>,    // EntityState ordinal for TYPE_ADD_STATE
    pub value: i32,
    pub turns: i32, // -1=infinite, else decremented at start of target's turn
    pub stat_key: Option<i32>, // reversible stat deltas
}

#[derive(Debug, Clone)]
pub struct SimEntity {
    pub fid: i32,
    /// 0-based team index = outer index in `scenario.entities` (matches official generator `State.addEntity(t, …)` / `getTeam()` / `getSide`).
    pub team: i32,
    /// Scenario JSON `team` id (`Entity.setTeamID` / `getTeamID()`), e.g. 1, 2.
    pub team_id: i32,
    /// Scenario entity `id` (`getLeekID()`), distinct from fight `fid` (`getLeek()` / `getEntity()`).
    pub leek_id: i32,
    pub level: i32,
    pub name: String,
    pub cell: i32,
    /// Cell at fight init (Official generator `Actions.addEntity` uses this, not the final position).
    pub spawn_cell: i32,
    pub life: i32,
    pub total_life: i32,
    pub strength: i32,
    pub agility: i32,
    pub wisdom: i32,
    pub resistance: i32,
    pub science: i32,
    pub magic: i32,
    /// Official generator: `Entity.STAT_POWER` (battle royale etc). Not yet driven by effects in this port.
    pub power: i32,
    pub absolute_shield: i32,
    pub relative_shield: i32, // percent (0..100+)
    pub damage_return: i32,   // percent (0..100+)
    pub tp: i32,
    pub mp: i32,
    /// Reset to this at the start of each of this entity's turns (Official generator `endTurn` zeroes `usedTP`/`usedMP`).
    pub total_tp: i32,
    pub total_mp: i32,
    pub frequency: i32,
    /// Scenario entity type (Official generator `Entity.getType()`), e.g. leek/mob/turret/chest.
    pub entity_type: i32,
    pub skin: i32,
    pub hat: i32,
    pub metal: bool,
    pub face: i32,
    pub weapons: Vec<i32>,
    pub chips: Vec<i32>,
    /// Component **template** ids from scenario / API (metadata; not all combat paths consume this yet).
    pub components: Vec<i32>,
    /// Equipped weapon **item** id (`weapons.json` / scenario), if any.
    pub equipped_weapon: Option<i32>,
    pub ai_path: String,
    /// Official generator: `Entity.getAIId()` (server-side AI id); from scenario `ai_folder`.
    pub ai_id: i32,
    /// Official generator: `Entity.getAIName()`; from scenario `ai` string.
    pub ai_name: String,
    /// Scenario AI language version override (Official generator `EntityInfo.aiVersion`). `0` means default / use preamble.
    pub ai_version: i32,
    /// Scenario strict-mode override (Official generator `EntityInfo.aiStrict`).
    pub ai_strict: bool,
    /// Official generator: `Entity.getBirthTurn()`; leeks start at 1, summons at creation turn.
    pub birth_turn: i32,
    pub cores: i32,
    pub ram: i32,
    pub farmer_id: i32,
    pub farmer_name: String,
    pub farmer_country: String,
    /// Official generator: `Generator` outcome `logs` map key: `ai_owner`, or `0` for `TYPE_MOB` (`4`).
    pub log_bucket_owner: i32,
    pub team_name: String,
    pub dead: bool,
    pub effects: Vec<ActiveEffect>,
    /// Log-ids of active effects this entity has launched onto others (Official generator `launchedEffects`).
    pub launched_effect_log_ids: Vec<i32>,
    pub is_summon: bool,
    pub summoner_fid: Option<i32>,
    /// Per-chip remaining cooldown turns (Official generator `Entity.mCooldown`); key = chip id.
    pub chip_cooldowns: HashMap<i32, i32>,
    /// Uses of each chip this turn (Official generator `Entity.itemUses`); cleared in `end_turn`.
    pub chip_uses_turn: HashMap<i32, i32>,
    /// Official generator: `Entity.saysTurn` (reset in `end_turn`); caps `say()` per leek turn.
    pub says_turn: i32,
}

/// Official generator: `FarmerLog`: debug / marks / pauses grouped by fight action index (`Actions.getNextId() - 1`).
#[derive(Debug, Clone, Default)]
pub struct FarmerAiLog {
    /// Official generator: `FarmerLog.mAction` (last bucket key written).
    current_action_bucket: i32,
    /// String keys = action index, values = JSON arrays of log rows.
    pub buckets: serde_json::Map<String, serde_json::Value>,
}

impl FarmerAiLog {
    /// Append one row; `fight_actions_len` is `world.actions.len()` (Official generator `Actions.getNextId()`).
    fn append_line(&mut self, fight_actions_len: usize, line: serde_json::Value) {
        let next_id = fight_actions_len;
        let bucket = if next_id == 0 {
            0_i32
        } else {
            (next_id - 1) as i32
        }
        .max(0);
        if self.current_action_bucket < bucket {
            self.buckets.insert(bucket.to_string(), json!([]));
            self.current_action_bucket = bucket;
        }
        let key = bucket.to_string();
        if let Some(serde_json::Value::Array(arr)) = self.buckets.get_mut(&key) {
            arr.push(line);
        }
    }
}

#[derive(Debug)]
pub struct FightWorld {
    pub entities: Vec<SimEntity>,
    pub team_fids: Vec<Vec<i32>>,
    pub turn_fids: Vec<i32>,
    /// Fight-start entity order (Official generator `State.initialOrder` / first `StartOrder.compute`); used to insert resurrected leeks before the next living leek in that list.
    pub initial_fids: Vec<i32>,
    pub active_fid: i32,
    /// Current global fight turn (Official generator `Order.turn` / `State.getTurn()`).
    pub active_turn: i32,
    pub max_turns: i32,
    pub seed: i64,
    pub rng: JavaCompatRng,
    pub map_w: i32,
    pub map_h: i32,
    /// `cell_id -> obstacle` (raw value from scenario map when present).
    pub obstacles: std::collections::BTreeMap<i32, i32>,
    /// When set (generator bootstrap), exact `fight.map.obstacles` object for outcome JSON; simulation still uses [`Self::obstacles`].
    pub outcome_obstacles_json: Option<serde_json::Value>,
    /// Official generator: `Map.getType()` (defaults to 0 when absent).
    pub map_type: i32,
    /// Official generator: fight id / type / context / boss (scenario metadata; 0 when absent).
    pub fight_id: i32,
    pub fight_type: i32,
    pub fight_context: i32,
    pub fight_boss: i32,
    /// Wall-clock fight start (`State.date` / `AI.getDate()`), Unix seconds.
    pub fight_start_unix_secs: i64,
    /// Scenario `max_operations_per_entity` when present (VM operation budget).
    pub max_operations_per_entity: Option<u64>,
    pub chips_by_id: HashMap<i32, ChipStats>,
    pub summons_by_id: HashMap<i32, SummonTemplate>,
    pub weapons_by_item: HashMap<i32, WeaponStats>,
    pub draw_check_life: bool,
    /// Official-generator-style `Actions.actions` log (arrays like `[ActionCode, ...args]`).
    pub actions: Vec<serde_json::Value>,
    /// Cumulative VM operations per `fid` (Official generator statistics `getOperationsByEntity` / outcome `fight.ops`).
    pub entity_ops_totals: HashMap<i32, u64>,
    pub next_effect_log_id: i32,
    /// Per-team chip cooldowns (Official generator `Team.cooldowns`) when `ChipStats.team_cooldown` is set.
    pub team_chip_cooldowns: Vec<HashMap<i32, i32>>,
    /// Official generator: `State.registers`-like key/value storage (used by `getRegister*` natives).
    pub registers: HashMap<String, String>,
    /// Per-entity network message inbox (Official generator `EntityAI.mMessages` + `sendAll`/`sendTo`/`getMessages`).
    pub inbox: HashMap<i32, Vec<Value>>,
    /// Pending `say()` lines for `listen()` (Official generator `EntityAI.mSays`); cleared after each entity AI run.
    pub say_inbox: HashMap<i32, Vec<(i32, String)>>,
    /// Official generator: `Outcome.logs`: `ai_owner` → [`FarmerLog`](FarmerAiLog) JSON.
    pub farmer_ai_logs: HashMap<i32, FarmerAiLog>,
    /// Optional Rust-only trace (see [`super::trace`]).
    pub trace: Option<TraceSink>,
}

/// Mutable trace buffer attached to a fight when tracing is enabled.
#[derive(Debug, Clone)]
pub struct TraceSink {
    pub config: TraceConfig,
    pub events: Vec<TraceEvent>,
}

impl FightWorld {
    /// Official generator: `State.MAX_TURNS` (`Chip.getCooldown() == -1` → `MAX_TURNS + 2` turns).
    pub const JAVA_MAX_TURNS: i32 = 64;

    pub fn trace_event(&mut self, turn: i32, fid: i32, kind: &str, detail: Option<serde_json::Value>) {
        let Some(sink) = self.trace.as_mut() else {
            return;
        };
        if !sink.config.enabled || sink.events.len() >= sink.config.max_events {
            return;
        }
        sink.events.push(TraceEvent {
            turn,
            fid,
            kind: kind.to_string(),
            detail,
        });
    }

    pub fn cleanup_on_death(&mut self, dead_fid: i32) {
        // Remove launched effects (log removals on targets).
        let launched = self
            .entity(dead_fid)
            .map(|e| e.launched_effect_log_ids.clone())
            .unwrap_or_default();

        if !launched.is_empty() {
            let mut removed_log_ids: Vec<i32> = Vec::new();
            for target in &mut self.entities {
                if target.dead {
                    continue;
                }
                for i in (0..target.effects.len()).rev() {
                    if launched.contains(&target.effects[i].log_id) {
                        // Revert stat delta if needed.
                        if let Some(key) = target.effects[i].stat_key {
                            apply_stat_delta(target, key, -target.effects[i].value);
                        }
                        let log_id = target.effects[i].log_id;
                        target.effects.swap_remove(i);
                        removed_log_ids.push(log_id);
                    }
                }
            }
            for log_id in removed_log_ids {
                self.log_action(serde_json::json!([303, log_id]));
            }
        }

        // Clear effects on the dead entity without logging removals (Official generator client removes them).
        if let Some(dead) = self.entity_mut(dead_fid) {
            dead.effects.clear();
            dead.launched_effect_log_ids.clear();
        }

        // Kill summons of this entity.
        let summon_fids: Vec<i32> = self
            .entities
            .iter()
            .filter(|e| !e.dead && e.is_summon && e.summoner_fid == Some(dead_fid))
            .map(|e| e.fid)
            .collect();
        for &sfid in &summon_fids {
            if let Some(s) = self.entity_mut(sfid) {
                s.dead = true;
                s.life = 0;
            }
            // Mirror existing death action format (who killed is unknown here).
            self.log_action(serde_json::json!([5, sfid, dead_fid]));
        }

        // Official generator: `Order.removeEntity` removes dead entities from the play order.
        // We keep `team_fids` stable, but prune `turn_fids` (dead leeks are re-inserted on resurrect).
        self.turn_fids.retain(|&fid| {
            if fid == dead_fid {
                return false;
            }
            if summon_fids.contains(&fid) {
                return false;
            }
            true
        });
    }
    pub fn from_scenario(
        sc: &Scenario,
        weapons_by_item: HashMap<i32, WeaponStats>,
        chips_by_id: HashMap<i32, ChipStats>,
        summons_by_id: HashMap<i32, SummonTemplate>,
    ) -> Self {
        let farmer_by_id: HashMap<i32, (&str, &str)> = sc
            .farmers
            .iter()
            .map(|f| (f.id, (f.name.as_str(), f.country.as_str())))
            .collect();
        let team_name_by_id: HashMap<i32, &str> =
            sc.teams.iter().map(|t| (t.id, t.name.as_str())).collect();

        let mut entities = Vec::new();
        let mut team_fids: Vec<Vec<i32>> = Vec::new();

        for (team_idx, team_entities) in sc.entities.iter().enumerate() {
            let mut fids = Vec::new();
            for e in team_entities {
                let fid = entities.len() as i32;
                fids.push(fid);
                let cell = e.cell.unwrap_or(0);
                let team_id = if e.team > 0 {
                    e.team
                } else {
                    (team_idx as i32) + 1
                };
                let (farmer_name, farmer_country) = farmer_by_id
                    .get(&e.farmer)
                    .map(|(n, c)| ((*n).to_string(), (*c).to_string()))
                    .unwrap_or_else(|| ("".into(), "?".into()));
                let team_name = team_name_by_id
                    .get(&team_id)
                    .map(|s| (*s).to_string())
                    .unwrap_or_default();
                let weapons: Vec<i32> = e
                    .weapons
                    .iter()
                    .copied()
                    .filter(|wid| weapons_by_item.contains_key(wid))
                    .collect();
                let life0 = effective_entity_life(e);
                let tp0 = effective_int(e.total_tp, e.tp);
                let mp0 = effective_int(e.total_mp, e.mp);
                entities.push(SimEntity {
                    fid,
                    team: team_idx as i32,
                    team_id,
                    leek_id: e.id,
                    level: e.level,
                    name: e.name.clone(),
                    cell,
                    spawn_cell: cell,
                    life: life0,
                    total_life: life0,
                    strength: effective_int(e.total_strength, e.strength),
                    agility: effective_int(e.total_agility, e.agility),
                    wisdom: effective_int(e.total_wisdom, e.wisdom),
                    resistance: effective_int(e.total_resistance, e.resistance),
                    science: effective_int(e.total_science, e.science),
                    magic: effective_int(e.total_magic, e.magic),
                    power: 0,
                    absolute_shield: 0,
                    relative_shield: 0,
                    damage_return: 0,
                    tp: tp0,
                    mp: mp0,
                    total_tp: tp0,
                    total_mp: mp0,
                    frequency: effective_int(e.total_frequency, e.frequency),
                    entity_type: e.r#type,
                    skin: e.skin,
                    hat: e.hat,
                    metal: e.metal,
                    face: e.face,
                    weapons,
                    chips: e.chips.clone(),
                    components: e.components.clone(),
                    equipped_weapon: None,
                    ai_path: e.ai.clone(),
                    ai_id: e.ai_folder,
                    ai_name: e.ai.clone(),
                    ai_version: e.ai_version,
                    ai_strict: e.ai_strict,
                    birth_turn: 1,
                    cores: effective_int(e.total_cores, e.cores),
                    ram: effective_int(e.total_ram, e.ram),
                    farmer_id: e.farmer,
                    farmer_name,
                    farmer_country,
                    log_bucket_owner: if e.r#type == 4 { 0 } else { e.ai_owner },
                    team_name,
                    dead: e.dead,
                    effects: Vec::new(),
                    launched_effect_log_ids: Vec::new(),
                    is_summon: false,
                    summoner_fid: None,
                    chip_cooldowns: HashMap::new(),
                    chip_uses_turn: HashMap::new(),
                    says_turn: 0,
                });
            }
            team_fids.push(fids);
        }

        let team_cd_count = team_fids.len();
        let seed = sc.random_seed.unwrap_or(1) as i64;
        let (map_w, map_h) = sc.engine_map_size_java_main();
        let obstacles = sc.map_obstacles();
        let map_type = sc.map_type();
        let fight_id = sc
            .extra
            .get("fight_id")
            .and_then(|v| v.as_i64())
            .or_else(|| sc.extra.get("fightId").and_then(|v| v.as_i64()))
            .or_else(|| {
                sc.extra
                    .get("fight")
                    .and_then(|v| v.get("id"))
                    .and_then(|v| v.as_i64())
            })
            .unwrap_or(0) as i32;
        let fight_type = sc
            .extra
            .get("fight_type")
            .and_then(|v| v.as_i64())
            .or_else(|| sc.extra.get("fightType").and_then(|v| v.as_i64()))
            .or_else(|| {
                sc.extra
                    .get("fight")
                    .and_then(|v| v.get("type"))
                    .and_then(|v| v.as_i64())
            })
            .unwrap_or(0) as i32;
        let fight_context = sc
            .extra
            .get("fight_context")
            .and_then(|v| v.as_i64())
            .or_else(|| sc.extra.get("fightContext").and_then(|v| v.as_i64()))
            .or_else(|| {
                sc.extra
                    .get("fight")
                    .and_then(|v| v.get("context"))
                    .and_then(|v| v.as_i64())
            })
            .unwrap_or(0) as i32;
        let fight_boss = sc
            .extra
            .get("fight_boss")
            .and_then(|v| v.as_i64())
            .or_else(|| sc.extra.get("fightBoss").and_then(|v| v.as_i64()))
            .or_else(|| {
                sc.extra
                    .get("fight")
                    .and_then(|v| v.get("boss"))
                    .and_then(|v| v.as_i64())
            })
            .unwrap_or(0) as i32;
        let fight_start_unix_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let max_operations_per_entity = sc
            .extra
            .get("max_operations_per_entity")
            .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|i| i.max(0) as u64)));

        let mut team_chip_cooldowns: Vec<HashMap<i32, i32>> =
            (0..team_cd_count).map(|_| HashMap::new()).collect();
        Self::apply_initial_chip_cooldowns(&chips_by_id, &mut entities, &mut team_chip_cooldowns);

        let mut farmer_ai_logs: HashMap<i32, FarmerAiLog> = HashMap::new();
        for e in &entities {
            farmer_ai_logs
                .entry(e.log_bucket_owner)
                .or_insert_with(FarmerAiLog::default);
        }

        Self {
            entities,
            team_fids,
            turn_fids: Vec::new(),
            initial_fids: Vec::new(),
            active_fid: 0,
            active_turn: 1,
            max_turns: sc.max_turns,
            seed,
            rng: JavaCompatRng::new(seed),
            map_w,
            map_h,
            obstacles,
            outcome_obstacles_json: None,
            map_type,
            fight_id,
            fight_type,
            fight_context,
            fight_boss,
            fight_start_unix_secs,
            max_operations_per_entity,
            chips_by_id,
            summons_by_id,
            weapons_by_item,
            draw_check_life: sc.draw_check_life,
            actions: Vec::new(),
            entity_ops_totals: HashMap::new(),
            next_effect_log_id: 1,
            team_chip_cooldowns,
            registers: HashMap::new(),
            inbox: HashMap::new(),
            say_inbox: HashMap::new(),
            farmer_ai_logs,
            trace: None,
        }
    }

    /// Official generator: `Outcome.logs` object (`ai_owner` string keys → `FarmerLog.toJSON()`).
    pub fn outcome_logs_json(&self) -> serde_json::Value {
        let mut owners: Vec<i32> = self.farmer_ai_logs.keys().copied().collect();
        owners.sort_unstable();
        let mut map = serde_json::Map::new();
        for o in owners {
            let fl = &self.farmer_ai_logs[&o];
            map.insert(o.to_string(), serde_json::Value::Object(fl.buckets.clone()));
        }
        serde_json::Value::Object(map)
    }

    /// Official generator: `State` fight init: for each chip with `initial_cooldown > 0`, call
    /// `addCooldown(entity, chip, chip.getInitialCooldown() + 1)` for every starting entity
    /// (routing team chips to `Team.cooldowns`).
    fn apply_initial_chip_cooldowns(
        chips_by_id: &HashMap<i32, ChipStats>,
        entities: &mut [SimEntity],
        team_chip_cooldowns: &mut Vec<HashMap<i32, i32>>,
    ) {
        use std::collections::HashSet;

        for chip in chips_by_id.values() {
            if chip.initial_cooldown <= 0 {
                continue;
            }
            let turns = chip.initial_cooldown + 1;
            if chip.team_cooldown {
                let mut seen: HashSet<usize> = HashSet::new();
                for e in entities.iter() {
                    let team = e.team.max(0) as usize;
                    if seen.insert(team) {
                        if team_chip_cooldowns.len() <= team {
                            team_chip_cooldowns.resize_with(team + 1, HashMap::new);
                        }
                        team_chip_cooldowns[team].insert(chip.chip_id, turns);
                    }
                }
            } else {
                for e in entities.iter_mut() {
                    e.chip_cooldowns.insert(chip.chip_id, turns);
                }
            }
        }
    }

    /// Official generator: `Entity.hasCooldown` / `Team.hasCooldown` depending on `team_cooldown`.
    pub fn chip_on_cooldown(&self, caster_fid: i32, cs: &ChipStats) -> bool {
        if cs.team_cooldown {
            let team = self.entity(caster_fid).map(|e| e.team).unwrap_or(-1);
            if team < 0 {
                return false;
            }
            self.team_chip_cooldowns
                .get(team as usize)
                .is_some_and(|m| m.contains_key(&cs.chip_id))
        } else {
            self.entity(caster_fid)
                .is_some_and(|e| e.chip_cooldowns.contains_key(&cs.chip_id))
        }
    }

    /// Official generator: `Entity.addCooldown` / `Team.addCooldown` after a successful chip use.
    pub fn apply_chip_cooldown(&mut self, caster_fid: i32, cs: &ChipStats) {
        if cs.cooldown == 0 {
            return;
        }
        let turns = if cs.cooldown == -1 {
            Self::JAVA_MAX_TURNS + 2
        } else {
            cs.cooldown
        };
        if cs.team_cooldown {
            let team = self.entity(caster_fid).map(|e| e.team).unwrap_or(0).max(0) as usize;
            if self.team_chip_cooldowns.len() <= team {
                self.team_chip_cooldowns.resize_with(team + 1, HashMap::new);
            }
            self.team_chip_cooldowns[team].insert(cs.chip_id, turns);
        } else if let Some(e) = self.entity_mut(caster_fid) {
            e.chip_cooldowns.insert(cs.chip_id, turns);
        }
    }

    pub fn register_chip_use_after_success(&mut self, caster_fid: i32, cs: &ChipStats) {
        self.apply_chip_cooldown(caster_fid, cs);
        if let Some(e) = self.entity_mut(caster_fid) {
            *e.chip_uses_turn.entry(cs.chip_id).or_insert(0) += 1;
        }
    }

    /// Official generator: `Team.applyCoolDown` when the global fight turn advances.
    pub fn tick_all_team_chip_cooldowns(&mut self) {
        for m in &mut self.team_chip_cooldowns {
            let keys: Vec<i32> = m.keys().copied().collect();
            for k in keys {
                let Some(v) = m.get(&k).copied() else {
                    continue;
                };
                if v <= 1 {
                    m.remove(&k);
                } else {
                    m.insert(k, v - 1);
                }
            }
        }
    }

    pub fn log_action(&mut self, action: serde_json::Value) {
        self.actions.push(action);
    }

    /// Official generator: `FarmerLog.addLog` (AILog types1/2/3 + optional color for `debugC` + optional `[fileId, line]`).
    pub fn push_ai_debug_log(
        &mut self,
        log_owner: i32,
        fid: i32,
        log_type: i32,
        message: &str,
        color: Option<i32>,
        position: Option<(i32, i32)>,
    ) {
        if message.is_empty() {
            return;
        }
        self.farmer_ai_logs
            .entry(log_owner)
            .or_insert_with(FarmerAiLog::default);
        let n = self.actions.len();
        let mut row = vec![json!(fid), json!(log_type), json!(message)];
        let java_color = color.unwrap_or(-1);
        if java_color >= 0 || position.is_some() {
            row.push(json!(java_color));
        }
        if let Some((file_id, line)) = position {
            row.push(json!(file_id));
            row.push(json!(line));
        }
        let line = serde_json::Value::Array(row);
        if let Some(flog) = self.farmer_ai_logs.get_mut(&log_owner) {
            flog.append_line(n, line);
        }
    }

    /// Official generator: `FarmerLog.addSystemLogString` — farmer log rows `[fid, type, trace, key, params?]`.
    pub fn push_ai_system_log(
        &mut self,
        log_owner: i32,
        fid: i32,
        log_type: i32,
        trace: &str,
        key: i32,
        parameters: Option<&[String]>,
    ) {
        self.farmer_ai_logs
            .entry(log_owner)
            .or_insert_with(FarmerAiLog::default);
        let n = self.actions.len();
        let mut row = vec![json!(fid), json!(log_type), json!(trace), json!(key)];
        if let Some(ps) = parameters {
            row.push(json!(ps));
        }
        let line = serde_json::Value::Array(row);
        if let Some(flog) = self.farmer_ai_logs.get_mut(&log_owner) {
            flog.append_line(n, line);
        }
    }

    /// Official generator: `Order.addSummon(owner, invoc)` inserts the summon right after its owner.
    pub fn add_summon_after(&mut self, owner_fid: i32, mut ent: SimEntity) -> i32 {
        let fid = self.entities.len() as i32;
        ent.fid = fid;
        ent.team = self.entity(owner_fid).map(|e| e.team).unwrap_or(ent.team);
        ent.team_id = self
            .entity(owner_fid)
            .map(|e| e.team_id)
            .unwrap_or(ent.team_id);
        ent.is_summon = true;
        ent.summoner_fid = Some(owner_fid);

        let team = ent.team.max(0) as usize;
        if self.team_fids.len() <= team {
            self.team_fids.resize_with(team + 1, Vec::new);
        }
        self.team_fids[team].push(fid);
        self.entities.push(ent);

        if let Some(i) = self.turn_fids.iter().position(|&x| x == owner_fid) {
            self.turn_fids.insert(i + 1, fid);
        } else {
            self.turn_fids.push(fid);
        }
        fid
    }

    /// Official generator: `State.SUMMON_LIMIT`
    pub const SUMMON_LIMIT: i32 = 8;

    /// Living summons on a team (`Entity.isSummon` / same `team` index).
    pub fn team_summon_count(&self, team: i32) -> i32 {
        self.entities
            .iter()
            .filter(|e| !e.dead && e.is_summon && e.team == team)
            .count() as i32
    }

    /// Official generator: `Cell.available`: walkable, no living entity.
    pub fn cell_available_for_summon(&self, cell: i32) -> bool {
        map::is_valid_cell(self.map_w, self.map_h, cell)
            && !self.is_obstacle_cell(cell)
            && self.living_entity_on_cell(cell, None).is_none()
    }

    /// Official generator: `BulbTemplate.base` scaling (`createInvocation`).
    fn bulb_invocation_stat(min_v: i32, max_v: i32, owner_level: i32, critical: bool) -> i32 {
        let c = (owner_level.min(300) as f64) / 300.0;
        let mult = if critical { 1.2 } else { 1.0 };
        ((min_v as f64 + ((max_v - min_v) as f64 * c).floor()) * mult).round() as i32
    }

    /// Spawn a bulb on `cell` after validations. Returns new `fid`, or `None` if owner dead / cell blocked / unknown template.
    pub fn summon_bulb(
        &mut self,
        owner_fid: i32,
        bulb_template_id: i32,
        cell: i32,
        critical: bool,
    ) -> Option<i32> {
        if !self.cell_available_for_summon(cell) {
            return None;
        }
        let tpl = self.summons_by_id.get(&bulb_template_id)?;
        let (owner_level, team, team_id, frequency) = {
            let o = self.entity(owner_fid)?;
            if o.dead {
                return None;
            }
            (o.level, o.team, o.team_id, o.frequency)
        };

        let life = Self::bulb_invocation_stat(tpl.life.0, tpl.life.1, owner_level, critical);
        let tp = Self::bulb_invocation_stat(tpl.tp.0, tpl.tp.1, owner_level, critical);
        let mp = Self::bulb_invocation_stat(tpl.mp.0, tpl.mp.1, owner_level, critical);

        let ent = SimEntity {
            fid: 0,
            team,
            team_id,
            leek_id: -bulb_template_id,
            level: owner_level,
            name: tpl.name.clone(),
            cell,
            spawn_cell: cell,
            life,
            total_life: life,
            strength: Self::bulb_invocation_stat(
                tpl.strength.0,
                tpl.strength.1,
                owner_level,
                critical,
            ),
            agility: Self::bulb_invocation_stat(
                tpl.agility.0,
                tpl.agility.1,
                owner_level,
                critical,
            ),
            wisdom: Self::bulb_invocation_stat(tpl.wisdom.0, tpl.wisdom.1, owner_level, critical),
            resistance: Self::bulb_invocation_stat(
                tpl.resistance.0,
                tpl.resistance.1,
                owner_level,
                critical,
            ),
            science: Self::bulb_invocation_stat(
                tpl.science.0,
                tpl.science.1,
                owner_level,
                critical,
            ),
            magic: Self::bulb_invocation_stat(tpl.magic.0, tpl.magic.1, owner_level, critical),
            power: 0,
            absolute_shield: 0,
            relative_shield: 0,
            damage_return: 0,
            tp,
            mp,
            total_tp: tp,
            total_mp: mp,
            frequency,
            // Official generator: `Entity.TYPE_BULB`
            entity_type: 1,
            skin: 0,
            hat: 0,
            metal: false,
            face: 0,
            weapons: Vec::new(),
            chips: tpl.chips.clone(),
            components: Vec::new(),
            equipped_weapon: None,
            ai_path: String::new(),
            ai_id: self.entity(owner_fid).map(|o| o.ai_id).unwrap_or(0),
            ai_name: self
                .entity(owner_fid)
                .map(|o| o.ai_name.clone())
                .unwrap_or_default(),
            ai_version: 0,
            ai_strict: false,
            birth_turn: self.active_turn,
            cores: 0,
            ram: 0,
            farmer_id: self.entity(owner_fid).map(|o| o.farmer_id).unwrap_or(0),
            farmer_name: self
                .entity(owner_fid)
                .map(|o| o.farmer_name.clone())
                .unwrap_or_default(),
            farmer_country: self
                .entity(owner_fid)
                .map(|o| o.farmer_country.clone())
                .unwrap_or_else(|| "?".into()),
            log_bucket_owner: self
                .entity(owner_fid)
                .map(|o| o.log_bucket_owner)
                .unwrap_or(0),
            team_name: self
                .entity(owner_fid)
                .map(|o| o.team_name.clone())
                .unwrap_or_default(),
            dead: false,
            effects: Vec::new(),
            launched_effect_log_ids: Vec::new(),
            is_summon: true,
            summoner_fid: Some(owner_fid),
            chip_cooldowns: HashMap::new(),
            chip_uses_turn: HashMap::new(),
            says_turn: 0,
        };
        Some(self.add_summon_after(owner_fid, ent))
    }

    pub fn ensure_in_turn_order(&mut self, fid: i32) {
        if !self.turn_fids.contains(&fid) {
            self.turn_fids.push(fid);
        }
    }

    /// Apply start-of-turn effect ticking (subset).
    /// Official generator: does this in `Entity.startTurn()` before the AI runs.
    pub fn start_turn(&mut self, fid: i32) {
        let mut removed: Vec<(i32, i32)> = Vec::new(); // (log_id, caster_fid)
        {
            let Some(e) = self.entity_mut(fid) else {
                return;
            };
            if e.dead {
                return;
            }
            // Official generator: `Entity.applyCoolDown` at start of turn.
            let keys: Vec<i32> = e.chip_cooldowns.keys().copied().collect();
            for k in keys {
                let Some(v) = e.chip_cooldowns.get(&k).copied() else {
                    continue;
                };
                if v <= 1 {
                    e.chip_cooldowns.remove(&k);
                } else {
                    e.chip_cooldowns.insert(k, v - 1);
                }
            }
            // Decrement duration and drop expired.
            for i in (0..e.effects.len()).rev() {
                if e.effects[i].turns != -1 {
                    e.effects[i].turns -= 1;
                    if e.effects[i].turns <= 0 {
                        // revert stat deltas
                        if let Some(key) = e.effects[i].stat_key {
                            let delta = -e.effects[i].value;
                            apply_stat_delta(e, key, delta);
                        }
                        let log_id = e.effects[i].log_id;
                        let caster_fid = e.effects[i].caster_fid;
                        removed.push((log_id, caster_fid));
                        e.effects.swap_remove(i);
                    }
                }
            }
        }
        for (log_id, caster_fid) in removed {
            self.remove_launched_effect_log(caster_fid, log_id);
            self.log_action(serde_json::json!([303, log_id]));
        }
    }

    pub fn apply_effect_stat_delta(&mut self, target_fid: i32, effect: ActiveEffect) {
        let caster_fid = effect.caster_fid;
        let log_id = effect.log_id;
        if let Some(e) = self.entity_mut(target_fid) {
            if e.dead {
                return;
            }
            if let Some(key) = effect.stat_key {
                apply_stat_delta(e, key, effect.value);
            }
            e.effects.push(effect);
        }
        // Track launched effects on the caster (Official generator `Entity.addLaunchedEffect`).
        if let Some(caster) = self.entity_mut(caster_fid) {
            caster.launched_effect_log_ids.push(log_id);
        }
    }

    pub fn alloc_effect_log_id(&mut self) -> i32 {
        let id = self.next_effect_log_id;
        self.next_effect_log_id += 1;
        id
    }

    /// Official generator: `Entity.removeLaunchedEffect` — drop one occurrence of `log_id` from the caster's list.
    pub fn remove_launched_effect_log(&mut self, caster_fid: i32, log_id: i32) {
        if let Some(c) = self.entity_mut(caster_fid) {
            if let Some(i) = c.launched_effect_log_ids.iter().position(|&x| x == log_id) {
                c.launched_effect_log_ids.swap_remove(i);
            }
        }
    }

    pub fn reduce_effects(&mut self, target_fid: i32, percent: f64) {
        let mut to_remove: Vec<(i32, i32)> = Vec::new(); // (log_id, caster_fid)
        let mut to_update: Vec<(i32, i32)> = Vec::new(); // (log_id, new_value)
        {
            let Some(e) = self.entity_mut(target_fid) else {
                return;
            };
            if e.dead {
                return;
            }
            let p = percent.clamp(0.0, 1.0);
            for i in (0..e.effects.len()).rev() {
                // Irreductible effect? skip (Official generator `Effect.MODIFIER_IRREDUCTIBLE` = 16)
                if (e.effects[i].modifiers & 16) != 0 {
                    continue;
                }
                let old = e.effects[i].value;
                let newv = ((old as f64) * (1.0 - p)).round() as i32;
                let delta_change = newv - old;
                if let Some(key) = e.effects[i].stat_key {
                    apply_stat_delta(e, key, delta_change);
                }
                e.effects[i].value = newv;
                if e.effects[i].value <= 0 {
                    let log_id = e.effects[i].log_id;
                    let caster_fid = e.effects[i].caster_fid;
                    to_remove.push((log_id, caster_fid));
                    e.effects.swap_remove(i);
                } else {
                    to_update.push((e.effects[i].log_id, e.effects[i].value));
                }
            }
        }
        // Official generator: logs per-effect update/remove
        for (log_id, newv) in to_update {
            self.log_action(serde_json::json!([304, log_id, newv]));
        }
        for (log_id, caster_fid) in to_remove {
            self.remove_launched_effect_log(caster_fid, log_id);
            self.log_action(serde_json::json!([303, log_id]));
        }
    }

    /// Official generator: `Entity.reduceEffectsTotal` — like [`reduce_effects`] but does not skip irreducible effects.
    pub fn reduce_effects_total(&mut self, target_fid: i32, percent: f64) {
        let mut to_remove: Vec<(i32, i32)> = Vec::new();
        let mut to_update: Vec<(i32, i32)> = Vec::new();
        {
            let Some(e) = self.entity_mut(target_fid) else {
                return;
            };
            if e.dead {
                return;
            }
            let p = percent.clamp(0.0, 1.0);
            for i in (0..e.effects.len()).rev() {
                let old = e.effects[i].value;
                let newv = ((old as f64) * (1.0 - p)).round() as i32;
                let delta_change = newv - old;
                if let Some(key) = e.effects[i].stat_key {
                    apply_stat_delta(e, key, delta_change);
                }
                e.effects[i].value = newv;
                if e.effects[i].value <= 0 {
                    let log_id = e.effects[i].log_id;
                    let caster_fid = e.effects[i].caster_fid;
                    to_remove.push((log_id, caster_fid));
                    e.effects.swap_remove(i);
                } else {
                    to_update.push((e.effects[i].log_id, e.effects[i].value));
                }
            }
        }
        for (log_id, newv) in to_update {
            self.log_action(serde_json::json!([304, log_id, newv]));
        }
        for (log_id, caster_fid) in to_remove {
            self.remove_launched_effect_log(caster_fid, log_id);
            self.log_action(serde_json::json!([303, log_id]));
        }
    }

    /// Official generator: `Entity.endTurn()` handles propagation of active effects.
    pub fn end_turn(&mut self, fid: i32) {
        let (self_cell, effects_snapshot) = match self.entity(fid) {
            Some(e) if !e.dead => (e.cell, e.effects.clone()),
            _ => return,
        };
        if let Some(e) = self.entity_mut(fid) {
            e.chip_uses_turn.clear();
            e.says_turn = 0;
            // Official generator: `Entity.endTurn()` resets used counters; `ActionEndTurn` is logged *after* this,
            // so it observes full TP/MP again (via `getTP()` / `getMP()`).
            e.tp = e.total_tp;
            e.mp = e.total_mp;
        }
        for eff in effects_snapshot {
            if eff.propagate <= 0 {
                continue;
            }
            // Sample one jet per propagated effect (official generator).
            let jet = self.rng.next_double01();
            let around: Vec<i32> = self
                .entities
                .iter()
                .filter(|e| !e.dead && e.fid != fid)
                .filter(|e| {
                    super::map::case_distance(self.map_w, self_cell, e.cell) <= eff.propagate
                })
                .map(|e| e.fid)
                .collect();
            for tfid in around {
                // not-replaceable propagation: if target already has an effect from this item, skip
                if (eff.propagate_modifiers & 8) != 0 {
                    if self
                        .entity(tfid)
                        .is_some_and(|t| t.effects.iter().any(|te| te.item_id == eff.item_id))
                    {
                        continue;
                    }
                }

                // Re-apply same effect id with original parameters (aoe=1).
                let ceff = crate::fight::ChipEffect {
                    id: eff.id,
                    value1: eff.value1,
                    value2: eff.value2,
                    turns: eff.base_turns,
                    targets: 0,
                    modifiers: eff.modifiers,
                    r#type: 0,
                };
                let cell = self.entity(tfid).map(|e| e.cell).unwrap_or(-1);
                crate::fight::apply_effects_on_cells(
                    self,
                    eff.caster_fid,
                    &[cell],
                    &[ceff],
                    crate::fight::EffectContext {
                        critical: eff.critical,
                        result_code: 1,
                        jet,
                        attack_type: 2,
                        item_id: eff.item_id,
                    },
                    1,
                    cell,
                );
            }
        }
    }

    pub fn remove_poisons(&mut self, target_fid: i32) {
        let mut removed: Vec<ActiveEffect> = Vec::new();
        {
            let Some(e) = self.entity_mut(target_fid) else {
                return;
            };
            if e.dead {
                return;
            }
            for i in (0..e.effects.len()).rev() {
                // Official generator: `Effect.TYPE_POISON` = 13
                if e.effects[i].id == 13 {
                    removed.push(e.effects.swap_remove(i));
                }
            }
        }
        for eff in removed {
            self.remove_launched_effect_log(eff.caster_fid, eff.log_id);
            self.log_action(serde_json::json!([303, eff.log_id]));
        }
    }

    pub fn remove_shackles(&mut self, target_fid: i32) {
        let mut removed: Vec<ActiveEffect> = Vec::new();
        {
            let Some(e) = self.entity_mut(target_fid) else {
                return;
            };
            if e.dead {
                return;
            }
            for i in (0..e.effects.len()).rev() {
                // Official generator: shackle ids: MP=17, TP=18, STR=19, MAG=24, AGI=47, WIS=48
                if matches!(e.effects[i].id, 17 | 18 | 19 | 24 | 47 | 48) {
                    // revert stat deltas if any
                    if let Some(key) = e.effects[i].stat_key {
                        apply_stat_delta(e, key, -e.effects[i].value);
                    }
                    removed.push(e.effects.swap_remove(i));
                }
            }
        }
        for eff in removed {
            self.remove_launched_effect_log(eff.caster_fid, eff.log_id);
            self.log_action(serde_json::json!([303, eff.log_id]));
        }
    }

    pub fn is_obstacle_cell(&self, cell: i32) -> bool {
        self.obstacles.contains_key(&cell)
    }

    pub fn entity_mut(&mut self, fid: i32) -> Option<&mut SimEntity> {
        self.entities.get_mut(fid as usize)
    }

    pub fn entity(&self, fid: i32) -> Option<&SimEntity> {
        self.entities.get(fid as usize)
    }

    /// First living entity standing on `cell`, excluding `except_fid` if set (`Map.getEntity`-style).
    pub fn living_entity_on_cell(&self, cell: i32, except_fid: Option<i32>) -> Option<i32> {
        self.entities.iter().find_map(|e| {
            if e.dead || e.cell != cell {
                return None;
            }
            if except_fid == Some(e.fid) {
                return None;
            }
            Some(e.fid)
        })
    }

    pub fn alive_team_count(&self) -> usize {
        let mut alive = vec![false; self.team_fids.len()];
        for e in &self.entities {
            if !e.dead {
                alive[e.team as usize] = true;
            }
        }
        alive.into_iter().filter(|&x| x).count()
    }

    pub fn compute_winner(&self) -> i32 {
        let alive_teams = self.alive_team_count();
        if alive_teams == 1 {
            for e in &self.entities {
                if !e.dead {
                    return e.team;
                }
            }
            return -1;
        }
        if !self.draw_check_life || self.team_fids.len() < 2 {
            return -1;
        }
        // Official generator: `Fight.computeWinner`: `mWinteam == -1 && drawCheckLife` compares team 0 vs 1 total life.
        let life = |team_idx: usize| -> i32 {
            self.team_fids[team_idx]
                .iter()
                .filter_map(|&fid| self.entity(fid))
                .map(|e| e.life)
                .sum()
        };
        let l0 = life(0);
        let l1 = life(1);
        if l0 > l1 {
            0
        } else if l1 > l0 {
            1
        } else {
            -1
        }
    }

    pub fn is_finished(&self) -> bool {
        self.alive_team_count() <= 1
    }

    /// Official generator: `State.resurrect`: re-insert turn order, apply `Entity.resurrect`, set cell, log `[Action.RESURRECT, ...]`.
    pub fn apply_resurrection(
        &mut self,
        caster_fid: i32,
        target_fid: i32,
        cell: i32,
        critical: bool,
        full_life: bool,
    ) {
        const ACTION_RESURRECT: i32 = 105;

        self.insert_resurrected_in_turn_order(target_fid);
        let factor = if critical { 1.3 } else { 1.0 };
        if let Some(e) = self.entity_mut(target_fid) {
            if full_life {
                e.life = e.total_life;
            } else {
                let new_total = ((e.total_life as f64) * 0.5 * factor).round() as i32;
                e.total_life = new_total.max(10);
                e.life = e.total_life / 2;
            }
            e.dead = false;
            e.cell = cell;
        }
        // Official generator: `Entity.resurrect` calls `endTurn()` while already alive (life > 0).
        self.end_turn(target_fid);

        let (life, max_life) = self
            .entity(target_fid)
            .map(|e| (e.life, e.total_life))
            .unwrap_or((0, 0));
        self.log_action(serde_json::json!([
            ACTION_RESURRECT,
            caster_fid,
            target_fid,
            cell,
            life,
            max_life
        ]));
    }

    /// Official generator: `State.resurrectEntity`: invulnerability after cooldown when template is chip **415**.
    pub fn grant_awakening_invulnerability(
        &mut self,
        caster_fid: i32,
        target_fid: i32,
        critical: bool,
    ) {
        const EFFECT_ADD_STATE: i32 = 59;
        const CHIP_AWEKENING: i32 = 415;
        const MOD_IRREDUCTIBLE: i32 = 16;

        let log_id = self.alloc_effect_log_id();
        let sid = 3i32; // Official generator: `EntityState.INVINCIBLE`
        let turns = -1;
        self.log_action(serde_json::json!([
            302,
            CHIP_AWEKENING,
            log_id,
            caster_fid,
            target_fid,
            EFFECT_ADD_STATE,
            sid,
            turns,
            MOD_IRREDUCTIBLE
        ]));
        self.apply_effect_stat_delta(
            target_fid,
            ActiveEffect {
                id: EFFECT_ADD_STATE,
                caster_fid,
                item_id: CHIP_AWEKENING,
                log_id,
                modifiers: MOD_IRREDUCTIBLE,
                propagate: 0,
                propagate_modifiers: 0,
                base_turns: turns,
                value1: sid as f64,
                value2: 0.0,
                critical,
                state_id: Some(sid),
                value: sid,
                turns,
                stat_key: None,
            },
        );
    }

    /// Insert `resurrected_fid` before the next living leek in `initial_fids`, else append (Official generator `Order.addEntity`).
    fn insert_resurrected_in_turn_order(&mut self, resurrected_fid: i32) {
        self.turn_fids.retain(|&f| f != resurrected_fid);
        let insert_idx =
            if let Some(pos) = self.initial_fids.iter().position(|&f| f == resurrected_fid) {
                let mut next_fid: Option<i32> = None;
                for &f in self.initial_fids.iter().skip(pos + 1) {
                    if self.entity(f).is_some_and(|e| !e.dead) {
                        next_fid = Some(f);
                        break;
                    }
                }
                if let Some(next) = next_fid {
                    self.turn_fids
                        .iter()
                        .position(|&f| f == next)
                        .unwrap_or(self.turn_fids.len())
                } else {
                    self.turn_fids.len()
                }
            } else {
                self.turn_fids.len()
            };
        self.turn_fids.insert(insert_idx, resurrected_fid);
    }
}

#[cfg(test)]
mod initial_cooldown_tests {
    use super::*;
    use crate::fight::chips::ChipStats;

    fn stub_entity(fid: i32, team: i32) -> SimEntity {
        SimEntity {
            fid,
            team,
            team_id: 1,
            leek_id: fid,
            level: 1,
            name: String::new(),
            cell: 0,
            spawn_cell: 0,
            life: 100,
            total_life: 100,
            strength: 0,
            agility: 0,
            wisdom: 0,
            resistance: 0,
            science: 0,
            magic: 0,
            power: 0,
            absolute_shield: 0,
            relative_shield: 0,
            damage_return: 0,
            tp: 10,
            mp: 10,
            total_tp: 10,
            total_mp: 10,
            frequency: 1,
            entity_type: 0,
            skin: 0,
            hat: 0,
            metal: false,
            face: 0,
            weapons: Vec::new(),
            chips: Vec::new(),
            components: Vec::new(),
            equipped_weapon: None,
            ai_path: String::new(),
            ai_id: 0,
            ai_name: String::new(),
            ai_version: 0,
            ai_strict: false,
            birth_turn: 1,
            cores: 0,
            ram: 0,
            farmer_id: 0,
            farmer_name: String::new(),
            farmer_country: "?".into(),
            log_bucket_owner: 0,
            team_name: String::new(),
            dead: false,
            effects: Vec::new(),
            launched_effect_log_ids: Vec::new(),
            is_summon: false,
            summoner_fid: None,
            chip_cooldowns: HashMap::new(),
            chip_uses_turn: HashMap::new(),
            says_turn: 0,
        }
    }

    fn chip_with_initial(id: i32, initial: i32, team_cd: bool) -> ChipStats {
        ChipStats {
            chip_id: id,
            name: String::new(),
            template_id: 0,
            cost: 0,
            min_range: 0,
            max_range: 0,
            los: false,
            launch_type: 0,
            area: 0,
            effects: Vec::new(),
            cooldown: 0,
            team_cooldown: team_cd,
            initial_cooldown: initial,
            max_uses: -1,
        }
    }

    #[test]
    fn initial_entity_cooldown_is_initial_plus_one() {
        let mut entities = vec![stub_entity(0, 0), stub_entity(1, 1)];
        let mut team_cd: Vec<HashMap<i32, i32>> = vec![HashMap::new(), HashMap::new()];
        let mut chips = HashMap::new();
        chips.insert(42, chip_with_initial(42, 2, false));
        FightWorld::apply_initial_chip_cooldowns(&chips, &mut entities, &mut team_cd);
        assert_eq!(entities[0].chip_cooldowns.get(&42), Some(&3));
        assert_eq!(entities[1].chip_cooldowns.get(&42), Some(&3));
        assert!(team_cd[0].is_empty() && team_cd[1].is_empty());
    }

    #[test]
    fn initial_team_cooldown_goes_to_team_map_once_per_team() {
        let mut entities = vec![stub_entity(0, 0), stub_entity(1, 0)];
        let mut team_cd: Vec<HashMap<i32, i32>> = vec![HashMap::new()];
        let mut chips = HashMap::new();
        chips.insert(7, chip_with_initial(7, 1, true));
        FightWorld::apply_initial_chip_cooldowns(&chips, &mut entities, &mut team_cd);
        assert!(entities[0].chip_cooldowns.is_empty());
        assert!(entities[1].chip_cooldowns.is_empty());
        assert_eq!(team_cd[0].get(&7), Some(&2));
    }

    #[test]
    fn resurrect_reorders_turn_fids_before_next_alive_in_initial_order() {
        let a = stub_entity(0, 0);
        let mut b = stub_entity(1, 0);
        let c = stub_entity(2, 0);
        b.dead = true;
        let mut w = FightWorld {
            entities: vec![a, b, c],
            team_fids: vec![vec![0, 1, 2]],
            // Like official generator `Order` after death: dead leek removed from play order.
            turn_fids: vec![0, 2],
            initial_fids: vec![0, 1, 2],
            active_fid: 0,
            active_turn: 1,
            max_turns: 64,
            seed: 1,
            rng: JavaCompatRng::new(1),
            map_w: 17,
            map_h: 17,
            obstacles: std::collections::BTreeMap::new(),
            outcome_obstacles_json: None,
            map_type: 0,
            fight_id: 0,
            fight_type: 0,
            fight_context: 0,
            fight_boss: 0,
            fight_start_unix_secs: 0,
            max_operations_per_entity: None,
            chips_by_id: HashMap::new(),
            summons_by_id: HashMap::new(),
            weapons_by_item: HashMap::new(),
            draw_check_life: false,
            actions: Vec::new(),
            entity_ops_totals: HashMap::new(),
            next_effect_log_id: 1,
            team_chip_cooldowns: vec![HashMap::new()],
            registers: HashMap::new(),
            inbox: HashMap::new(),
            say_inbox: HashMap::new(),
            farmer_ai_logs: HashMap::new(),
            trace: None,
        };
        w.apply_resurrection(0, 1, 5, false, false);
        assert_eq!(w.turn_fids, vec![0, 1, 2]);
        assert!(!w.entity(1).unwrap().dead);
        assert_eq!(w.entity(1).unwrap().cell, 5);
    }
}
