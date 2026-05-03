use crate::fight::map;
use crate::fight::{pathfinding, ChipEffect, FightWorld};
use serde_json::json;

#[derive(Debug, Clone, Copy)]
pub struct EffectContext {
    pub critical: bool,
    pub result_code: i32, // Official generator: Attack.USE_SUCCESS=1 / USE_CRITICAL=2
    pub jet: f64,         // Official generator: `Attack.applyOnCell` jet = random double
    pub attack_type: i32, // 1=weapon, 2=chip (official generator `Attack.TYPE_*`)
    pub item_id: i32,     // Attack.getItemId()
}

// Official generator: action ids (subset we currently emit).
const ACTION_PLAYER_DEAD: i32 = 5;
const ACTION_KILL: i32 = 11;
const ACTION_LOST_LIFE: i32 = 101;
const ACTION_POISON_DAMAGE: i32 = 110;
const ACTION_DAMAGE_RETURN: i32 = 108;
const ACTION_HEAL: i32 = 103;
const ACTION_VITALITY: i32 = 104;
const ACTION_AFTEREFFECT: i32 = 111;
const ACTION_NOVA_DAMAGE: i32 = 107;
const ACTION_REDUCE_EFFECTS: i32 = 306;
const ACTION_REMOVE_POISONS: i32 = 307;
const ACTION_REMOVE_SHACKLES: i32 = 308;

// Official generator: `Effect.TYPE_*` ids (subset).
const EFFECT_DAMAGE: i32 = 1;
const EFFECT_HEAL: i32 = 2;
const EFFECT_RELATIVE_SHIELD: i32 = 5;
const EFFECT_ABSOLUTE_SHIELD: i32 = 6;
const EFFECT_VITALITY: i32 = 12;
const EFFECT_POISON: i32 = 13;
const EFFECT_DAMAGE_RETURN: i32 = 20;
const EFFECT_STEAL_LIFE: i32 = 61;
const EFFECT_KILL: i32 = 16;
const EFFECT_TELEPORT: i32 = 10;
const EFFECT_SUMMON: i32 = 14;
const EFFECT_RESURRECT: i32 = 15;
const EFFECT_ATTRACT: i32 = 46;
const EFFECT_PUSH: i32 = 51;
const EFFECT_DEBUFF: i32 = 9;
const EFFECT_ANTIDOTE: i32 = 23;
const EFFECT_REMOVE_SHACKLES: i32 = 49;
const EFFECT_AFTEREFFECT: i32 = 25;
const EFFECT_ADD_STATE: i32 = 59;
const EFFECT_TOTAL_DEBUFF: i32 = 60;
// raw buffs/shields/heal
const EFFECT_RAW_BUFF_MP: i32 = 31;
const EFFECT_RAW_BUFF_TP: i32 = 32;
const EFFECT_RAW_ABSOLUTE_SHIELD: i32 = 37;
const EFFECT_RAW_BUFF_STRENGTH: i32 = 38;
const EFFECT_RAW_BUFF_MAGIC: i32 = 39;
const EFFECT_RAW_BUFF_SCIENCE: i32 = 40;
const EFFECT_RAW_BUFF_AGILITY: i32 = 41;
const EFFECT_RAW_BUFF_RESISTANCE: i32 = 42;
const EFFECT_RAW_BUFF_WISDOM: i32 = 44;
const EFFECT_RAW_RELATIVE_SHIELD: i32 = 54;
const EFFECT_RAW_HEAL: i32 = 57;

// buffs/shackles
const EFFECT_BUFF_STRENGTH: i32 = 3;
const EFFECT_BUFF_AGILITY: i32 = 4;
const EFFECT_BUFF_MP: i32 = 7;
const EFFECT_BUFF_TP: i32 = 8;
const EFFECT_BUFF_RESISTANCE: i32 = 21;
const EFFECT_BUFF_WISDOM: i32 = 22;
const EFFECT_SHACKLE_MP: i32 = 17;
const EFFECT_SHACKLE_TP: i32 = 18;
const EFFECT_SHACKLE_STRENGTH: i32 = 19;
const EFFECT_SHACKLE_MAGIC: i32 = 24;
const EFFECT_SHACKLE_AGILITY: i32 = 47;
const EFFECT_SHACKLE_WISDOM: i32 = 48;

fn critical_power(ctx: EffectContext) -> f64 {
    if ctx.critical {
        1.3
    } else {
        1.0
    }
}

fn caster_mult(stat: i32) -> f64 {
    1.0 + f64::from(stat) / 100.0
}

fn ctx_attack_is_chip(ctx: EffectContext) -> bool {
    ctx.attack_type == 2
}

fn has_state(w: &FightWorld, fid: i32, state_id: i32) -> bool {
    w.entity(fid)
        .is_some_and(|e| e.effects.iter().any(|ef| ef.state_id == Some(state_id)))
}

fn filter_target(
    targets: i32,
    caster: &crate::fight::SimEntity,
    target: &crate::fight::SimEntity,
) -> bool {
    // Official generator: Attack.filterTarget (bitmask in Effect.TARGET_*)
    if (targets & 1) == 0 && caster.team != target.team {
        return false;
    }
    if (targets & 2) == 0 && caster.team == target.team {
        return false;
    }
    if (targets & 4) == 0 && caster.fid == target.fid {
        return false;
    }
    if (targets & 8) == 0 && !target.is_summon {
        return false;
    }
    if (targets & 16) == 0 && target.is_summon {
        return false;
    }
    true
}

