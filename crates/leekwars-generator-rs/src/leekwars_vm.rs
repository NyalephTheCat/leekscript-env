use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use leekscript::vm::{NativeFn, Value, VmError};
use serde_json::json;

use crate::world_map::WorldMap;
use crate::registers::{RegisterManagerRc, Registers};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i64)]
pub enum LaunchType {
    Line = 1,
    Diagonal = 2,
    Star = 3,
    StarInverted = 4,
    DiagonalInverted = 5,
    LineInverted = 6,
    Circle = 7,
}

impl LaunchType {
    #[must_use]
    pub fn from_i64(v: i64) -> Option<Self> {
        Some(match v {
            1 => Self::Line,
            2 => Self::Diagonal,
            3 => Self::Star,
            4 => Self::StarInverted,
            5 => Self::DiagonalInverted,
            6 => Self::LineInverted,
            7 => Self::Circle,
            _ => return None,
        })
    }

    #[must_use]
    pub fn allows(self, dx: i32, dy: i32) -> bool {
        let dx = dx.abs();
        let dy = dy.abs();
        let is_line = dx == 0 || dy == 0;
        let is_diag = dx == dy;
        match self {
            Self::Line => is_line,
            Self::Diagonal => is_diag,
            Self::Star => is_line || is_diag,
            Self::StarInverted => !(is_line || is_diag),
            Self::DiagonalInverted => !is_diag,
            Self::LineInverted => !is_line,
            Self::Circle => true,
        }
    }

    #[must_use]
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i64)]
pub enum AreaId {
    Point = 1,
    LaserLine = 2,
    // Many more exist; we only special-case a few in logic.
    FirstInline = 13,
    Enemies = 14,
    Allies = 15,
}

impl AreaId {
    #[must_use]
    pub fn from_i64(v: i64) -> Option<Self> {
        Some(match v {
            1 => Self::Point,
            2 => Self::LaserLine,
            13 => Self::FirstInline,
            14 => Self::Enemies,
            15 => Self::Allies,
            _ => return None,
        })
    }

    #[must_use]
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i64)]
pub enum EffectType {
    Damage = 1,
    Heal = 2,
    BuffStrength = 3,
    BuffAgility = 4,
    RelativeShield = 5,
    AbsoluteShield = 6,
    Debuff = 9,
    Teleport = 10,
    Permutation = 11,
    Poison = 13,
    Summon = 14,
    Resurrect = 15,
    Kill = 16,
    ShackleMp = 17,
    ShackleTp = 18,
    ShackleStrength = 19,
    DamageReturn = 20,
    Antidote = 23,
    Vulnerability = 26,
    RemoveShackles = 49,
    Attract = 46,
    Push = 51,
    Repel = 53,
    AddState = 59,
    TotalDebuff = 60,
}

impl EffectType {
    #[must_use]
    pub fn from_i64(v: i64) -> Option<Self> {
        Some(match v {
            1 => Self::Damage,
            2 => Self::Heal,
            3 => Self::BuffStrength,
            4 => Self::BuffAgility,
            5 => Self::RelativeShield,
            6 => Self::AbsoluteShield,
            9 => Self::Debuff,
            10 => Self::Teleport,
            11 => Self::Permutation,
            13 => Self::Poison,
            14 => Self::Summon,
            15 => Self::Resurrect,
            16 => Self::Kill,
            17 => Self::ShackleMp,
            18 => Self::ShackleTp,
            19 => Self::ShackleStrength,
            20 => Self::DamageReturn,
            23 => Self::Antidote,
            26 => Self::Vulnerability,
            49 => Self::RemoveShackles,
            46 => Self::Attract,
            51 => Self::Push,
            53 => Self::Repel,
            59 => Self::AddState,
            60 => Self::TotalDebuff,
            _ => return None,
        })
    }

    #[must_use]
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ChipItemId(pub i64);

impl ChipItemId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ChipTemplateId(pub i64);

impl ChipTemplateId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct WeaponItemId(pub i64);

impl WeaponItemId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct WeaponTemplateId(pub i64);

impl WeaponTemplateId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct SummonId(pub i64);

impl SummonId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct CellId(pub i64);

impl CellId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct ChipEffectDef {
    pub id: EffectType,
    pub value1: f64,
    pub value2: f64,
    pub turns: i64,
    pub targets: i64,
    pub modifiers: i64,
    pub r#type: i64,
}

#[derive(Debug, Clone)]
pub struct ChipDef {
    pub item: ChipItemId,
    pub template: ChipTemplateId,
    pub cost: i64,
    pub min_range: i64,
    pub max_range: i64,
    pub launch_type: LaunchType,
    pub area: i64,
    pub los: bool,
    pub cooldown: i64,
    pub team_cooldown: bool,
    pub initial_cooldown: i64,
    pub max_uses: i64,
    pub effects: Vec<ChipEffectDef>,
}

#[derive(Debug, Clone)]
pub struct SummonDef {
    pub id: SummonId,
    pub name: String,
    pub chips: Vec<i64>,
    pub life_range: (i64, i64),
    pub tp_range: (i64, i64),
    pub mp_range: (i64, i64),
    pub strength_range: (i64, i64),
}

#[derive(Debug, Clone)]
pub struct EffectInstance {
    pub instance_id: i64,
    pub item_id: i64,
    pub caster: i64,
    pub target: i64,
    pub effect_id: EffectType,
    pub value: i64,
    pub turns_left: i64,
    pub modifiers: i64,
    pub from_weapon: bool,
}

#[derive(Debug, Clone)]
pub struct WeaponDef {
    pub item: WeaponItemId,
    pub template: WeaponTemplateId,
    pub cost: i64,
    pub min_range: i64,
    pub max_range: i64,
    pub launch_type: LaunchType,
    pub base_damage: i64,
    pub los: bool,
    pub area: i64,
    pub max_uses: i64,
}

#[derive(Debug, Clone, Default)]
pub struct LeekWarsEntity {
    pub id: i64,
    pub name: String,
    pub team: i64,
    pub cell: i64,
    pub life: i64,
    pub total_life: i64,
    pub strength: i64,
    pub agility: i64,
    pub magic: i64,
    pub science: i64,
    pub wisdom: i64,
    pub resistance: i64,
    pub power: i64,
    pub tp: i64,
    pub mp: i64,
    pub max_tp: i64,
    pub max_mp: i64,
    pub weapons: Vec<i64>,
    pub chips: Vec<i64>,
    pub equipped_weapon: Option<i64>,
    pub registers: Option<Registers>,
    pub is_summon: bool,
    pub chip_cooldowns: HashMap<i64, i64>,
    pub item_uses: HashMap<i64, i64>,
    pub effects: Vec<EffectInstance>,
    pub shield_abs: i64,
    pub shield_rel_percent: i64,
    pub strength_bonus: i64,
    pub mp_bonus: i64,
    pub tp_bonus: i64,
    pub damage_return: i64,
    pub state_unhealable: bool,
    pub state_invincible: bool,
    pub state_static: bool,
}

#[derive(Debug)]
pub struct LeekWarsState {
    pub entities: HashMap<i64, LeekWarsEntity>,
    pub say_log: Vec<(i64, String)>,
    pub fight_actions: Vec<serde_json::Value>,
    pub map: WorldMap,
    pub weapons: HashMap<i64, WeaponDef>,
    pub chips: HashMap<i64, ChipDef>,
    pub summons: HashMap<SummonId, SummonDef>,
    pub register_manager: Option<RegisterManagerRc>,
    pub next_effect_instance_id: i64,
    pub turn_order: Vec<i64>,
    pub next_entity_id: i64,
    pub rng_state: i64,
    pub team_chip_cooldowns: HashMap<(i64, i64), i64>,
}

#[derive(Debug, Clone)]
pub struct LeekWarsContext {
    pub self_id: i64,
    pub state: Rc<RefCell<LeekWarsState>>,
}

fn ctx<'a>(vm: &'a leekscript::vm::Vm) -> Result<&'a LeekWarsContext, VmError> {
    vm.host_ref::<LeekWarsContext>()
        .ok_or(VmError::MissingHost("LeekWarsContext"))
}

fn ctx_mut<'a>(vm: &'a mut leekscript::vm::Vm) -> Result<&'a mut LeekWarsContext, VmError> {
    vm.host_mut::<LeekWarsContext>()
        .ok_or(VmError::MissingHost("LeekWarsContext"))
}

fn with_state<R>(vm: &leekscript::vm::Vm, f: impl FnOnce(&LeekWarsState) -> R) -> Result<R, VmError> {
    let c = ctx(vm)?;
    Ok(f(&c.state.borrow()))
}

