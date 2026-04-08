use serde_json::json;

use super::state::{LeekWarsEntity, LeekWarsState};
use super::types::EffectType;

pub(crate) fn reduce_effects(
    st: &mut LeekWarsState,
    target_id: i64,
    percent: f64,
    skip_irreducible: bool,
) {
    let Some(t) = st.entities.get_mut(&target_id) else {
        return;
    };
    let reduction = (1.0 - percent).clamp(0.0, 1.0);
    let mut removed: Vec<i64> = Vec::new();
    for ef in &mut t.effects {
        if skip_irreducible && (ef.modifiers & 16) != 0 {
            continue;
        }
        let next = ((ef.value as f64) * reduction).round() as i64;
        ef.value = next;
        if ef.value <= 0 {
            removed.push(ef.instance_id);
        } else {
            // UPDATE_EFFECT updates effect value: [304, id, value]
            st.fight_actions.push(json!([304, ef.instance_id, ef.value]));
        }
    }
    if !removed.is_empty() {
        t.effects.retain(|e| !removed.contains(&e.instance_id));
        for id in removed {
            st.fight_actions.push(json!([303, id]));
        }
    }
    recompute_derived_buffs(t);
}

pub(crate) fn clear_poisons(st: &mut LeekWarsState, target_id: i64) {
    let Some(t) = st.entities.get_mut(&target_id) else {
        return;
    };
    let removed: Vec<i64> = t
        .effects
        .iter()
        .filter(|e| e.turns_left > 0 && e.effect_id == EffectType::Poison)
        .map(|e| e.instance_id)
        .collect();
    if removed.is_empty() {
        return;
    }
    t.effects.retain(|e| !removed.contains(&e.instance_id));
    recompute_derived_buffs(t);
}

pub(crate) fn remove_shackles(st: &mut LeekWarsState, target_id: i64) {
    let Some(t) = st.entities.get_mut(&target_id) else {
        return;
    };
    let removed: Vec<i64> = t
        .effects
        .iter()
        .filter(|e| {
            e.turns_left > 0
                && matches!(
                    e.effect_id,
                    EffectType::ShackleMp | EffectType::ShackleTp | EffectType::ShackleStrength
                )
        })
        .map(|e| e.instance_id)
        .collect();
    if removed.is_empty() {
        return;
    }
    t.effects.retain(|e| !removed.contains(&e.instance_id));
    recompute_derived_buffs(t);
}