/// Official generator: `Attack.getPowerForCell`: `1 - dist * 0.2`, except line / first-in-line / all enemies / all allies (full power).
fn aoe_multiplier(w: &FightWorld, area_shape: i32, center_cell: i32, entity_cell: i32) -> f64 {
    match area_shape {
        2 | 13 | 14 | 15 => 1.0,
        _ => {
            if center_cell < 0 || !map::is_valid_cell(w.map_w, w.map_h, center_cell) {
                return 1.0;
            }
            let d = map::case_distance(w.map_w, center_cell, entity_cell);
            (1.0 - 0.2 * f64::from(d)).max(0.0)
        }
    }
}

fn effect_target_count(
    w: &FightWorld,
    caster_fid: i32,
    target_cells: &[i32],
    targets_mask: i32,
    modifiers: i32,
) -> i32 {
    // Official generator: targetCount is number of filtered targets when MODIFIER_MULTIPLIED_BY_TARGETS is set, else 1.
    if (modifiers & 2) == 0 {
        return 1;
    }
    let Some(caster) = w.entity(caster_fid) else {
        return 1;
    };
    let on_caster = (modifiers & 4) != 0;
    let mut uniq: Vec<i32> = Vec::new();
    for &cell in target_cells {
        if let Some(tfid) = w.living_entity_on_cell(cell, None) {
            if uniq.contains(&tfid) {
                continue;
            }
            let Some(t) = w.entity(tfid) else { continue };
            if t.dead {
                continue;
            }
            if !filter_target(targets_mask, caster, t) {
                continue;
            }
            if on_caster && tfid == caster_fid {
                continue;
            }
            uniq.push(tfid);
        }
    }
    uniq.len().max(1) as i32
}