fn with_state_mut<R>(
    vm: &mut leekscript::vm::Vm,
    f: impl FnOnce(&mut LeekWarsState, i64) -> R,
) -> Result<R, VmError> {
    let self_id = ctx(vm)?.self_id;
    let c = ctx_mut(vm)?;
    Ok(f(&mut c.state.borrow_mut(), self_id))
}

fn cell_xy_i64(st: &LeekWarsState, cell: i64) -> Option<(i32, i32)> {
    let id = i32::try_from(cell).ok()?;
    let c = st.map.get_cell(id)?;
    Some((c.x, c.y))
}

fn cell_dist_i64(st: &LeekWarsState, a: i64, b: i64) -> i64 {
    let Ok(aa) = i32::try_from(a) else { return i64::MAX };
    let Ok(bb) = i32::try_from(b) else { return i64::MAX };
    st.map.case_distance(aa, bb).map(|d| d as i64).unwrap_or(i64::MAX)
}

fn spend_tp(st: &mut LeekWarsState, self_id: i64, amount: i64) {
    if let Some(me) = st.entities.get_mut(&self_id) {
        me.tp = (me.tp - amount).max(0);
    }
}

fn spend_mp(st: &mut LeekWarsState, self_id: i64, amount: i64) {
    if let Some(me) = st.entities.get_mut(&self_id) {
        me.mp = (me.mp - amount).max(0);
    }
}

fn effect_add_action(ef: &EffectInstance) -> serde_json::Value {
    // Matches reference ActionAddEffect JSON:
    // [ADD_CHIP_EFFECT|ADD_WEAPON_EFFECT, itemID, id, caster, target, effectID, value, turns, modifiers?]
    let opcode = if ef.from_weapon { 301 } else { 302 };
    if ef.modifiers != 0 {
        json!([
            opcode,
            ef.item_id,
            ef.instance_id,
            ef.caster,
            ef.target,
            ef.effect_id.as_i64(),
            ef.value,
            ef.turns_left,
            ef.modifiers
        ])
    } else {
        json!([
            opcode,
            ef.item_id,
            ef.instance_id,
            ef.caster,
            ef.target,
            ef.effect_id.as_i64(),
            ef.value,
            ef.turns_left
        ])
    }
}