pub(crate) fn cell_blocked(st: &LeekWarsState, cell: i64, ignore_entity: Option<i64>) -> bool {
    let Ok(cid) = i32::try_from(cell) else {
        return true;
    };
    let Some(c) = st.map.get_cell(cid) else {
        return true;
    };
    if !c.walkable {
        return true;
    }
    st.entities
        .values()
        .any(|e| e.life > 0 && Some(e.id) != ignore_entity && e.cell == cell)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlideMode {
    Push,
    Attract,
}

pub(crate) fn slide_toward_target_with_checks(
    st: &LeekWarsState,
    entity_cell: i64,
    target_cell: i64,
    caster_cell: i64,
    mode: SlideMode,
    ignore_entity: Option<i64>,
) -> i64 {
    let Ok(eid) = i32::try_from(entity_cell) else {
        return entity_cell;
    };
    let Ok(tid) = i32::try_from(target_cell) else {
        return entity_cell;
    };
    let Ok(cid) = i32::try_from(caster_cell) else {
        return entity_cell;
    };
    let Some(ec) = st.map.get_cell(eid) else {
        return entity_cell;
    };
    let Some(tc) = st.map.get_cell(tid) else {
        return entity_cell;
    };
    let Some(cc) = st.map.get_cell(cid) else {
        return entity_cell;
    };
    let (ex, ey) = (ec.x, ec.y);
    let (tx, ty) = (tc.x, tc.y);
    let (cx, cy) = (cc.x, cc.y);

    let cdx = (ex - cx).signum();
    let cdy = (ey - cy).signum();
    let dx = (tx - ex).signum();
    let dy = (ty - ey).signum();
    let ok_dir = match mode {
        SlideMode::Push => cdx == dx && cdy == dy,
        SlideMode::Attract => cdx == -dx && cdy == -dy,
    };
    if !ok_dir {
        return entity_cell;
    }

    let mut cur = eid;
    while cur != tid {
        let Some(c0) = st.map.get_cell(cur) else {
            break;
        };
        let Some(next) = st.map.get_cell_xy(c0.x + dx, c0.y + dy) else {
            break;
        };
        if cell_blocked(st, next as i64, ignore_entity) {
            return cur as i64;
        }
        cur = next;
    }
    cur as i64
}

pub(crate) fn slide_away_until_blocked(
    st: &LeekWarsState,
    start: i64,
    away_from: i64,
    ignore_entity: Option<i64>,
) -> i64 {
    let Ok(aid) = i32::try_from(away_from) else {
        return start;
    };
    let Some(ac) = st.map.get_cell(aid) else {
        return start;
    };
    let (ax, ay) = (ac.x, ac.y);
    let Ok(mut cur) = i32::try_from(start) else {
        return start;
    };
    loop {
        let Some(cc) = st.map.get_cell(cur) else {
            break;
        };
        let (cx, cy) = (cc.x, cc.y);
        let dx = (cx - ax).signum();
        let dy = (cy - ay).signum();
        if dx == 0 && dy == 0 {
            break;
        }
        let Some(next) = st.map.get_cell_xy(cx + dx, cy + dy) else {
            break;
        };
        if cell_blocked(st, next as i64, ignore_entity) {
            break;
        }
        cur = next;
    }
    cur as i64
}

pub(crate) fn slide_entity(st: &mut LeekWarsState, entity_id: i64, dest: i64) {
    let Some(e) = st.entities.get_mut(&entity_id) else {
        return;
    };
    if e.life <= 0 {
        return;
    }
    recompute_derived_buffs(e);
    if e.state_static || dest == e.cell {
        return;
    }
    e.cell = dest;
}

pub(crate) fn tick_effects_start_turn(st: &mut LeekWarsState) {
    // Start-of-turn effects: poison + heal-over-time.
    let ids: Vec<i64> = st.entities.keys().copied().collect();
    for id in ids {
        let Some(ent) = st.entities.get_mut(&id) else { continue };
        if ent.life <= 0 {
            continue;
        }
        let mut remove: Vec<i64> = Vec::new();
        // We'll snapshot derived buffs to avoid borrow conflicts while iterating mutably.
        let (shield_abs, shield_rel_percent) = {
            recompute_derived_buffs(ent);
            (ent.shield_abs, ent.shield_rel_percent)
        };
        // Iterate by index and avoid holding a mutable borrow of the effect across damage application.
        for idx in 0..ent.effects.len() {
            let (instance_id, effect_id, caster, value, turns_left) = {
                let ef = &ent.effects[idx];
                (
                    ef.instance_id,
                    ef.effect_id,
                    ef.caster,
                    ef.value,
                    ef.turns_left,
                )
            };
            if turns_left <= 0 {
                remove.push(instance_id);
                continue;
            }
            if effect_id == EffectType::Poison {
                let dmg = value.max(0);
                if dmg > 0 && ent.life > 0 {
                    if ent.state_invincible {
                        // Invincible: no damage.
                        // Still tick duration below.
                    } else {
                        let dealt =
                            apply_damage_with_shields_with(ent, dmg, shield_abs, shield_rel_percent);
                        if dealt > 0 {
                            let erosion = (dealt as f64 * 0.10).round() as i64;
                            apply_erosion(ent, erosion);
                            st.fight_actions.push(json!([110, id, dealt, erosion]));
                        }
                        if ent.life == 0 {
                            // Log kill; killer is caster id when known.
                            st.fight_actions.push(json!([11, caster, id]));
                            st.fight_actions.push(json!([5, id, caster]));
                        }
                    }
                }
            } else if effect_id == EffectType::Heal {
                let heal = value.max(0);
                if heal > 0 && ent.life > 0 {
                    if ent.state_unhealable {
                        // UNHEALABLE: no healing.
                    } else {
                        let before = ent.life;
                        ent.life = ent.life.saturating_add(heal).min(ent.total_life.max(0));
                        let applied = ent.life - before;
                        if applied > 0 {
                            st.fight_actions.push(json!([103, id, applied]));
                        }
                    }
                }
            }
            // Update turns_left in place (clients can decrement locally; generator doesn't emit per-turn updates).
            if let Some(ef) = ent.effects.get_mut(idx) {
                ef.turns_left -= 1;
                if ef.turns_left <= 0 {
                    remove.push(instance_id);
                }
            }
        }
        if !remove.is_empty() {
            ent.effects.retain(|e| !remove.contains(&e.instance_id));
            for rid in remove {
                // REMOVE_EFFECT shape [303, effect_instance_id]
                st.fight_actions.push(json!([303, rid]));
            }
        }
    }
}

pub(crate) fn recompute_derived_buffs(ent: &mut LeekWarsEntity) {
    ent.shield_abs = 0;
    ent.shield_rel_percent = 0;
    ent.strength_bonus = 0;
    ent.mp_bonus = 0;
    ent.tp_bonus = 0;
    ent.damage_return = 0;
    ent.state_unhealable = false;
    ent.state_invincible = false;
    ent.state_static = false;
    for ef in &ent.effects {
        if ef.turns_left <= 0 {
            continue;
        }
        match ef.effect_id {
            // These are additive stats; we keep it additive and clamp where needed.
            EffectType::RelativeShield => {
                ent.shield_rel_percent = ent.shield_rel_percent.saturating_add(ef.value);
            }
            EffectType::Vulnerability => {
                ent.shield_rel_percent = ent
                    .shield_rel_percent
                    .saturating_sub(ef.value.max(0));
            }
            EffectType::AbsoluteShield => {
                ent.shield_abs = ent.shield_abs.saturating_add(ef.value.max(0));
            }
            EffectType::BuffStrength => {
                ent.strength_bonus = ent.strength_bonus.saturating_add(ef.value);
            }
            EffectType::ShackleMp => {
                ent.mp_bonus = ent.mp_bonus.saturating_sub(ef.value.max(0));
            }
            EffectType::ShackleTp => {
                ent.tp_bonus = ent.tp_bonus.saturating_sub(ef.value.max(0));
            }
            EffectType::DamageReturn => {
                ent.damage_return = ent.damage_return.saturating_add(ef.value.max(0));
            }
            EffectType::AddState => {
                // EntityState ordinals: UNHEALABLE=2, INVINCIBLE=3
                if ef.value == 2 {
                    ent.state_unhealable = true;
                } else if ef.value == 3 {
                    ent.state_invincible = true;
                } else if ef.value == 11 {
                    // STATIC
                    ent.state_static = true;
                }
            }
            _ => {}
        }
    }
    ent.shield_rel_percent = ent.shield_rel_percent.clamp(0, 100);
}

pub(crate) fn apply_damage_with_shields(ent: &mut LeekWarsEntity, incoming: i64) -> i64 {
    let mut dmg = incoming.max(0);
    if dmg == 0 {
        return 0;
    }
    if ent.shield_rel_percent > 0 {
        let p = ent.shield_rel_percent.clamp(0, 100);
        dmg = dmg.saturating_mul(100 - p) / 100;
    }
    if ent.shield_abs > 0 {
        dmg = (dmg - ent.shield_abs).max(0);
    }
    if dmg > 0 {
        ent.life = (ent.life - dmg).max(0);
    }
    dmg
}

pub(crate) fn apply_erosion(ent: &mut LeekWarsEntity, erosion: i64) {
    if erosion <= 0 {
        return;
    }
    ent.total_life = (ent.total_life - erosion).max(0);
    if ent.life > ent.total_life {
        ent.life = ent.total_life;
    }
}

pub(crate) fn apply_damage_with_shields_with(
    ent: &mut LeekWarsEntity,
    incoming: i64,
    shield_abs: i64,
    shield_rel_percent: i64,
) -> i64 {
    let mut dmg = incoming.max(0);
    if dmg == 0 {
        return 0;
    }
    if shield_rel_percent > 0 {
        let p = shield_rel_percent.clamp(0, 100);
        dmg = dmg.saturating_mul(100 - p) / 100;
    }
    if shield_abs > 0 {
        dmg = (dmg - shield_abs).max(0);
    }
    if dmg > 0 {
        ent.life = (ent.life - dmg).max(0);
    }
    dmg
}

pub(crate) fn tick_chip_cooldowns(st: &mut LeekWarsState) {
    for e in st.entities.values_mut() {
        for v in e.chip_cooldowns.values_mut() {
            if *v > 0 {
                *v -= 1;
            }
        }
        // Reset per-turn item use counters.
        e.item_uses.clear();
    }
    for v in st.team_chip_cooldowns.values_mut() {
        if *v > 0 {
            *v -= 1;
        }
    }
}

pub fn tick_effects_start_turn_public(st: &mut LeekWarsState) {
    tick_effects_start_turn(st);
}

pub fn tick_chip_cooldowns_public(st: &mut LeekWarsState) {
    tick_chip_cooldowns(st);
}

