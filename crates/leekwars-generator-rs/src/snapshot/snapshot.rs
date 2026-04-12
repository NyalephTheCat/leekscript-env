use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectSnapshot {
    pub instance_id: i64,
    pub item_id: i64,
    pub caster: i64,
    pub target: i64,
    pub effect_id: i64,
    pub value: i64,
    pub turns_left: i64,
    pub modifiers: i64,
    pub from_weapon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub id: i64,
    pub team: i64,
    pub cell: i64,
    pub life: i64,
    pub tp: i64,
    pub mp: i64,
    pub equipped_weapon: Option<i64>,
    #[serde(default)]
    pub effects: Vec<EffectSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FightSnapshot {
    pub action_index: usize,
    pub turn: i64,
    pub active_entity: Option<i64>,
    pub entities: BTreeMap<i64, EntitySnapshot>,
    pub dead: BTreeSet<i64>,
}

/// Replay `fight.actions` up to `action_index` (inclusive) and return a reconstructed state.
pub fn snapshot_at_action_index(
    outcome_fight: &Value,
    action_index: usize,
) -> miette::Result<FightSnapshot> {
    let leeks = outcome_fight
        .get("leeks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| miette::miette!("fight.leeks missing/invalid"))?;
    let actions = outcome_fight
        .get("actions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| miette::miette!("fight.actions missing/invalid"))?;

    let mut entities: BTreeMap<i64, EntitySnapshot> = BTreeMap::new();
    for l in leeks {
        let Some(o) = l.as_object() else { continue };
        let id = o.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        if id == 0 {
            continue;
        }
        let team = o.get("team").and_then(|v| v.as_i64()).unwrap_or(0);
        let cell = o.get("cellPos").and_then(|v| v.as_i64()).unwrap_or(0);
        let life = o.get("life").and_then(|v| v.as_i64()).unwrap_or(0);
        let tp = o.get("tp").and_then(|v| v.as_i64()).unwrap_or(0);
        let mp = o.get("mp").and_then(|v| v.as_i64()).unwrap_or(0);
        entities.insert(
            id,
            EntitySnapshot {
                id,
                team,
                cell,
                life,
                tp,
                mp,
                equipped_weapon: None,
                effects: Vec::new(),
            },
        );
    }

    let mut snap = FightSnapshot {
        action_index,
        turn: 0,
        active_entity: None,
        entities,
        dead: BTreeSet::new(),
    };

    let max_i = action_index.min(actions.len().saturating_sub(1));
    for i in 0..=max_i {
        let a = &actions[i];
        let Some(arr) = a.as_array() else { continue };
        let Some(op) = arr.get(0).and_then(|v| v.as_i64()) else {
            continue;
        };
        match op {
            6 => {
                // NEW_TURN [6, turn]
                snap.turn = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(snap.turn);
            }
            7 => {
                // LEEK_TURN [7, entity_id]
                let id = arr.get(1).and_then(|v| v.as_i64());
                snap.active_entity = id;
            }
            8 => {
                // END_TURN [8, entity_id, tp, mp]
                let id = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                let tp = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
                let mp = arr.get(3).and_then(|v| v.as_i64()).unwrap_or(0);
                if let Some(e) = snap.entities.get_mut(&id) {
                    e.tp = tp;
                    e.mp = mp;
                }
            }
            9 => {
                // SUMMON [9, owner, target, cell, success]
                let tid = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
                let cell = arr.get(3).and_then(|v| v.as_i64()).unwrap_or(0);
                let ok = arr.get(4).and_then(|v| v.as_i64()).unwrap_or(0);
                if ok == 1 && tid != 0 {
                    snap.entities.insert(
                        tid,
                        EntitySnapshot {
                            id: tid,
                            team: snap
                                .active_entity
                                .and_then(|aid| snap.entities.get(&aid).map(|e| e.team))
                                .unwrap_or(0),
                            cell,
                            life: 1,
                            tp: 0,
                            mp: 0,
                            equipped_weapon: None,
                            effects: Vec::new(),
                        },
                    );
                }
            }
            10 => {
                // MOVE [10, entity_id, end_cell, path]
                let id = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                let cell = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
                if let Some(e) = snap.entities.get_mut(&id) {
                    e.cell = cell;
                }
            }
            11 => {
                // KILL? [11, killer_id, target_id]
                let tid = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
                if let Some(e) = snap.entities.get_mut(&tid) {
                    e.life = 0;
                }
                if tid != 0 {
                    snap.dead.insert(tid);
                }
            }
            12 => {
                // USE_CHIP [12, chip_template, cell, success]
                // No direct state change unless effects are separately logged; ignore.
            }
            13 => {
                // SET_WEAPON [13, weapon_template]
                // The action doesn't include the entity id; apply to current active entity.
                let w = arr.get(1).and_then(|v| v.as_i64());
                if let (Some(id), Some(w)) = (snap.active_entity, w) {
                    if let Some(e) = snap.entities.get_mut(&id) {
                        e.equipped_weapon = Some(w);
                    }
                }
            }
            101 => {
                // DAMAGE [101, target_id, dmg, ???]
                let tid = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                let dmg = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
                if let Some(e) = snap.entities.get_mut(&tid) {
                    e.life = (e.life - dmg).max(0);
                    if e.life == 0 {
                        snap.dead.insert(tid);
                    }
                }
            }
            103 => {
                // HEAL [103, target_id, amount]
                let tid = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                let heal = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
                if let Some(e) = snap.entities.get_mut(&tid) {
                    e.life = e.life.saturating_add(heal);
                    if e.life > 0 {
                        snap.dead.remove(&tid);
                    }
                }
            }
            105 => {
                // RESURRECT [105, target_id]
                let tid = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                if let Some(e) = snap.entities.get_mut(&tid) {
                    if e.life == 0 {
                        e.life = 1;
                    }
                    snap.dead.remove(&tid);
                }
            }
            110 => {
                // POISON_DAMAGE [110, target_id, amount, secondary]
                let tid = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                let dmg = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
                if let Some(e) = snap.entities.get_mut(&tid) {
                    e.life = (e.life - dmg).max(0);
                    if e.life == 0 {
                        snap.dead.insert(tid);
                    }
                }
            }
            301 | 302 => {
                // ADD_WEAPON_EFFECT / ADD_CHIP_EFFECT
                // [opcode, itemID, id, caster, target, effectID, value, turns, modifiers?]
                let item_id = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                let instance_id = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
                let caster = arr.get(3).and_then(|v| v.as_i64()).unwrap_or(0);
                let target = arr.get(4).and_then(|v| v.as_i64()).unwrap_or(0);
                let effect_id = arr.get(5).and_then(|v| v.as_i64()).unwrap_or(0);
                let value = arr.get(6).and_then(|v| v.as_i64()).unwrap_or(0);
                let turns_left = arr.get(7).and_then(|v| v.as_i64()).unwrap_or(0);
                let modifiers = arr.get(8).and_then(|v| v.as_i64()).unwrap_or(0);
                if instance_id != 0 && target != 0 {
                    if let Some(e) = snap.entities.get_mut(&target) {
                        e.effects.retain(|ef| ef.instance_id != instance_id);
                        e.effects.push(EffectSnapshot {
                            instance_id,
                            item_id,
                            caster,
                            target,
                            effect_id,
                            value,
                            turns_left,
                            modifiers,
                            from_weapon: op == 301,
                        });
                    }
                }
            }
            303 => {
                // REMOVE_EFFECT [303, effect_instance_id]
                let instance_id = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                if instance_id != 0 {
                    for e in snap.entities.values_mut() {
                        e.effects.retain(|ef| ef.instance_id != instance_id);
                    }
                }
            }
            304 => {
                // UPDATE_EFFECT [304, effect_instance_id, value]
                let instance_id = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                let value = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
                if instance_id != 0 {
                    for e in snap.entities.values_mut() {
                        if let Some(ef) = e
                            .effects
                            .iter_mut()
                            .find(|ef| ef.instance_id == instance_id)
                        {
                            ef.value = value;
                        }
                    }
                }
            }
            14 => {
                // STACK_EFFECT [14, effect_instance_id, delta_value]
                let instance_id = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                let delta = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(0);
                if instance_id != 0 && delta != 0 {
                    for e in snap.entities.values_mut() {
                        if let Some(ef) = e
                            .effects
                            .iter_mut()
                            .find(|ef| ef.instance_id == instance_id)
                        {
                            ef.value = ef.value.saturating_add(delta);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(snap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn snapshot_replays_move_and_weapon_and_end_turn() {
        let fight = json!({
            "leeks": [
                {"id": 1, "team": 1, "cellPos": 10, "life": 20, "tp": 5, "mp": 4},
                {"id": 2, "team": 2, "cellPos": 20, "life": 30, "tp": 5, "mp": 4}
            ],
            "map": {},
            "actions": [
                [0],
                [6, 1],
                [7, 1],
                [10, 1, 15, [10, 15]],
                [13, 37],
                [8, 1, 2, 1]
            ],
            "dead": {},
            "ops": {}
        });
        let snap = snapshot_at_action_index(&fight, 5).unwrap();
        assert_eq!(snap.turn, 1);
        assert_eq!(snap.entities.get(&1).unwrap().cell, 15);
        assert_eq!(snap.entities.get(&1).unwrap().equipped_weapon, Some(37));
        assert_eq!(snap.entities.get(&1).unwrap().tp, 2);
        assert_eq!(snap.entities.get(&1).unwrap().mp, 1);
    }

    #[test]
    fn snapshot_replays_poison_damage_and_resurrect() {
        let fight = json!({
            "leeks": [
                {"id": 1, "team": 1, "cellPos": 10, "life": 10, "tp": 5, "mp": 4}
            ],
            "map": {},
            "actions": [
                [6, 1],
                [110, 1, 3, 0],
                [11, 2, 1],
                [5, 1, 2],
                [105, 1]
            ],
            "dead": {},
            "ops": {}
        });
        let snap = snapshot_at_action_index(&fight, 4).unwrap();
        assert_eq!(snap.entities.get(&1).unwrap().life, 1);
        assert!(!snap.dead.contains(&1));
    }
}