fn apply_stat_delta_local(e: &mut crate::fight::SimEntity, key: i32, delta: i32) {
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

struct ReversibleStatEffectArgs {
    caster_fid: i32,
    target_fid: i32,
    turns: i32,
    key: i32,
    delta: i32,
    effect_id: i32,
    modifiers: i32,
    propagate: i32,
    propagate_modifiers: i32,
    value1: f64,
    value2: f64,
    ctx: EffectContext,
}

fn apply_reversible_stat_effect(w: &mut FightWorld, args: ReversibleStatEffectArgs) {
    let ReversibleStatEffectArgs {
        caster_fid,
        target_fid,
        turns,
        key,
        delta,
        effect_id,
        modifiers,
        propagate,
        propagate_modifiers,
        value1,
        value2,
        ctx,
    } = args;
    if delta == 0 {
        return;
    }

    // Not-replaceable: if any effect from the same item already exists, skip.
    if (modifiers & 8) != 0
        && w.entity(target_fid)
            .is_some_and(|e| e.effects.iter().any(|ef| ef.item_id == ctx.item_id))
    {
        return;
    }

    let stackable = (modifiers & 1) != 0;
    if turns != 0 {
        if !stackable {
            // Remove previous effect of same type + same item id.
            if let Some(t) = w.entity_mut(target_fid) {
                if let Some(pos) = t
                    .effects
                    .iter()
                    .position(|ef| ef.id == effect_id && ef.item_id == ctx.item_id)
                {
                    let old = t.effects.remove(pos);
                    if let Some(k) = old.stat_key {
                        apply_stat_delta_local(t, k, -old.value);
                    }
                    w.remove_launched_effect_log(old.caster_fid, old.log_id);
                    w.log_action(json!([303, old.log_id]));
                }
            }
        }
        // Stack to previous item with same characteristics (same effect id, item id, turns, caster)
        let mut stack_log: Option<i32> = None;
        {
            if let Some(t) = w.entity_mut(target_fid) {
                let idx = t.effects.iter().position(|ef| {
                    ef.id == effect_id
                        && ef.item_id == ctx.item_id
                        && ef.turns == turns
                        && ef.caster_fid == caster_fid
                });
                if let Some(i) = idx {
                    // Update stat by delta and value by delta.
                    if let Some(k) = t.effects[i].stat_key {
                        apply_stat_delta_local(t, k, delta);
                    }
                    t.effects[i].value += delta;
                    stack_log = Some(t.effects[i].log_id);
                }
            }
        }
        if let Some(log_id) = stack_log {
            // Official generator: ActionStackEffect: [14, logID, addedValue]
            w.log_action(json!([14, log_id, delta]));
            return;
        }
    }

    let log_id = w.alloc_effect_log_id();
    let add_code = if ctx_attack_is_chip(ctx) { 302 } else { 301 };
    // Include modifiers only when non-zero (official generator behavior).
    if modifiers != 0 {
        w.log_action(json!([
            add_code,
            ctx.item_id,
            log_id,
            caster_fid,
            target_fid,
            effect_id,
            delta,
            turns,
            modifiers
        ]));
    } else {
        w.log_action(json!([
            add_code,
            ctx.item_id,
            log_id,
            caster_fid,
            target_fid,
            effect_id,
            delta,
            turns
        ]));
    }
    w.apply_effect_stat_delta(
        target_fid,
        crate::fight::ActiveEffect {
            id: effect_id,
            caster_fid,
            item_id: ctx.item_id,
            log_id,
            modifiers,
            propagate,
            propagate_modifiers,
            base_turns: turns,
            value1,
            value2,
            critical: ctx.critical,
            state_id: None,
            value: delta,
            turns,
            stat_key: Some(key),
        },
    );
}

const ACTION_MOVE_TO: i32 = 10;

fn is_available_cell(w: &FightWorld, cell: i32, entity_to_ignore: Option<i32>) -> bool {
    if !map::is_valid_cell(w.map_w, w.map_h, cell) {
        return false;
    }
    if w.is_obstacle_cell(cell) {
        return false;
    }
    w.living_entity_on_cell(cell, entity_to_ignore).is_none()
}

fn slide_entity(w: &mut FightWorld, caster_fid: i32, entity_fid: i32, dest: i32) {
    let Some(start) = w.entity(entity_fid).map(|e| e.cell) else {
        return;
    };
    if start == dest {
        return;
    }
    if !is_available_cell(w, dest, Some(entity_fid)) {
        return;
    }
    // Official generator: logs ActionMove for slide paths; it uses A* path with [start,dest] ignored.
    let ignore = [start, dest];
    let path =
        pathfinding::get_path_between(w, start, dest, Some(&ignore)).unwrap_or_else(|| vec![dest]);
    if let Some(e) = w.entity_mut(entity_fid) {
        e.cell = dest;
    }
    w.log_action(json!([ACTION_MOVE_TO, entity_fid, dest, path]));
    // Passive hooks like `onMoved` are not implemented yet.
    let _ = caster_fid;
}

fn teleport_entity(w: &mut FightWorld, entity_fid: i32, dest: i32) {
    if !is_available_cell(w, dest, Some(entity_fid)) {
        return;
    }
    if let Some(e) = w.entity_mut(entity_fid) {
        e.cell = dest;
    }
    // Official generator: does not log a dedicated teleport action here.
}

fn push_last_available_cell(
    w: &FightWorld,
    entity_cell: i32,
    target_cell: i32,
    caster_cell: i32,
    entity_fid: i32,
) -> i32 {
    let mw = w.map_w;
    let (ex, ey) = map::cell_xy(mw, entity_cell);
    let (tx, ty) = map::cell_xy(mw, target_cell);
    let (cx, cy) = map::cell_xy(mw, caster_cell);
    let cdx = (ex - cx).signum();
    let cdy = (ey - cy).signum();
    let dx = (tx - ex).signum();
    let dy = (ty - ey).signum();
    if cdx != dx || cdy != dy {
        return entity_cell;
    }
    let mut current = entity_cell;
    while current != target_cell {
        let (ux, uy) = map::cell_xy(mw, current);
        let next = map::cell_id_from_xy(mw, ux + dx, uy + dy);
        if !is_available_cell(w, next, Some(entity_fid)) {
            return current;
        }
        current = next;
    }
    current
}

fn attract_last_available_cell(
    w: &FightWorld,
    entity_cell: i32,
    target_cell: i32,
    caster_cell: i32,
    entity_fid: i32,
) -> i32 {
    let mw = w.map_w;
    let (ex, ey) = map::cell_xy(mw, entity_cell);
    let (tx, ty) = map::cell_xy(mw, target_cell);
    let (cx, cy) = map::cell_xy(mw, caster_cell);
    let cdx = (ex - cx).signum();
    let cdy = (ey - cy).signum();
    let dx = (tx - ex).signum();
    let dy = (ty - ey).signum();
    if cdx != -dx || cdy != -dy {
        return entity_cell;
    }
    let mut current = entity_cell;
    while current != target_cell {
        let (ux, uy) = map::cell_xy(mw, current);
        let next = map::cell_id_from_xy(mw, ux + dx, uy + dy);
        if !is_available_cell(w, next, Some(entity_fid)) {
            return current;
        }
        current = next;
    }
    current
}

fn apply_damage_with_shields(
    w: &mut FightWorld,
    caster_fid: i32,
    target_fid: i32,
    mut dmg: i32,
    log_action: i32,
) -> (i32, i32, bool) {
    if dmg <= 0 {
        return (0, 0, false);
    }
    // Official generator: invincible targets take no damage (EntityState.INVINCIBLE = 3)
    if has_state(w, target_fid, 3) {
        return (0, 0, false);
    }
    // absolute shield: flat reduction, consumed
    let mut died = false;
    let mut absorbed = 0;
    let mut erosion = 0;
    let mut should_log_nova = false;
    {
        let Some(victim) = w.entity_mut(target_fid) else {
            return (0, 0, false);
        };
        if victim.dead {
            return (0, 0, false);
        }
        if victim.absolute_shield > 0 {
            let take = victim.absolute_shield.min(dmg);
            victim.absolute_shield -= take;
            dmg -= take;
            absorbed += take;
        }
        if dmg > 0 && victim.relative_shield > 0 {
            let rs = f64::from(victim.relative_shield.max(0)) / 100.0;
            let reduced = (f64::from(dmg) * (1.0 - rs)).round() as i32;
            dmg = reduced.max(0);
        }
        if dmg > 0 {
            victim.life = (victim.life - dmg).max(0);
            // Official generator: erosion: poison uses 0.10, most other damage uses 0.05. We'll approximate by action code.
            let rate = if log_action == ACTION_POISON_DAMAGE {
                0.10
            } else {
                0.05
            };
            erosion = (f64::from(dmg) * rate).round() as i32;
            if erosion > 0 {
                victim.total_life = (victim.total_life - erosion).max(1);
                should_log_nova = true;
            }
            if victim.life == 0 {
                victim.dead = true;
                died = true;
            }
        }
    }
    if should_log_nova {
        w.log_action(json!([ACTION_NOVA_DAMAGE, target_fid, erosion, 0]));
    }
    // keep existing log format stable: include absorbed in the 4th slot for LOST_LIFE for now
    if dmg > 0 || absorbed > 0 {
        if log_action == ACTION_LOST_LIFE {
            w.log_action(json!([ACTION_LOST_LIFE, target_fid, dmg, absorbed]));
        } else {
            // ActionDamage-style: [code, target, pv, erosion]
            w.log_action(json!([log_action, target_fid, dmg, erosion]));
        }
    }
    if died {
        w.log_action(json!([ACTION_PLAYER_DEAD, target_fid, caster_fid]));
        w.cleanup_on_death(target_fid);
    }
    (dmg, absorbed, died)
}

pub fn apply_effects_on_cells(
    w: &mut FightWorld,
    caster_fid: i32,
    target_cells: &[i32],
    effects: &[ChipEffect],
    ctx: EffectContext,
    area_shape: i32,
    aoe_center_cell: i32,
) {
    // Official generator: order: effect-centric loop with `previousEffectTotalValue` chaining.
    let mut target_fids: Vec<i32> = Vec::new();
    for &cell in target_cells {
        if let Some(tfid) = w.living_entity_on_cell(cell, None) {
            if !target_fids.contains(&tfid) {
                target_fids.push(tfid);
            }
        }
    }

    let mut propagate = 0i32;
    let mut propagate_modifiers = 0i32;
    for eff in effects {
        if eff.id == 43 {
            propagate = eff.value1 as i32;
            propagate_modifiers = eff.modifiers;
            break;
        }
    }

    let caster_cell = w.entity(caster_fid).map_or(-999, |e| e.cell);
    let mut previous_effect_total_value: i32 = 0;

    for eff in effects {
        if eff.id == 43 {
            continue;
        }

        // Official generator: `Attack.applyOnCell`: push/attract/teleport run on all entities in the area, not `filterTarget`.
        if eff.id == EFFECT_ATTRACT {
            let focus = target_cells.first().copied().unwrap_or(caster_cell);
            for &tfid in &target_fids {
                if w.entity(tfid).is_none_or(|e| e.dead) {
                    continue;
                }
                let tcell = w.entity(tfid).map_or(-1, |e| e.cell);
                let dest = attract_last_available_cell(w, tcell, focus, caster_cell, tfid);
                slide_entity(w, caster_fid, tfid, dest);
            }
            continue;
        }
        if eff.id == EFFECT_PUSH {
            let focus = target_cells.first().copied().unwrap_or(caster_cell);
            for &tfid in &target_fids {
                if w.entity(tfid).is_none_or(|e| e.dead) {
                    continue;
                }
                let tcell = w.entity(tfid).map_or(-1, |e| e.cell);
                let dest = push_last_available_cell(w, tcell, focus, caster_cell, tfid);
                slide_entity(w, caster_fid, tfid, dest);
            }
            continue;
        }
        if eff.id == EFFECT_TELEPORT {
            if let Some(&dest) = target_cells.first() {
                teleport_entity(w, caster_fid, dest);
            }
            continue;
        }

        let modifiers = eff.modifiers;
        let on_caster = (modifiers & 4) != 0;
        let multiplied = (modifiers & 2) != 0;
        let not_replaceable = (modifiers & 8) != 0;

        let target_count = effect_target_count(w, caster_fid, target_cells, eff.targets, modifiers);
        let mut effect_total_value: i32 = 0;

        let mut effect_targets: Vec<i32> = Vec::new();
        if on_caster {
            if w.entity(caster_fid).is_some_and(|e| !e.dead) {
                effect_targets.push(caster_fid);
            }
        } else {
            let Some(caster) = w.entity(caster_fid) else {
                continue;
            };
            for &tfid in &target_fids {
                let Some(target) = w.entity(tfid) else {
                    continue;
                };
                if target.dead {
                    continue;
                }
                if !filter_target(eff.targets, caster, target) {
                    continue;
                }
                if not_replaceable && target.effects.iter().any(|te| te.item_id == ctx.item_id) {
                    continue;
                }
                effect_targets.push(tfid);
            }
        }

        for tfid in effect_targets {
            let tcell = w.entity(tfid).map_or(-999, |e| e.cell);
            let ap = if on_caster {
                1.0
            } else {
                aoe_multiplier(w, area_shape, aoe_center_cell, tcell)
            };

            match eff.id {
                EFFECT_DAMAGE => {
                    let caster_str = w.entity(caster_fid).map_or(0, |e| e.strength).max(0);
                    let base = ((eff.value1 + ctx.jet * eff.value2)
                        * (1.0 + f64::from(caster_str) / 100.0)
                        * critical_power(ctx)
                        * ap
                        * (if multiplied {
                            f64::from(target_count)
                        } else {
                            1.0
                        }))
                    .round() as i32;
                    if base <= 0 {
                        continue;
                    }

                    let dmg_return_pct = w.entity(tfid).map_or(0, |e| e.damage_return);
                    let return_dmg = if tfid != caster_fid && dmg_return_pct > 0 {
                        (f64::from(base) * f64::from(dmg_return_pct) / 100.0).round() as i32
                    } else {
                        0
                    };

                    let (done, _abs, _died) =
                        apply_damage_with_shields(w, caster_fid, tfid, base, ACTION_LOST_LIFE);
                    effect_total_value += done;

                    if done > 0 && tfid != caster_fid && !has_state(w, caster_fid, 2) {
                        let wis = w.entity(caster_fid).map_or(0, |e| e.wisdom);
                        let mut steal = (f64::from(done) * f64::from(wis) / 1000.0).round() as i32;
                        if steal > 0 {
                            if let Some(caster_ent) = w.entity_mut(caster_fid) {
                                if !caster_ent.dead && caster_ent.life < caster_ent.total_life {
                                    steal = steal.min(caster_ent.total_life - caster_ent.life);
                                    if steal > 0 {
                                        caster_ent.life += steal;
                                        w.log_action(json!([ACTION_HEAL, caster_fid, steal]));
                                    }
                                }
                            }
                        }
                    }

                    if return_dmg > 0 && tfid != caster_fid && !has_state(w, caster_fid, 3) {
                        apply_damage_with_shields(
                            w,
                            tfid,
                            caster_fid,
                            return_dmg,
                            ACTION_DAMAGE_RETURN,
                        );
                    }
                }
                EFFECT_HEAL => {
                    if has_state(w, tfid, 2) {
                        continue;
                    }
                    let caster_wis = w.entity(caster_fid).map_or(0, |e| e.wisdom);
                    let per = ((eff.value1 + ctx.jet * eff.value2)
                        * (1.0 + f64::from(caster_wis) / 100.0)
                        * critical_power(ctx)
                        * ap
                        * (if multiplied {
                            f64::from(target_count)
                        } else {
                            1.0
                        }))
                    .round() as i32;
                    if per <= 0 {
                        continue;
                    }
                    if w.entity(tfid).is_some_and(|v| v.dead) {
                        continue;
                    }
                    if eff.turns == 0 {
                        let mut heal = per;
                        if let Some(v) = w.entity_mut(tfid) {
                            heal = heal.min(v.total_life - v.life);
                            if heal > 0 {
                                v.life += heal;
                                w.log_action(json!([ACTION_HEAL, tfid, heal]));
                                effect_total_value += heal;
                            }
                        }
                    } else {
                        let log_id = w.alloc_effect_log_id();
                        let add_code = if ctx_attack_is_chip(ctx) { 302 } else { 301 };
                        if eff.modifiers != 0 {
                            w.log_action(json!([
                                add_code,
                                ctx.item_id,
                                log_id,
                                caster_fid,
                                tfid,
                                EFFECT_HEAL,
                                per,
                                eff.turns,
                                eff.modifiers
                            ]));
                        } else {
                            w.log_action(json!([
                                add_code,
                                ctx.item_id,
                                log_id,
                                caster_fid,
                                tfid,
                                EFFECT_HEAL,
                                per,
                                eff.turns
                            ]));
                        }
                        w.apply_effect_stat_delta(
                            tfid,
                            crate::fight::ActiveEffect {
                                id: EFFECT_HEAL,
                                caster_fid,
                                item_id: ctx.item_id,
                                log_id,
                                modifiers: eff.modifiers,
                                propagate,
                                propagate_modifiers,
                                base_turns: eff.turns,
                                value1: eff.value1,
                                value2: eff.value2,
                                critical: ctx.critical,
                                state_id: None,
                                value: per,
                                turns: eff.turns,
                                stat_key: None,
                            },
                        );
                        effect_total_value += per;
                    }
                }
                EFFECT_STEAL_LIFE => {
                    if has_state(w, tfid, 2) {
                        continue;
                    }
                    let mut heal = previous_effect_total_value.max(0);
                    if heal <= 0 {
                        continue;
                    }
                    if let Some(v) = w.entity_mut(tfid) {
                        if v.dead {
                            continue;
                        }
                        heal = heal.min(v.total_life - v.life);
                        if heal > 0 {
                            v.life += heal;
                            w.log_action(json!([ACTION_HEAL, tfid, heal]));
                            effect_total_value += heal;
                        }
                    }
                }
                EFFECT_RELATIVE_SHIELD => {
                    let caster_res = w.entity(caster_fid).map_or(0, |e| e.resistance);
                    let add = ((eff.value1 + eff.value2 * ctx.jet)
                        * caster_mult(caster_res)
                        * critical_power(ctx)
                        * ap)
                        .round() as i32;
                    if add > 0 {
                        apply_reversible_stat_effect(
                            w,
                            ReversibleStatEffectArgs {
                                caster_fid,
                                target_fid: tfid,
                                turns: eff.turns,
                                key: 11,
                                delta: add,
                                effect_id: EFFECT_RELATIVE_SHIELD,
                                modifiers: eff.modifiers,
                                propagate,
                                propagate_modifiers,
                                value1: eff.value1,
                                value2: eff.value2,
                                ctx,
                            },
                        );
                        effect_total_value += add;
                    }
                }
                EFFECT_ABSOLUTE_SHIELD => {
                    let caster_res = w.entity(caster_fid).map_or(0, |e| e.resistance);
                    let add = ((eff.value1 + eff.value2 * ctx.jet)
                        * caster_mult(caster_res)
                        * critical_power(ctx)
                        * ap)
                        .round() as i32;
                    if add > 0 {
                        apply_reversible_stat_effect(
                            w,
                            ReversibleStatEffectArgs {
                                caster_fid,
                                target_fid: tfid,
                                turns: eff.turns,
                                key: 10,
                                delta: add,
                                effect_id: EFFECT_ABSOLUTE_SHIELD,
                                modifiers: eff.modifiers,
                                propagate,
                                propagate_modifiers,
                                value1: eff.value1,
                                value2: eff.value2,
                                ctx,
                            },
                        );
                        effect_total_value += add;
                    }
                }
                EFFECT_POISON => {
                    let turns = eff.turns;
                    if turns == 0 {
                        continue;
                    }
                    let mag = w.entity(caster_fid).map_or(0, |e| e.magic).max(0);
                    let per_turn = ((eff.value1 + eff.value2 * ctx.jet)
                        * (1.0 + f64::from(mag) / 100.0)
                        * ap
                        * critical_power(ctx))
                    .round() as i32;
                    if per_turn <= 0 {
                        continue;
                    }
                    if w.entity(tfid).is_some_and(|v| v.dead) {
                        continue;
                    }
                    let log_id = w.alloc_effect_log_id();
                    let add_code = if ctx_attack_is_chip(ctx) { 302 } else { 301 };
                    if eff.modifiers != 0 {
                        w.log_action(json!([
                            add_code,
                            ctx.item_id,
                            log_id,
                            caster_fid,
                            tfid,
                            EFFECT_POISON,
                            per_turn,
                            turns,
                            eff.modifiers
                        ]));
                    } else {
                        w.log_action(json!([
                            add_code,
                            ctx.item_id,
                            log_id,
                            caster_fid,
                            tfid,
                            EFFECT_POISON,
                            per_turn,
                            turns
                        ]));
                    }
                    w.apply_effect_stat_delta(
                        tfid,
                        crate::fight::ActiveEffect {
                            id: EFFECT_POISON,
                            caster_fid,
                            item_id: ctx.item_id,
                            log_id,
                            modifiers: eff.modifiers,
                            propagate,
                            propagate_modifiers,
                            base_turns: turns,
                            value1: eff.value1,
                            value2: eff.value2,
                            critical: ctx.critical,
                            state_id: None,
                            value: per_turn,
                            turns,
                            stat_key: None,
                        },
                    );
                    effect_total_value += per_turn;
                }
                EFFECT_VITALITY => {
                    let caster_wis = w.entity(caster_fid).map_or(0, |e| e.wisdom);
                    let add = ((eff.value1 + eff.value2 * ctx.jet)
                        * (1.0 + f64::from(caster_wis) / 100.0)
                        * ap
                        * critical_power(ctx))
                    .round()
                    .max(0.0) as i32;
                    if add <= 0 {
                        continue;
                    }
                    if let Some(v) = w.entity_mut(tfid) {
                        if v.dead {
                            continue;
                        }
                        v.total_life += add;
                        v.life += add;
                        w.log_action(json!([ACTION_VITALITY, tfid, add]));
                        effect_total_value += add;
                    }
                }
                EFFECT_DAMAGE_RETURN => {
                    let agi = w.entity(caster_fid).map_or(0, |e| e.agility);
                    let add = ((eff.value1 + eff.value2 * ctx.jet)
                        * (1.0 + f64::from(agi) / 100.0)
                        * ap
                        * critical_power(ctx))
                    .round() as i32;
                    if add > 0 {
                        apply_reversible_stat_effect(
                            w,
                            ReversibleStatEffectArgs {
                                caster_fid,
                                target_fid: tfid,
                                turns: eff.turns,
                                key: 9,
                                delta: add,
                                effect_id: EFFECT_DAMAGE_RETURN,
                                modifiers: eff.modifiers,
                                propagate,
                                propagate_modifiers,
                                value1: eff.value1,
                                value2: eff.value2,
                                ctx,
                            },
                        );
                        effect_total_value += add;
                    }
                }
                EFFECT_DEBUFF => {
                    let v = ((eff.value1 + eff.value2 * ctx.jet)
                        * ap
                        * critical_power(ctx)
                        * (if multiplied {
                            f64::from(target_count)
                        } else {
                            1.0
                        }))
                    .round() as i32;
                    if v > 0 {
                        w.reduce_effects(tfid, f64::from(v) / 100.0);
                        w.log_action(json!([ACTION_REDUCE_EFFECTS, tfid, v]));
                        effect_total_value += v;
                    }
                }
                EFFECT_TOTAL_DEBUFF => {
                    let v = ((eff.value1 + eff.value2 * ctx.jet)
                        * ap
                        * critical_power(ctx)
                        * (if multiplied {
                            f64::from(target_count)
                        } else {
                            1.0
                        }))
                    .round() as i32;
                    if v > 0 {
                        w.reduce_effects_total(tfid, f64::from(v) / 100.0);
                        w.log_action(json!([ACTION_REDUCE_EFFECTS, tfid, v]));
                        effect_total_value += v;
                    }
                }
                EFFECT_ANTIDOTE => {
                    w.remove_poisons(tfid);
                    w.log_action(json!([ACTION_REMOVE_POISONS, tfid]));
                }
                EFFECT_REMOVE_SHACKLES => {
                    w.remove_shackles(tfid);
                    w.log_action(json!([ACTION_REMOVE_SHACKLES, tfid]));
                }
                EFFECT_KILL => {
                    if has_state(w, tfid, 3) {
                        continue;
                    }
                    let life = w.entity(tfid).map_or(0, |e| e.life);
                    if life <= 0 || w.entity(tfid).is_none_or(|e| e.dead) {
                        continue;
                    }
                    // Match official generator `ActionKill` JSON (both ids are the target due to a generator bug).
                    w.log_action(json!([ACTION_KILL, tfid, tfid]));
                    let (done, _, _) =
                        apply_damage_with_shields(w, caster_fid, tfid, life, ACTION_LOST_LIFE);
                    effect_total_value += done;
                }
                EFFECT_AFTEREFFECT => {
                    let sci = w.entity(caster_fid).map_or(0, |e| e.science);
                    let mut dmg = ((eff.value1 + eff.value2 * ctx.jet)
                        * (1.0 + f64::from(sci) / 100.0)
                        * ap
                        * critical_power(ctx))
                    .round() as i32;
                    dmg = dmg.max(0);
                    if has_state(w, tfid, 3) {
                        dmg = 0;
                    }
                    if let Some(v) = w.entity(tfid) {
                        if !v.dead {
                            dmg = dmg.min(v.life);
                        }
                    }
                    if dmg > 0 {
                        apply_damage_with_shields(w, caster_fid, tfid, dmg, ACTION_AFTEREFFECT);
                        effect_total_value += dmg;
                    }
                    if eff.turns != 0 && dmg > 0 && w.entity(tfid).is_some_and(|e| !e.dead) {
                        let log_id = w.alloc_effect_log_id();
                        let add_code = if ctx_attack_is_chip(ctx) { 302 } else { 301 };
                        if eff.modifiers != 0 {
                            w.log_action(json!([
                                add_code,
                                ctx.item_id,
                                log_id,
                                caster_fid,
                                tfid,
                                EFFECT_AFTEREFFECT,
                                dmg,
                                eff.turns,
                                eff.modifiers
                            ]));
                        } else {
                            w.log_action(json!([
                                add_code,
                                ctx.item_id,
                                log_id,
                                caster_fid,
                                tfid,
                                EFFECT_AFTEREFFECT,
                                dmg,
                                eff.turns
                            ]));
                        }
                        w.apply_effect_stat_delta(
                            tfid,
                            crate::fight::ActiveEffect {
                                id: EFFECT_AFTEREFFECT,
                                caster_fid,
                                item_id: ctx.item_id,
                                log_id,
                                modifiers: eff.modifiers,
                                propagate,
                                propagate_modifiers,
                                base_turns: eff.turns,
                                value1: eff.value1,
                                value2: eff.value2,
                                critical: ctx.critical,
                                state_id: None,
                                value: dmg,
                                turns: eff.turns,
                                stat_key: None,
                            },
                        );
                    }
                }
                EFFECT_ADD_STATE => {
                    let sid = eff.value1 as i32;
                    if sid < 0 || w.entity(tfid).is_none_or(|e| e.dead) {
                        continue;
                    }
                    if eff.turns == 0 {
                        continue;
                    }
                    let log_id = w.alloc_effect_log_id();
                    let add_code = if ctx_attack_is_chip(ctx) { 302 } else { 301 };
                    if eff.modifiers != 0 {
                        w.log_action(json!([
                            add_code,
                            ctx.item_id,
                            log_id,
                            caster_fid,
                            tfid,
                            EFFECT_ADD_STATE,
                            sid,
                            eff.turns,
                            eff.modifiers
                        ]));
                    } else {
                        w.log_action(json!([
                            add_code,
                            ctx.item_id,
                            log_id,
                            caster_fid,
                            tfid,
                            EFFECT_ADD_STATE,
                            sid,
                            eff.turns
                        ]));
                    }
                    w.apply_effect_stat_delta(
                        tfid,
                        crate::fight::ActiveEffect {
                            id: EFFECT_ADD_STATE,
                            caster_fid,
                            item_id: ctx.item_id,
                            log_id,
                            modifiers: eff.modifiers,
                            propagate,
                            propagate_modifiers,
                            base_turns: eff.turns,
                            value1: eff.value1,
                            value2: eff.value2,
                            critical: ctx.critical,
                            state_id: Some(sid),
                            value: sid,
                            turns: eff.turns,
                            stat_key: None,
                        },
                    );
                    effect_total_value += sid.max(0);
                }
                EFFECT_BUFF_STRENGTH
                | EFFECT_BUFF_AGILITY
                | EFFECT_BUFF_WISDOM
                | EFFECT_BUFF_RESISTANCE
                | EFFECT_BUFF_MP
                | EFFECT_BUFF_TP => {
                    let sci = w.entity(caster_fid).map_or(0, |e| e.science);
                    let add = ((eff.value1 + eff.value2 * ctx.jet)
                        * (1.0 + f64::from(sci) / 100.0)
                        * ap
                        * critical_power(ctx))
                    .round() as i32;
                    let key = match eff.id {
                        EFFECT_BUFF_STRENGTH => 1,
                        EFFECT_BUFF_AGILITY => 2,
                        EFFECT_BUFF_WISDOM => 3,
                        EFFECT_BUFF_RESISTANCE => 4,
                        EFFECT_BUFF_MP => 8,
                        EFFECT_BUFF_TP => 7,
                        _ => continue,
                    };
                    if add > 0 {
                        apply_reversible_stat_effect(
                            w,
                            ReversibleStatEffectArgs {
                                caster_fid,
                                target_fid: tfid,
                                turns: eff.turns,
                                key,
                                delta: add,
                                effect_id: eff.id,
                                modifiers: eff.modifiers,
                                propagate,
                                propagate_modifiers,
                                value1: eff.value1,
                                value2: eff.value2,
                                ctx,
                            },
                        );
                        effect_total_value += add;
                    }
                }
                EFFECT_SHACKLE_STRENGTH
                | EFFECT_SHACKLE_MAGIC
                | EFFECT_SHACKLE_AGILITY
                | EFFECT_SHACKLE_WISDOM
                | EFFECT_SHACKLE_MP
                | EFFECT_SHACKLE_TP => {
                    let mag = w.entity(caster_fid).map_or(0, |e| e.magic).max(0);
                    let sub = ((eff.value1 + eff.value2 * ctx.jet)
                        * (1.0 + f64::from(mag) / 100.0)
                        * ap
                        * critical_power(ctx))
                    .round() as i32;
                    let key = match eff.id {
                        EFFECT_SHACKLE_STRENGTH => 1,
                        EFFECT_SHACKLE_MAGIC => 6,
                        EFFECT_SHACKLE_AGILITY => 2,
                        EFFECT_SHACKLE_WISDOM => 3,
                        EFFECT_SHACKLE_MP => 8,
                        EFFECT_SHACKLE_TP => 7,
                        _ => continue,
                    };
                    if sub > 0 {
                        apply_reversible_stat_effect(
                            w,
                            ReversibleStatEffectArgs {
                                caster_fid,
                                target_fid: tfid,
                                turns: eff.turns,
                                key,
                                delta: -sub,
                                effect_id: eff.id,
                                modifiers: eff.modifiers,
                                propagate,
                                propagate_modifiers,
                                value1: eff.value1,
                                value2: eff.value2,
                                ctx,
                            },
                        );
                        effect_total_value += sub;
                    }
                }
                EFFECT_RAW_BUFF_STRENGTH
                | EFFECT_RAW_BUFF_MAGIC
                | EFFECT_RAW_BUFF_SCIENCE
                | EFFECT_RAW_BUFF_AGILITY
                | EFFECT_RAW_BUFF_RESISTANCE
                | EFFECT_RAW_BUFF_WISDOM
                | EFFECT_RAW_BUFF_MP
                | EFFECT_RAW_BUFF_TP
                | EFFECT_RAW_ABSOLUTE_SHIELD
                | EFFECT_RAW_RELATIVE_SHIELD => {
                    let add = ((eff.value1 + eff.value2 * ctx.jet) * ap * critical_power(ctx))
                        .round() as i32;
                    let key = match eff.id {
                        EFFECT_RAW_BUFF_STRENGTH => 1,
                        EFFECT_RAW_BUFF_MAGIC => 6,
                        EFFECT_RAW_BUFF_SCIENCE => 5,
                        EFFECT_RAW_BUFF_AGILITY => 2,
                        EFFECT_RAW_BUFF_RESISTANCE => 4,
                        EFFECT_RAW_BUFF_WISDOM => 3,
                        EFFECT_RAW_BUFF_MP => 8,
                        EFFECT_RAW_BUFF_TP => 7,
                        EFFECT_RAW_ABSOLUTE_SHIELD => 10,
                        EFFECT_RAW_RELATIVE_SHIELD => 11,
                        _ => continue,
                    };
                    if add != 0 {
                        if let Some(ent) = w.entity_mut(tfid) {
                            if ent.dead {
                                continue;
                            }
                            apply_stat_delta_local(ent, key, add);
                            effect_total_value += add;
                        }
                    }
                }
                EFFECT_RAW_HEAL => {
                    if has_state(w, tfid, 2) {
                        continue;
                    }
                    let mut heal = ((eff.value1 + eff.value2 * ctx.jet)
                        * ap
                        * critical_power(ctx)
                        * (if multiplied {
                            f64::from(target_count)
                        } else {
                            1.0
                        }))
                    .round() as i32;
                    if heal <= 0 {
                        continue;
                    }
                    if let Some(v) = w.entity_mut(tfid) {
                        if v.dead {
                            continue;
                        }
                        heal = heal.min(v.total_life - v.life);
                        if heal > 0 {
                            v.life += heal;
                            w.log_action(json!([ACTION_HEAL, tfid, heal]));
                            effect_total_value += heal;
                        }
                    }
                }
                EFFECT_SUMMON | EFFECT_RESURRECT => {
                    // Official generator: handles summon chips via `Fight.summonEntity`, not `EffectSummon.apply`.
                }
                _ => {}
            }
        }

        previous_effect_total_value = effect_total_value;
    }
}

pub fn apply_start_turn_effects(w: &mut FightWorld, fid: i32) {
    let Some(effects) = w.entity(fid).map(|e| e.effects.clone()) else {
        return;
    };
    for eff in effects {
        if eff.id == EFFECT_POISON {
            let dmg;
            {
                let Some(victim) = w.entity_mut(fid) else {
                    continue;
                };
                if victim.dead {
                    continue;
                }
                dmg = eff.value.min(victim.life).max(0);
            }
            apply_damage_with_shields(w, eff.caster_fid, fid, dmg, ACTION_POISON_DAMAGE);
        }
        if eff.id == EFFECT_AFTEREFFECT {
            let dmg = eff.value.min(w.entity(fid).map_or(0, |e| e.life)).max(0);
            apply_damage_with_shields(w, eff.caster_fid, fid, dmg, ACTION_AFTEREFFECT);
        }
        if eff.id == EFFECT_HEAL {
            if has_state(w, fid, 2) {
                continue;
            }
            let mut heal = eff.value;
            if let Some(v) = w.entity_mut(fid) {
                if v.dead {
                    continue;
                }
                heal = heal.min(v.total_life - v.life);
                if heal > 0 {
                    v.life += heal;
                    w.log_action(json!([ACTION_HEAL, fid, heal]));
                }
            }
        }
    }
}