fn tick_effects_start_turn(st: &mut LeekWarsState) {
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
                    let dealt = apply_damage_with_shields_with(ent, dmg, shield_abs, shield_rel_percent);
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
            EffectType::RelativeShield => ent.shield_rel_percent = ent.shield_rel_percent.saturating_add(ef.value),
            EffectType::Vulnerability => ent.shield_rel_percent = ent.shield_rel_percent.saturating_sub(ef.value.max(0)),
            EffectType::AbsoluteShield => ent.shield_abs = ent.shield_abs.saturating_add(ef.value.max(0)),
            EffectType::BuffStrength => ent.strength_bonus = ent.strength_bonus.saturating_add(ef.value),
            EffectType::ShackleMp => ent.mp_bonus = ent.mp_bonus.saturating_sub(ef.value.max(0)),
            EffectType::ShackleTp => ent.tp_bonus = ent.tp_bonus.saturating_sub(ef.value.max(0)),
            EffectType::DamageReturn => ent.damage_return = ent.damage_return.saturating_add(ef.value.max(0)),
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

fn apply_damage_with_shields(ent: &mut LeekWarsEntity, incoming: i64) -> i64 {
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

fn apply_erosion(ent: &mut LeekWarsEntity, erosion: i64) {
    if erosion <= 0 {
        return;
    }
    ent.total_life = (ent.total_life - erosion).max(0);
    if ent.life > ent.total_life {
        ent.life = ent.total_life;
    }
}

fn apply_damage_with_shields_with(
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

fn reduce_effects(
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

fn clear_poisons(st: &mut LeekWarsState, target_id: i64) {
    let Some(t) = st.entities.get_mut(&target_id) else { return };
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

fn remove_shackles(st: &mut LeekWarsState, target_id: i64) {
    let Some(t) = st.entities.get_mut(&target_id) else { return };
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

fn cell_blocked(st: &LeekWarsState, cell: i64, ignore_entity: Option<i64>) -> bool {
    let Ok(cid) = i32::try_from(cell) else {
        return true;
    };
    let Some(c) = st.map.get_cell(cid) else {
        return true;
    };
    if !c.walkable {
        return true;
    }
    st.entities.values().any(|e| e.life > 0 && Some(e.id) != ignore_entity && e.cell == cell)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlideMode {
    Push,
    Attract,
}

fn slide_toward_target_with_checks(
    st: &LeekWarsState,
    entity_cell: i64,
    target_cell: i64,
    caster_cell: i64,
    mode: SlideMode,
    ignore_entity: Option<i64>,
) -> i64 {
    let Ok(eid) = i32::try_from(entity_cell) else { return entity_cell };
    let Ok(tid) = i32::try_from(target_cell) else { return entity_cell };
    let Ok(cid) = i32::try_from(caster_cell) else { return entity_cell };
    let Some(ec) = st.map.get_cell(eid) else { return entity_cell };
    let Some(tc) = st.map.get_cell(tid) else { return entity_cell };
    let Some(cc) = st.map.get_cell(cid) else { return entity_cell };
    let (ex, ey) = (ec.x, ec.y);
    let (tx, ty) = (tc.x, tc.y);
    let (cx, cy) = (cc.x, cc.y);

    // Direction checks:
    // - cdx/cdy = sign(entity - caster)
    // - dx/dy = sign(target - entity)
    // - push: require cdx==dx && cdy==dy
    // - attract: require cdx==-dx && cdy==-dy
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
        let Some(c0) = st.map.get_cell(cur) else { break };
        let Some(next) = st.map.get_cell_xy(c0.x + dx, c0.y + dy) else { break };
        if cell_blocked(st, next as i64, ignore_entity) {
            return cur as i64;
        }
        cur = next;
    }
    cur as i64
}

fn slide_away_until_blocked(st: &LeekWarsState, start: i64, away_from: i64, ignore_entity: Option<i64>) -> i64 {
    let Ok(aid) = i32::try_from(away_from) else { return start };
    let Some(ac) = st.map.get_cell(aid) else { return start };
    let (ax, ay) = (ac.x, ac.y);
    let Ok(mut cur) = i32::try_from(start) else { return start };
    loop {
        let Some(cc) = st.map.get_cell(cur) else { break };
        let (cx, cy) = (cc.x, cc.y);
        let dx = (cx - ax).signum();
        let dy = (cy - ay).signum();
        if dx == 0 && dy == 0 {
            break;
        }
        let Some(next) = st.map.get_cell_xy(cx + dx, cy + dy) else { break };
        if cell_blocked(st, next as i64, ignore_entity) {
            break;
        }
        cur = next;
    }
    cur as i64
}

fn slide_entity(st: &mut LeekWarsState, entity_id: i64, dest: i64) {
    let Some(e) = st.entities.get_mut(&entity_id) else { return };
    if e.life <= 0 {
        return;
    }
    recompute_derived_buffs(e);
    if e.state_static || dest == e.cell {
        return;
    }
    e.cell = dest;
    // Sliding updates position but does not emit a MOVE_TO action.
}

fn tick_chip_cooldowns(st: &mut LeekWarsState) {
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

// Exposed for generator turn-loop hooks.
pub fn tick_effects_start_turn_public(st: &mut LeekWarsState) {
    tick_effects_start_turn(st);
}

pub fn tick_chip_cooldowns_public(st: &mut LeekWarsState) {
    tick_chip_cooldowns(st);
}

// Reference RNG LCG:
// n = n * 1103515245 + 12345; r = (n / 65536) % 32768 + 32768; return r/65536
fn rng_next_u32(st: &mut LeekWarsState) -> u32 {
    st.rng_state = st
        .rng_state
        .wrapping_mul(1_103_515_245)
        .wrapping_add(12_345);
    let r = ((st.rng_state / 65_536) % 32_768) + 32_768;
    r as u32
}

fn rng_double_0_1(st: &mut LeekWarsState) -> f64 {
    // getDouble() returns r/65536 with r in [32768, 65535].
    (rng_next_u32(st) as f64) / 65536.0
}

fn rng_int_inclusive(st: &mut LeekWarsState, min: i64, max: i64) -> i64 {
    if max < min {
        return min;
    }
    let span = (max - min + 1).max(1) as u64;
    let x = rng_next_u32(st) as u64;
    min + (x % span) as i64
}

#[cfg(test)]
mod parity_tests {
    use super::*;

    #[test]
    fn use_chip_logs_action_and_applies_damage() {
        let mut st = LeekWarsState {
            entities: HashMap::from([(
                1,
                LeekWarsEntity {
                    id: 1,
                    name: "A".into(),
                    team: 1,
                    cell: 10,
                    life: 20,
                    total_life: 20,
                    strength: 0,
                    agility: 0,
                    magic: 0,
                    science: 0,
                    wisdom: 0,
                    resistance: 0,
                    power: 0,
                    tp: 10,
                    mp: 0,
                    max_tp: 10,
                    max_mp: 0,
                    weapons: vec![],
                    chips: vec![],
                    equipped_weapon: None,
                    registers: None,
                    is_summon: false,
                    chip_cooldowns: HashMap::new(),
                    item_uses: HashMap::new(),
                    effects: Vec::new(),
                    shield_abs: 0,
                    shield_rel_percent: 0,
                    strength_bonus: 0,
                    mp_bonus: 0,
                    tp_bonus: 0,
                    damage_return: 0,
                    state_unhealable: false,
                    state_invincible: false,
                    state_static: false,
                },
            ), (
                2,
                LeekWarsEntity {
                    id: 2,
                    name: "B".into(),
                    team: 2,
                    cell: 11,
                    life: 20,
                    total_life: 20,
                    strength: 0,
                    agility: 0,
                    magic: 0,
                    science: 0,
                    wisdom: 0,
                    resistance: 0,
                    power: 0,
                    tp: 0,
                    mp: 0,
                    max_tp: 0,
                    max_mp: 0,
                    weapons: vec![],
                    chips: vec![],
                    equipped_weapon: None,
                    registers: None,
                    is_summon: false,
                    chip_cooldowns: HashMap::new(),
                    item_uses: HashMap::new(),
                    effects: Vec::new(),
                    shield_abs: 0,
                    shield_rel_percent: 0,
                    strength_bonus: 0,
                    mp_bonus: 0,
                    tp_bonus: 0,
                    damage_return: 0,
                    state_unhealable: false,
                    state_invincible: false,
                    state_static: false,
                },
            )]),
            say_log: vec![],
            fight_actions: vec![],
            map: WorldMap::new(18, 18),
            weapons: HashMap::new(),
            chips: HashMap::from([(
                999,
                ChipDef {
                    item: ChipItemId(999),
                    template: ChipTemplateId(6),
                    cost: 2,
                    min_range: 0,
                    max_range: 6,
                    launch_type: LaunchType::Circle,
                    area: 1,
                    los: true,
                    cooldown: 1,
                    team_cooldown: false,
                    initial_cooldown: 0,
                    max_uses: -1,
                    effects: vec![ChipEffectDef {
                        id: EffectType::Damage,
                        value1: 5.0,
                        value2: 0.0,
                        turns: 0,
                        targets: 0,
                        modifiers: 0,
                        r#type: 1,
                    }],
                },
            )]),
            summons: HashMap::new(),
            register_manager: None,
            next_effect_instance_id: 0,
            turn_order: vec![1, 2],
            next_entity_id: 2,
            rng_state: 0,
            team_chip_cooldowns: HashMap::new(),
        };

        let ok = apply_chip_use(&mut st, 1, 6, 11);
        assert_eq!(ok, 1);
        let has = |opcode: i64| {
            st.fight_actions.iter().any(|a| {
                a.as_array()
                    .and_then(|x| x.get(0))
                    .and_then(|v| v.as_i64())
                    == Some(opcode)
            })
        };
        assert!(has(12));
        assert!(has(101));
    }

    #[test]
    fn use_chip_fails_when_out_of_range() {
        let mut st = LeekWarsState {
            entities: HashMap::from([(
                1,
                LeekWarsEntity {
                    id: 1,
                    name: "A".into(),
                    team: 1,
                    cell: 0,
                    life: 20,
                    total_life: 20,
                    strength: 0,
                    agility: 0,
                    magic: 0,
                    science: 0,
                    wisdom: 0,
                    resistance: 0,
                    power: 0,
                    tp: 10,
                    mp: 0,
                    max_tp: 10,
                    max_mp: 0,
                    weapons: vec![],
                    chips: vec![],
                    equipped_weapon: None,
                    registers: None,
                    is_summon: false,
                    chip_cooldowns: HashMap::new(),
                    item_uses: HashMap::new(),
                    effects: Vec::new(),
                    shield_abs: 0,
                    shield_rel_percent: 0,
                    strength_bonus: 0,
                    mp_bonus: 0,
                    tp_bonus: 0,
                    damage_return: 0,
                    state_unhealable: false,
                    state_invincible: false,
                    state_static: false,
                },
            )]),
            say_log: vec![],
            fight_actions: vec![],
            map: WorldMap::new(18, 18),
            weapons: HashMap::new(),
            chips: HashMap::from([(
                1,
                ChipDef {
                    item: ChipItemId(1),
                    template: ChipTemplateId(6),
                    cost: 2,
                    min_range: 0,
                    max_range: 0,
                    launch_type: LaunchType::Circle,
                    area: 1,
                    los: false,
                    cooldown: 0,
                    team_cooldown: false,
                    initial_cooldown: 0,
                    max_uses: -1,
                    effects: vec![],
                },
            )]),
            summons: HashMap::new(),
            register_manager: None,
            next_effect_instance_id: 0,
            turn_order: vec![1],
            next_entity_id: 1,
            rng_state: 0,
            team_chip_cooldowns: HashMap::new(),
        };
        // cell 1 is out of range when max_range==0
        let ok = apply_chip_use(&mut st, 1, 6, 1);
        assert_eq!(ok, 0);
        assert_eq!(st.fight_actions.last().and_then(|v| v.as_array()).and_then(|a| a.get(3)).and_then(|v| v.as_i64()), Some(0));
    }
}

fn ensure_registers_loaded(st: &mut LeekWarsState, entity_id: i64) -> &mut Registers {
    // If no entity exists, create a dummy new registers container (won't be persisted).
    // Keep behavior consistent with the reference generator (null -> new registers).
    if !st.entities.contains_key(&entity_id) {
        st.entities.insert(
            entity_id,
            LeekWarsEntity {
                id: entity_id,
                name: String::new(),
                team: 0,
                cell: 0,
                life: 0,
                total_life: 0,
                strength: 0,
                agility: 0,
                magic: 0,
                science: 0,
                wisdom: 0,
                resistance: 0,
                power: 0,
                tp: 0,
                mp: 0,
                max_tp: 0,
                max_mp: 0,
                weapons: Vec::new(),
                chips: Vec::new(),
                equipped_weapon: None,
                registers: Some(Registers::new(true)),
                is_summon: false,
                chip_cooldowns: HashMap::new(),
                item_uses: HashMap::new(),
                effects: Vec::new(),
                shield_abs: 0,
                shield_rel_percent: 0,
                strength_bonus: 0,
                mp_bonus: 0,
                tp_bonus: 0,
                damage_return: 0,
                state_unhealable: false,
                state_invincible: false,
                state_static: false,
            },
        );
    }
    let ent = st.entities.get_mut(&entity_id).expect("inserted above");
    if ent.registers.is_none() {
        let (json, is_new) = match &st.register_manager {
            Some(mgr) => match mgr.get_registers(entity_id) {
                Some(s) => (s, false),
                None => ("{}".to_string(), true),
            },
            None => ("{}".to_string(), true),
        };
        ent.registers = Some(Registers::from_json_string(&json, is_new));
    }
    ent.registers.as_mut().expect("set above")
}

fn entity_id_from_args(vm: &leekscript::vm::Vm, args: &[Value]) -> Result<i64, VmError> {
    if args.is_empty() {
        return Ok(ctx(vm)?.self_id);
    }
    if args.len() != 1 {
        return Err(VmError::BadArgCount {
            expected: 1,
            got: args.len(),
        });
    }
    let n = args[0].as_number().ok_or(VmError::ExpectedNumber)?;
    Ok(n as i64)
}

fn nf_get_entity(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if !args.is_empty() {
        return Err(VmError::BadArgCount {
            expected: 0,
            got: args.len(),
        });
    }
    vm.add_operations(5)?;
    Ok(Value::num_int(ctx(vm)?.self_id))
}

fn nf_get_cell(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    vm.add_operations(5)?;
    let id = entity_id_from_args(vm, args)?;
    Ok(Value::num_int(with_state(vm, |st| {
        st.entities.get(&id).map(|e| e.cell).unwrap_or(0)
    })?))
}

fn nf_get_life(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    vm.add_operations(15)?;
    let id = entity_id_from_args(vm, args)?;
    Ok(Value::num_int(with_state(vm, |st| {
        st.entities.get(&id).map(|e| e.life).unwrap_or(0)
    })?))
}

fn nf_get_strength(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    vm.add_operations(15)?;
    let id = entity_id_from_args(vm, args)?;
    Ok(Value::num_int(with_state(vm, |st| {
        st.entities.get(&id).map(|e| e.strength).unwrap_or(0)
    })?))
}

fn nf_get_tp(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    vm.add_operations(15)?;
    let id = entity_id_from_args(vm, args)?;
    Ok(Value::num_int(with_state(vm, |st| {
        st.entities.get(&id).map(|e| e.tp).unwrap_or(0)
    })?))
}

fn nf_get_mp(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    vm.add_operations(15)?;
    let id = entity_id_from_args(vm, args)?;
    Ok(Value::num_int(with_state(vm, |st| {
        st.entities.get(&id).map(|e| e.mp).unwrap_or(0)
    })?))
}

fn nf_get_team(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    vm.add_operations(15)?;
    let id = entity_id_from_args(vm, args)?;
    Ok(Value::num_int(with_state(vm, |st| {
        st.entities.get(&id).map(|e| e.team).unwrap_or(0)
    })?))
}

fn nf_get_name(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    vm.add_operations(15)?;
    let id = entity_id_from_args(vm, args)?;
    Ok(Value::String(
        with_state(vm, |st| {
            st.entities
                .get(&id)
                .map(|e| e.name.clone())
                .unwrap_or_default()
        })?
        .into(),
    ))
}

fn nf_say(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(VmError::BadArgCount {
            expected: 1,
            got: args.len(),
        });
    }
    vm.add_operations(30)?;
    let msg = match &args[0] {
        Value::String(s) => s.clone(),
        other => format!("{other:?}").into(),
    };
    with_state_mut(vm, |st, self_id| {
        st.say_log.push((self_id, msg));
        st.fight_actions
            .push(json!([203, st.say_log.last().unwrap().1.replace('\t', "    ")]));
    })?;
    Ok(Value::Null)
}

fn nf_get_weapons(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    vm.add_operations(50)?;
    let id = entity_id_from_args(vm, args)?;
    let arr: Vec<Value> = with_state(vm, |st| {
        st.entities
            .get(&id)
            .map(|e| e.weapons.iter().copied().map(Value::num_int).collect())
            .unwrap_or_default()
    })?;
    Ok(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(arr))))
}

fn nf_get_weapon(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    vm.add_operations(15)?;
    if !args.is_empty() {
        // Only support the defaulted `entity = getEntity()` for now.
        return Err(VmError::BadArgCount {
            expected: 0,
            got: args.len(),
        });
    }
    let self_id = ctx(vm)?.self_id;
    let w = with_state(vm, |st| {
        st.entities.get(&self_id).and_then(|e| e.equipped_weapon)
    })?;
    Ok(w.map_or(Value::Null, Value::num_int))
}

fn nf_set_weapon(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(VmError::BadArgCount {
            expected: 1,
            got: args.len(),
        });
    }
    vm.add_operations(15)?;
    let w = args[0].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    with_state_mut(vm, |st, self_id| {
        if let Some(me) = st.entities.get_mut(&self_id) {
            me.equipped_weapon = Some(w);
        }
    })?;
    Ok(Value::Null)
}

fn nf_get_nearest_enemy(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if !args.is_empty() {
        return Err(VmError::BadArgCount {
            expected: 0,
            got: args.len(),
        });
    }
    vm.add_operations(25)?;
    let self_id = ctx(vm)?.self_id;
    let enemy = with_state(vm, |st| {
        let Some(me) = st.entities.get(&self_id) else {
            return -1;
        };
        let self_team = me.team;

        let mut candidates: Vec<(i64, i64)> = st
            .entities
            .values()
            .filter(|e| e.life > 0 && e.team != self_team)
            .map(|e| (e.id, e.cell))
            .collect();
        candidates.sort_by_key(|(id, _)| *id);

        let mut best: Option<(i64, i64)> = None; // (dist, enemy_id)
        for (enemy_id, enemy_cell) in candidates {
            let dist = cell_dist_i64(st, me.cell, enemy_cell);
            let key = (dist, enemy_id);
            if best.map_or(true, |b| key < b) {
                best = Some(key);
            }
        }
        best.map(|(_, id)| id).unwrap_or(-1)
    })?;
    Ok(Value::num_int(enemy))
}

fn nf_move_toward(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    // moveToward(entity, mp = getMP())
    if args.len() != 1 && args.len() != 2 {
        return Err(VmError::BadArgCount {
            expected: 2,
            got: args.len(),
        });
    }
    vm.add_operations(35)?;
    let target = args[0].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let self_id = ctx(vm)?.self_id;
    // STATIC entities cannot move.
    let static_me = with_state(vm, |st| {
        st.entities.get(&self_id).map(|e| e.state_static).unwrap_or(false)
    })?;
    if static_me {
        return Ok(Value::num_int(0));
    }
    let (from_cell, target_cell, self_mp) = with_state(vm, |st| {
        let from_cell = st.entities.get(&self_id).map(|e| e.cell).unwrap_or(0);
        let self_mp = st.entities.get(&self_id).map(|e| e.mp).unwrap_or(0);
        let target_cell = st
            .entities
            .get(&target)
            .filter(|e| e.life > 0)
            .map(|e| e.cell)
            .unwrap_or(from_cell);
        (from_cell, target_cell, self_mp)
    })?;

    // Reference behavior: pm_to_use=-1 means "use all MP"; otherwise cap to current MP.
    let mut mp_budget = if args.len() == 2 {
        args[1].as_number().ok_or(VmError::ExpectedNumber)? as i64
    } else {
        -1
    };
    if mp_budget == -1 {
        mp_budget = self_mp;
    }
    if mp_budget > self_mp {
        mp_budget = self_mp;
    }
    let mp_budget = mp_budget.max(0);
    if mp_budget == 0 || from_cell == target_cell {
        return Ok(Value::num_int(0));
    }

    // Path between start and target. If the target cell is occupied, the pathfinder
    // returns the path to the closest available cell (popping the occupied goal).
    let path: Vec<i64> = with_state(vm, |st| {
        // Other living entities block movement.
        let occupied: std::collections::HashSet<i32> = st
            .entities
            .values()
            .filter(|e| e.life > 0 && e.id != self_id)
            .filter_map(|e| i32::try_from(e.cell).ok())
            .collect();
        let start = i32::try_from(from_cell).ok()?;
        let goal = i32::try_from(target_cell).ok()?;
        st.map
            .a_star_path(start, &[goal], &occupied, None)
            .map(|v| v.into_iter().map(|x| x as i64).collect())
    })?
    .unwrap_or_default();

    let steps = (mp_budget as usize).min(path.len());
    if steps == 0 {
        return Ok(Value::num_int(0));
    }
    let moved_path: Vec<i64> = path.into_iter().take(steps).collect();
    let end_cell = *moved_path.last().unwrap_or(&from_cell);

    with_state_mut(vm, |st, self_id| {
        // Don't log/spend if no movement.
        if end_cell == from_cell {
            return;
        }
        spend_mp(st, self_id, steps as i64);
        if let Some(me) = st.entities.get_mut(&self_id) {
            me.cell = end_cell;
        }
        st.fight_actions
            .push(json!([10, self_id, end_cell, moved_path]));
    })?;
    Ok(Value::num_int(steps as i64))
}

fn nf_get_cell_x(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(VmError::BadArgCount {
            expected: 1,
            got: args.len(),
        });
    }
    vm.add_operations(5)?;
    let cell = args[0].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let x = with_state(vm, |st| cell_xy_i64(st, cell).map(|(x, _)| x).unwrap_or(0))?;
    Ok(Value::num_int(x as i64))
}

fn nf_get_cell_y(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(VmError::BadArgCount {
            expected: 1,
            got: args.len(),
        });
    }
    vm.add_operations(5)?;
    let cell = args[0].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let y = with_state(vm, |st| cell_xy_i64(st, cell).map(|(_, y)| y).unwrap_or(0))?;
    Ok(Value::num_int(y as i64))
}

fn nf_get_cell_from_xy(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(VmError::BadArgCount {
            expected: 2,
            got: args.len(),
        });
    }
    vm.add_operations(5)?;
    let x = args[0].as_number().ok_or(VmError::ExpectedNumber)? as i32;
    let y = args[1].as_number().ok_or(VmError::ExpectedNumber)? as i32;
    let cell = with_state(vm, |st| st.map.get_cell_xy(x, y).unwrap_or(0) as i64)?;
    Ok(Value::num_int(cell))
}

fn nf_get_entity_on_cell(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(VmError::BadArgCount {
            expected: 1,
            got: args.len(),
        });
    }
    vm.add_operations(15)?;
    let cell = args[0].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let id = with_state(vm, |st| {
        st.entities
            .values()
            .find(|e| e.cell == cell && e.life > 0)
            .map(|e| e.id)
            .unwrap_or(-1)
    })?;
    Ok(Value::num_int(id))
}

fn nf_get_cell_distance(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(VmError::BadArgCount {
            expected: 2,
            got: args.len(),
        });
    }
    vm.add_operations(15)?;
    let a = args[0].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let b = args[1].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let d = with_state(vm, |st| {
        let d = cell_dist_i64(st, a, b);
        if d == i64::MAX { 0 } else { d }
    })?;
    Ok(Value::num_int(d))
}

fn nf_get_path_distance(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(VmError::BadArgCount {
            expected: 2,
            got: args.len(),
        });
    }
    vm.add_operations(30)?;
    let a = args[0].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let b = args[1].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let blocked = with_state(vm, |st| {
        st.entities
            .values()
            .map(|e| e.cell)
            .collect::<HashSet<i64>>()
    })?;
    let d = with_state(vm, |st| {
        let occupied: std::collections::HashSet<i32> = blocked
            .iter()
            .filter_map(|c| i32::try_from(*c).ok())
            .collect();
        let Ok(start) = i32::try_from(a) else { return 0 };
        let Ok(goal) = i32::try_from(b) else { return 0 };
        let Some(p) = st.map.a_star_path(start, &[goal], &occupied, None) else { return 0 };
        p.len() as i64
    })?;
    Ok(Value::num_int(d))
}

fn nf_use_weapon(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(VmError::BadArgCount {
            expected: 1,
            got: args.len(),
        });
    }
    vm.add_operations(40)?;
    let target = args[0].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let self_id = ctx(vm)?.self_id;
    let Some((self_cell, target_cell)) = with_state(vm, |st| {
        let self_cell = st.entities.get(&self_id).map(|e| e.cell)?;
        let te = st.entities.get(&target)?;
        if te.life <= 0 {
            return None;
        }
        Some((self_cell, te.cell))
    })? else {
        return Ok(Value::Null);
    };
    with_state_mut(vm, |st, self_id| {
        let Some(me) = st.entities.get(&self_id).cloned() else {
            return;
        };

        let Some(wt) = me.equipped_weapon else {
            return;
        };
        let Some(wdef) = st.weapons.get(&wt).cloned() else {
            return;
        };

        // Per-turn max uses (reference behavior).
        if wdef.max_uses != -1 {
            let uses = me.item_uses.get(&wdef.item.as_i64()).copied().unwrap_or(0);
            if uses >= wdef.max_uses {
                return;
            }
        }

        // Range check.
        let dist = cell_dist_i64(st, self_cell, target_cell);
        if dist < wdef.min_range || dist > wdef.max_range {
            return;
        }

        // Launch-type validation.
        let (sx, sy) = cell_xy_i64(st, self_cell).unwrap_or((0, 0));
        let (tx, ty) = cell_xy_i64(st, target_cell).unwrap_or((0, 0));
        if !wdef.launch_type.allows(tx - sx, ty - sy) {
            return;
        }

        // Line of sight check (if enabled).
        if wdef.los {
            let Ok(sid) = i32::try_from(self_cell) else {
                return;
            };
            let Ok(eid) = i32::try_from(target_cell) else {
                return;
            };
            let occupied: std::collections::HashSet<i32> = st
                .entities
                .values()
                .filter(|e| e.life > 0)
                .filter_map(|e| i32::try_from(e.cell).ok())
                .collect();
            let mut ignored = std::collections::HashSet::new();
            ignored.insert(sid);
            if !st.map.verify_los(sid, eid, true, &occupied, &ignored) {
                return;
            }
        }

        // TP cost check/spend.
        if me.tp < wdef.cost {
            return;
        }
        // Compute crit + jet once per attack.
        let caster_agility = st.entities.get(&self_id).map(|e| e.agility).unwrap_or(0);
        let critical = rng_double_0_1(st) < (caster_agility as f64) / 1000.0;
        let critical_power = if critical { 1.3 } else { 1.0 };
        let _jet = rng_double_0_1(st);

        spend_tp(st, self_id, wdef.cost);
        st.fight_actions
            .push(json!([16, target_cell, if critical { 2 } else { 1 }]));

        // Count successful uses per turn.
        if let Some(me_mut) = st.entities.get_mut(&self_id) {
            me_mut
                .item_uses
                .entry(wdef.item.as_i64())
                .and_modify(|v| *v += 1)
                .or_insert(1);
        }

        let caster_strength = st
            .entities
            .get(&self_id)
            .map(|e| (e.strength + e.strength_bonus).max(0))
            .unwrap_or(0) as f64;
        let base = (wdef.base_damage as f64) * critical_power;
        let dmg = (base * (1.0 + caster_strength / 100.0)).round() as i64;
        if dmg == 0 {
            return;
        }

        let caster_team = st.entities.get(&self_id).map(|e| e.team).unwrap_or(0);
        let Ok(tgt) = i32::try_from(target_cell) else { return };
        let target_cells = st.map.area_cells(wdef.area, tgt);
        for cell in target_cells {
            let cell = cell as i64;
            let Some(tid) = st
                .entities
                .values()
                .find(|e| e.life > 0 && e.cell == cell)
                .map(|e| e.id)
            else {
                continue;
            };
            // Only damage enemies in this baseline.
            if st.entities.get(&tid).map(|e| e.team).unwrap_or(0) == caster_team {
                continue;
            }
            if let Some(t) = st.entities.get_mut(&tid) {
                if t.life > 0 {
                    recompute_derived_buffs(t);
                    let dealt = apply_damage_with_shields(t, dmg);
                    st.fight_actions.push(json!([101, tid, dealt, 0]));
                    if t.life == 0 {
                        st.fight_actions.push(json!([11, self_id, tid]));
                        st.fight_actions.push(json!([5, tid, self_id]));
                    }
                }
            }
        }
    })?;
    Ok(Value::num_int(1))
}

fn nf_use_chip(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    // useChip(chip_template_or_item, cell)
    if args.len() != 2 {
        return Err(VmError::BadArgCount {
            expected: 2,
            got: args.len(),
        });
    }
    vm.add_operations(50)?;
    let chip_id = args[0].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let cell = args[1].as_number().ok_or(VmError::ExpectedNumber)? as i64;
    let self_id = ctx(vm)?.self_id;

    with_state_mut(vm, |st, _| {
        let _ = apply_chip_use(st, self_id, chip_id, cell);
    })?;

    Ok(Value::num_int(1))
}

fn apply_chip_use(st: &mut LeekWarsState, self_id: i64, chip_id: i64, cell: i64) -> i64 {
    let Some(chip) = st
        .chips
        .values()
        .find(|c| c.template.as_i64() == chip_id || c.item.as_i64() == chip_id)
        .cloned()
    else {
        st.fight_actions.push(json!([12, chip_id, cell, 0]));
        return 0;
    };

    let cd_left = match st.entities.get(&self_id) {
        Some(me) => {
            if chip.team_cooldown {
                st.team_chip_cooldowns
                    .get(&(me.team, chip.template.as_i64()))
                    .copied()
                    .unwrap_or(0)
            } else {
                me.chip_cooldowns
                    .get(&chip.template.as_i64())
                    .copied()
                    .unwrap_or(0)
            }
        }
        None => 0,
    };
    if cd_left > 0 {
        st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
        return 0;
    }

    let Some(me) = st.entities.get(&self_id).cloned() else {
        st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
        return 0;
    };
    // Max uses per turn: checked before cast, counted on success.
    if chip.max_uses != -1 {
        let uses = me.item_uses.get(&chip.item.as_i64()).copied().unwrap_or(0);
        if uses >= chip.max_uses {
            st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
            return 0;
        }
    }
    if me.tp < chip.cost {
        st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
        return 0;
    }
    // Cast validation (range + LOS) uses the *cast cell*.
    let self_cell = me.cell;
    let dist = cell_dist_i64(st, self_cell, cell);
    if dist < chip.min_range || dist > chip.max_range {
        st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
        return 0;
    }
    // Launch-type validation.
    let (sx, sy) = cell_xy_i64(st, self_cell).unwrap_or((0, 0));
    let (tx, ty) = cell_xy_i64(st, cell).unwrap_or((0, 0));
    let ok_launch = chip.launch_type.allows(tx - sx, ty - sy);
    if !ok_launch {
        st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
        return 0;
    }
    if chip.los {
        let Ok(sid) = i32::try_from(self_cell) else {
            st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
            return 0;
        };
        let Ok(eid) = i32::try_from(cell) else {
            st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
            return 0;
        };
        let occupied: std::collections::HashSet<i32> = st
            .entities
            .values()
            .filter(|e| e.life > 0)
            .filter_map(|e| i32::try_from(e.cell).ok())
            .collect();
        let mut ignored = std::collections::HashSet::new();
        ignored.insert(sid);
        if !st.map.verify_los(sid, eid, true, &occupied, &ignored) {
            st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
            return 0;
        }
    }

    // Some effects require an empty/available target cell (teleport/summon/resurrect).
    let needs_empty = chip
        .effects
        .iter()
        .any(|e| matches!(e.id, EffectType::Teleport | EffectType::Summon | EffectType::Resurrect));
    if needs_empty {
        let Ok(cid) = i32::try_from(cell) else {
            st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
            return 0;
        };
        if st.map.is_obstacle(cid) {
            st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
            return 0;
        }
        if st.entities.values().any(|e| e.life > 0 && e.cell == cell) {
            st.fight_actions.push(json!([12, chip.template.as_i64(), cell, 0]));
            return 0;
        }
    }

    // Compute crit + jet once per attack.
    let caster_agility = st.entities.get(&self_id).map(|e| e.agility).unwrap_or(0);
    let critical = rng_double_0_1(st) < (caster_agility as f64) / 1000.0;
    let critical_power = if critical { 1.3 } else { 1.0 };
    let jet = rng_double_0_1(st);

    spend_tp(st, self_id, chip.cost);
    st.fight_actions
        .push(json!([12, chip.template.as_i64(), cell, if critical { 2 } else { 1 }]));
    if let Some(me_mut) = st.entities.get_mut(&self_id) {
        if chip.cooldown > 0 {
            if chip.team_cooldown {
                st.team_chip_cooldowns
                    .insert((me_mut.team, chip.template.as_i64()), chip.cooldown);
            } else {
                me_mut
                    .chip_cooldowns
                    .insert(chip.template.as_i64(), chip.cooldown);
            }
        }
        me_mut
            .item_uses
            .entry(chip.item.as_i64())
            .and_modify(|v| *v += 1)
            .or_insert(1);
    }

    let Ok(tgt) = i32::try_from(cell) else { return 0 };
    let target_cells = st.map.area_cells(chip.area, tgt);
    for tc in target_cells {
        let tc = tc as i64;
        let Some(tid) = st
            .entities
            .values()
            .find(|e| e.life > 0 && e.cell == tc)
            .map(|e| e.id)
        else {
            continue;
        };
        // AOE attenuation: 100% at center, then -20% per distance.
        let aoe_factor: f64 = {
            // Some areas use constant 1.0.
            if matches!(chip.area, 2 | 13 | 14 | 15) {
                1.0
            } else {
                let d = cell_dist_i64(st, cell, tc);
                let d = if d == i64::MAX { 0 } else { d }.max(0) as f64;
                (1.0f64 - d * 0.2f64).clamp(0.0f64, 1.0f64)
            }
        };

        for eff in &chip.effects {
            // Target filters (bitmask)
            // Enemies=1, Allies=2, Caster=4, Non-summons=8, Summons=16.
            let mut allowed = true;
            let caster_team = me.team;
            let is_ally = st.entities.get(&tid).map(|e| e.team == caster_team).unwrap_or(false);
            let is_enemy = !is_ally;
            let is_caster = tid == self_id;
            let is_summon = st.entities.get(&tid).map(|e| e.is_summon).unwrap_or(false);
            let targets = eff.targets;
            if targets != 0 {
                allowed = false;
                if (targets & 1) != 0 && is_enemy {
                    allowed = true;
                }
                if (targets & 2) != 0 && is_ally {
                    allowed = true;
                }
                if (targets & 4) != 0 && is_caster {
                    allowed = true;
                }
                if (targets & 8) != 0 && !is_summon {
                    allowed = true;
                }
                if (targets & 16) != 0 && is_summon {
                    allowed = true;
                }
            }
            if !allowed {
                continue;
            }

            match eff.id {
                EffectType::Attract => {
                    // Slide each target entity toward the cast cell.
                    let start = st.entities.get(&tid).map(|e| e.cell).unwrap_or(tc);
                    let caster_cell = st.entities.get(&self_id).map(|e| e.cell).unwrap_or(cell);
                    let dest = slide_toward_target_with_checks(
                        st,
                        start,
                        cell,
                        caster_cell,
                        SlideMode::Attract,
                        Some(tid),
                    );
                    slide_entity(st, tid, dest);
                }
                EffectType::Push => {
                    // Slide each target entity away from the cast cell.
                    let start = st.entities.get(&tid).map(|e| e.cell).unwrap_or(tc);
                    let caster_cell = st.entities.get(&self_id).map(|e| e.cell).unwrap_or(cell);
                    let dest = slide_toward_target_with_checks(
                        st,
                        start,
                        cell,
                        caster_cell,
                        SlideMode::Push,
                        Some(tid),
                    );
                    slide_entity(st, tid, dest);
                }
                EffectType::Repel => {
                    // Slide away from the caster cell.
                    let caster_cell = st.entities.get(&self_id).map(|e| e.cell).unwrap_or(cell);
                    let start = st.entities.get(&tid).map(|e| e.cell).unwrap_or(tc);
                    let dest = slide_away_until_blocked(st, start, caster_cell, Some(tid));
                    slide_entity(st, tid, dest);
                }
                EffectType::Damage => {
                    let caster_strength = st
                        .entities
                        .get(&self_id)
                        .map(|e| (e.strength + e.strength_bonus).max(0))
                        .unwrap_or(0) as f64;
                    let caster_power = st.entities.get(&self_id).map(|e| e.power).unwrap_or(0) as f64;
                    let base = (eff.value1 + jet * eff.value2) * aoe_factor * critical_power;
                    let dmg = (base
                        * (1.0 + caster_strength / 100.0)
                        * (1.0 + caster_power / 100.0))
                        .round()
                        .max(0.0) as i64;
                    if dmg > 0 {
                        if let Some(t) = st.entities.get_mut(&tid) {
                            recompute_derived_buffs(t);
                            // Return damage uses pre-shield base damage * damage_return% (on target).
                            let ret = if tid != self_id {
                                ((dmg as f64) * (t.damage_return as f64) / 100.0).round() as i64
                            } else {
                                0
                            };
                            let dealt = if t.state_invincible {
                                0
                            } else {
                                apply_damage_with_shields(t, dmg)
                            };
                            let erosion = (dealt as f64 * 0.05).round() as i64;
                            apply_erosion(t, erosion);
                            st.fight_actions.push(json!([101, tid, dealt, erosion]));
                            // Life steal: round(dealt * caster.wisdom / 1000)
                            if dealt > 0 && tid != self_id {
                                let steal = ((dealt as f64)
                                    * (st.entities.get(&self_id).map(|e| e.wisdom).unwrap_or(0) as f64)
                                    / 1000.0)
                                    .round() as i64;
                                if steal > 0 {
                                    if let Some(c) = st.entities.get_mut(&self_id) {
                                        recompute_derived_buffs(c);
                                        if c.life > 0 && c.life < c.total_life && !c.state_unhealable {
                                            let before = c.life;
                                            c.life = c.life.saturating_add(steal).min(c.total_life.max(0));
                                            let applied = c.life - before;
                                            if applied > 0 {
                                                st.fight_actions.push(json!([103, self_id, applied]));
                                            }
                                        }
                                    }
                                }
                            }
                            if ret > 0 {
                                if let Some(c) = st.entities.get_mut(&self_id) {
                                    recompute_derived_buffs(c);
                                    if c.life > 0 && !c.state_invincible {
                                        let dealt_back = apply_damage_with_shields(c, ret);
                                        let erosion = (dealt_back as f64 * 0.05).round() as i64;
                                        apply_erosion(c, erosion);
                                        st.fight_actions.push(json!([108, self_id, dealt_back, erosion]));
                                        if c.life == 0 {
                                            st.fight_actions.push(json!([11, tid, self_id]));
                                            st.fight_actions.push(json!([5, self_id, tid]));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                EffectType::Heal => {
                    let caster_wisdom = st.entities.get(&self_id).map(|e| e.wisdom).unwrap_or(0) as f64;
                    let base = (eff.value1 + jet * eff.value2) * aoe_factor * critical_power;
                    let heal = (base * (1.0 + caster_wisdom / 100.0)).round() as i64;
                    if heal > 0 {
                        if eff.turns > 0 {
                            // Duration heal: add effect, applied at start of turn.
                            st.next_effect_instance_id += 1;
                            let inst = EffectInstance {
                                instance_id: st.next_effect_instance_id,
                                item_id: chip.item.as_i64(),
                                caster: self_id,
                                target: tid,
                                effect_id: EffectType::Heal,
                                value: heal,
                                turns_left: eff.turns.max(0),
                                modifiers: eff.modifiers,
                                from_weapon: false,
                            };
                            let stackable = (eff.modifiers & 1) != 0;
                            add_or_stack_effect(st, tid, inst, stackable);
                        } else {
                            if let Some(t) = st.entities.get_mut(&tid) {
                                if t.life > 0 {
                                    recompute_derived_buffs(t);
                                    if t.state_unhealable {
                                        // no-op
                                        continue;
                                    }
                                    let before = t.life;
                                    t.life = t.life.saturating_add(heal).min(t.total_life.max(0));
                                    let applied = t.life - before;
                                    if applied > 0 {
                                        st.fight_actions.push(json!([103, tid, applied]));
                                    }
                                }
                            }
                        }
                    }
                }
                EffectType::Poison => {
                    let turns = eff.turns.max(0);
                    let caster_magic = st
                        .entities
                        .get(&self_id)
                        .map(|e| e.magic.max(0))
                        .unwrap_or(0) as f64;
                    let caster_power = st.entities.get(&self_id).map(|e| e.power).unwrap_or(0) as f64;
                    let base = (eff.value1 + jet * eff.value2) * aoe_factor * critical_power;
                    let val = (base
                        * (1.0 + caster_magic / 100.0)
                        * (1.0 + caster_power / 100.0))
                        .round() as i64;
                    if turns > 0 && val > 0 {
                        let stackable = (eff.modifiers & 1) != 0;
                        st.next_effect_instance_id += 1;
                        let inst = EffectInstance {
                            instance_id: st.next_effect_instance_id,
                            item_id: chip.item.as_i64(),
                            caster: self_id,
                            target: tid,
                            effect_id: EffectType::Poison,
                            value: val,
                            turns_left: turns,
                            modifiers: eff.modifiers,
                            from_weapon: false,
                        };
                        add_or_stack_effect(st, tid, inst, stackable);
                    }
                }
                EffectType::Summon => {
                    let sid = eff.value1.round() as i64;
                    let Some(def) = st.summons.get(&SummonId(sid)).cloned() else { continue };
                    st.next_entity_id += 1;
                    let new_id = st.next_entity_id;
                    let occupied = st.entities.values().any(|e| e.life > 0 && e.cell == cell);
                    if occupied {
                        st.fight_actions.push(json!([9, self_id, new_id, cell, 0]));
                        continue;
                    }
                    // Deterministic RNG (seeded from scenario) for summon stats.
                    let life = rng_int_inclusive(st, def.life_range.0, def.life_range.1);
                    let tp = rng_int_inclusive(st, def.tp_range.0, def.tp_range.1);
                    let mp = rng_int_inclusive(st, def.mp_range.0, def.mp_range.1);
                    let strength =
                        rng_int_inclusive(st, def.strength_range.0, def.strength_range.1);
                    st.entities.insert(
                        new_id,
                        LeekWarsEntity {
                            id: new_id,
                            name: def.name.clone(),
                            team: me.team,
                            cell,
                            life,
                            total_life: life,
                            strength,
                            agility: 0,
                            magic: 0,
                            science: 0,
                            wisdom: 0,
                            resistance: 0,
                            power: 0,
                            tp,
                            mp,
                            max_tp: tp,
                            max_mp: mp,
                            weapons: Vec::new(),
                            chips: def.chips.clone(),
                            equipped_weapon: None,
                            registers: None,
                            is_summon: true,
                            chip_cooldowns: HashMap::new(),
                            item_uses: HashMap::new(),
                            effects: Vec::new(),
                shield_abs: 0,
                shield_rel_percent: 0,
                strength_bonus: 0,
                            mp_bonus: 0,
                            tp_bonus: 0,
                            damage_return: 0,
                            state_unhealable: false,
                            state_invincible: false,
                            state_static: false,
                        },
                    );
                    st.turn_order.push(new_id);
                    st.fight_actions.push(json!([9, self_id, new_id, cell, 1]));
                }
                EffectType::Resurrect => {
                    if let Some(t) = st.entities.get_mut(&tid) {
                        if t.life == 0 {
                            t.life = 1;
                            st.fight_actions.push(json!([105, tid]));
                        }
                    }
                }
                EffectType::Kill => {
                    if let Some(t) = st.entities.get_mut(&tid) {
                        if t.life > 0 {
                            t.life = 0;
                            st.fight_actions.push(json!([11, self_id, tid]));
                            st.fight_actions.push(json!([5, tid, self_id]));
                        }
                    }
                }
                EffectType::Teleport => {
                    // TELEPORT: move the caster to the cast cell (validated above).
                    if let Some(c) = st.entities.get_mut(&self_id) {
                        if c.life > 0 {
                            recompute_derived_buffs(c);
                            if c.state_static {
                                continue;
                            }
                            let from = c.cell;
                            c.cell = cell;
                            st.fight_actions.push(json!([10, self_id, cell, vec![from, cell]]));
                        }
                    }
                }
                EffectType::Permutation => {
                    // PERMUTATION: swap caster and entity on the cast cell, if any.
                    let target_id_opt = st
                        .entities
                        .values()
                        .find(|e| e.life > 0 && e.cell == cell)
                        .map(|e| e.id);
                    if let Some(other_id) = target_id_opt {
                        if other_id != self_id {
                            // Cannot invert a STATIC target.
                            let other_static = st
                                .entities
                                .get(&other_id)
                                .map(|e| e.state_static)
                                .unwrap_or(false);
                            if other_static {
                                continue;
                            }
                            let (from_caster, from_other) = {
                                let ccell = st.entities.get(&self_id).map(|e| e.cell).unwrap_or(cell);
                                let ocell = st.entities.get(&other_id).map(|e| e.cell).unwrap_or(cell);
                                (ccell, ocell)
                            };
                            if let Some(c) = st.entities.get_mut(&self_id) {
                                recompute_derived_buffs(c);
                                if c.state_static {
                                    continue;
                                }
                                c.cell = from_other;
                                st.fight_actions.push(json!([10, self_id, from_other, vec![from_caster, from_other]]));
                            }
                            if let Some(o) = st.entities.get_mut(&other_id) {
                                o.cell = from_caster;
                                st.fight_actions.push(json!([10, other_id, from_caster, vec![from_other, from_caster]]));
                            }
                        }
                    }
                }
                EffectType::ShackleMp
                | EffectType::ShackleTp
                | EffectType::ShackleStrength
                | EffectType::DamageReturn
                | EffectType::Vulnerability => {
                    // Duration-based debuffs/retaliation/vulnerability represented as effects.
                    let turns = eff.turns.max(0);
                    // Mirror reference scalars:
                    // - shackles multiply by (1 + max(0, magic)/100)
                    // - damage_return multiplies by (1 + agility/100)
                    // - vulnerability is a negative RELATIVE_SHIELD stat (no special scalar).
                    let base = (eff.value1 + jet * eff.value2) * aoe_factor * critical_power;
                    let caster_magic = st.entities.get(&self_id).map(|e| e.magic).unwrap_or(0).max(0) as f64;
                    let caster_agility = st.entities.get(&self_id).map(|e| e.agility).unwrap_or(0) as f64;
                    let scaled = match eff.id {
                        EffectType::ShackleMp | EffectType::ShackleTp | EffectType::ShackleStrength => {
                            base * (1.0 + caster_magic / 100.0)
                        }
                        EffectType::DamageReturn => base * (1.0 + caster_agility / 100.0),
                        EffectType::Vulnerability => base,
                        _ => base,
                    };
                    let val = scaled.round() as i64;
                    if turns > 0 && val != 0 {
                        let stackable = (eff.modifiers & 1) != 0;
                        st.next_effect_instance_id += 1;
                        let inst = EffectInstance {
                            instance_id: st.next_effect_instance_id,
                            item_id: chip.item.as_i64(),
                            caster: self_id,
                            target: tid,
                            effect_id: eff.id,
                            value: val.abs(),
                            turns_left: turns,
                            modifiers: eff.modifiers,
                            from_weapon: false,
                        };
                        add_or_stack_effect(st, tid, inst, stackable);
                    }
                }
                EffectType::BuffStrength | EffectType::RelativeShield | EffectType::AbsoluteShield => {
                    // Duration-based buffs/shields represented as effects.
                    let turns = eff.turns.max(0);
                    let base = (eff.value1 + jet * eff.value2) * aoe_factor * critical_power;
                    let caster_science = st.entities.get(&self_id).map(|e| e.science).unwrap_or(0) as f64;
                    let caster_resistance = st.entities.get(&self_id).map(|e| e.resistance).unwrap_or(0) as f64;
                    let val = match eff.id {
                        EffectType::BuffStrength => (base * (1.0 + caster_science / 100.0)).round() as i64,
                        EffectType::RelativeShield | EffectType::AbsoluteShield => {
                            (base * (1.0 + caster_resistance / 100.0)).round() as i64
                        }
                        _ => base.round() as i64,
                    };
                    if turns > 0 && val != 0 {
                        let stackable = (eff.modifiers & 1) != 0;
                        st.next_effect_instance_id += 1;
                        let inst = EffectInstance {
                            instance_id: st.next_effect_instance_id,
                            item_id: chip.item.as_i64(),
                            caster: self_id,
                            target: tid,
                            effect_id: eff.id,
                            value: val,
                            turns_left: turns,
                            modifiers: eff.modifiers,
                            from_weapon: false,
                        };
                        add_or_stack_effect(st, tid, inst, stackable);
                    }
                }
                EffectType::Debuff | EffectType::TotalDebuff => {
                    let turns = eff.turns.max(0);
                    let base = (eff.value1 + jet * eff.value2) * aoe_factor * critical_power;
                    let v = base.round() as i64;
                    if v > 0 {
                        // REDUCE_EFFECTS [306, target_id, value]
                        st.fight_actions.push(json!([306, tid, v]));
                        let pct = (v as f64) / 100.0;
                        let skip_irreducible = eff.id == EffectType::Debuff;
                        reduce_effects(st, tid, pct, skip_irreducible);
                    }
                    let _ = turns;
                }
                EffectType::AddState => {
                    let turns = eff.turns;
                    let state_id = eff.value1.round() as i64;
                    if turns != 0 && state_id > 0 {
                        st.next_effect_instance_id += 1;
                        let inst = EffectInstance {
                            instance_id: st.next_effect_instance_id,
                            item_id: chip.item.as_i64(),
                            caster: self_id,
                            target: tid,
                            effect_id: EffectType::AddState,
                            value: state_id,
                            turns_left: turns,
                            modifiers: eff.modifiers,
                            from_weapon: false,
                        };
                        let stackable = (eff.modifiers & 1) != 0;
                        add_or_stack_effect(st, tid, inst, stackable);
                    }
                }
                EffectType::Antidote => {
                    clear_poisons(st, tid);
                    // REMOVE_POISONS [307, target_id]
                    st.fight_actions.push(json!([307, tid]));
                }
                EffectType::RemoveShackles => {
                    remove_shackles(st, tid);
                    // REMOVE_SHACKLES [308, target_id]
                    st.fight_actions.push(json!([308, tid]));
                }
                _ => {}
            }
        }
    }
    1
}

fn add_or_stack_effect(st: &mut LeekWarsState, target_id: i64, inst: EffectInstance, stackable: bool) {
    let Some(t) = st.entities.get_mut(&target_id) else {
        return;
    };
    if stackable {
        if let Some(existing) = t.effects.iter_mut().find(|e| {
            e.effect_id == inst.effect_id
                && e.item_id == inst.item_id
                && e.caster == inst.caster
                && e.target == inst.target
                && e.turns_left == inst.turns_left
        }) {
            existing.value = existing.value.saturating_add(inst.value);
            st.fight_actions
                .push(json!([14, existing.instance_id, inst.value]));
            recompute_derived_buffs(t);
            return;
        }
    } else {
        t.effects
            .retain(|e| !(e.effect_id == inst.effect_id && e.item_id == inst.item_id));
    }

    st.fight_actions.push(effect_add_action(&inst));
    t.effects.push(inst);
    recompute_derived_buffs(t);
}

fn nf_get_register(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(VmError::BadArgCount {
            expected: 1,
            got: args.len(),
        });
    }
    vm.add_operations(30)?;
    let key = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    let self_id = ctx(vm)?.self_id;
    let v = with_state_mut(vm, |st, _| {
        let regs = ensure_registers_loaded(st, self_id);
        regs.get(key).map(|s| s.to_string())
    })?;
    Ok(v.map_or(Value::Null, |s| Value::String(s)))
}

fn nf_get_all_registers(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if !args.is_empty() {
        return Err(VmError::BadArgCount {
            expected: 0,
            got: args.len(),
        });
    }
    vm.add_operations(60)?;
    let self_id = ctx(vm)?.self_id;
    let pairs: Vec<(Value, Value)> = with_state_mut(vm, |st, _| {
        let regs = ensure_registers_loaded(st, self_id);
        regs.values()
            .iter()
            .map(|(k, v)| (Value::String(k.clone()), Value::String(v.clone())))
            .collect()
    })?;
    Ok(Value::Object(Rc::new(RefCell::new(pairs))))
}

fn nf_set_register(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(VmError::BadArgCount {
            expected: 2,
            got: args.len(),
        });
    }
    vm.add_operations(60)?;
    let key = match &args[0] {
        Value::String(s) => s.clone(),
        _ => return Err(VmError::ExpectedString),
    };
    let value = args[1].to_leek_coerce_string();
    let self_id = ctx(vm)?.self_id;
    let ok = with_state_mut(vm, |st, _| {
        let regs = ensure_registers_loaded(st, self_id);
        regs.set(key, value).is_ok()
    })?;
    Ok(Value::Bool(ok))
}

fn nf_delete_register(vm: &mut leekscript::vm::Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(VmError::BadArgCount {
            expected: 1,
            got: args.len(),
        });
    }
    vm.add_operations(30)?;
    let key = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    let self_id = ctx(vm)?.self_id;
    let _ = with_state_mut(vm, |st, _| {
        let regs = ensure_registers_loaded(st, self_id);
        regs.delete(key)
    })?;
    Ok(Value::Null)
}

static LEEKWARS_NATIVES: &[(&str, NativeFn)] = &[
    ("getEntity", nf_get_entity),
    ("getCell", nf_get_cell),
    ("getLife", nf_get_life),
    ("getStrength", nf_get_strength),
    ("getTP", nf_get_tp),
    ("getMP", nf_get_mp),
    ("getTeam", nf_get_team),
    ("getName", nf_get_name),
    ("say", nf_say),
    ("getWeapons", nf_get_weapons),
    ("getWeapon", nf_get_weapon),
    ("setWeapon", nf_set_weapon),
    ("getNearestEnemy", nf_get_nearest_enemy),
    ("moveToward", nf_move_toward),
    ("useWeapon", nf_use_weapon),
    ("useChip", nf_use_chip),
    ("getRegister", nf_get_register),
    ("getAllRegisters", nf_get_all_registers),
    ("setRegister", nf_set_register),
    ("deleteRegister", nf_delete_register),
    ("getCellX", nf_get_cell_x),
    ("getCellY", nf_get_cell_y),
    ("getCellFromXY", nf_get_cell_from_xy),
    ("getEntityOnCell", nf_get_entity_on_cell),
    ("getCellDistance", nf_get_cell_distance),
    ("getPathDistance", nf_get_path_distance),
];

/// Native id resolver that extends the stdlib native id space.
///
/// The Leek Wars native table is appended after `leekscript` stdlib natives.
pub fn native_id(name: &str) -> Option<u16> {
    if let Some(id) = leekscript::vm::stdlib::native_id(name) {
        return Some(id);
    }
    let local = LEEKWARS_NATIVES.iter().position(|(n, _)| *n == name)? as u16;
    Some(leekscript::vm::stdlib::stdlib_native_count().saturating_add(local))
}

pub fn default_natives() -> Vec<NativeFn> {
    let mut out = leekscript::vm::stdlib::default_natives();
    out.extend(LEEKWARS_NATIVES.iter().map(|(_, f)| *f));
    out
}

