use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use leekscript::vm::{NativeFn, Value, VmError};
use serde_json::json;

use crate::map::WorldMap;
use crate::persistence::{RegisterManagerRc, Registers};

use super::defs::*;
use super::types::*;

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

pub(crate) fn ctx<'a>(vm: &'a leekscript::vm::Vm) -> Result<&'a LeekWarsContext, VmError> {
    vm.host_ref::<LeekWarsContext>()
        .ok_or(VmError::MissingHost("LeekWarsContext"))
}

pub(crate) fn ctx_mut<'a>(
    vm: &'a mut leekscript::vm::Vm,
) -> Result<&'a mut LeekWarsContext, VmError> {
    vm.host_mut::<LeekWarsContext>()
        .ok_or(VmError::MissingHost("LeekWarsContext"))
}

pub(crate) fn with_state<R>(
    vm: &leekscript::vm::Vm,
    f: impl FnOnce(&LeekWarsState) -> R,
) -> Result<R, VmError> {
    let c = ctx(vm)?;
    Ok(f(&c.state.borrow()))
}

pub(crate) fn with_state_mut<R>(
    vm: &mut leekscript::vm::Vm,
    f: impl FnOnce(&mut LeekWarsState, i64) -> R,
) -> Result<R, VmError> {
    let self_id = ctx(vm)?.self_id;
    let c = ctx_mut(vm)?;
    Ok(f(&mut c.state.borrow_mut(), self_id))
}

pub(crate) fn cell_xy_i64(st: &LeekWarsState, cell: i64) -> Option<(i32, i32)> {
    let id = i32::try_from(cell).ok()?;
    let c = st.map.get_cell(id)?;
    Some((c.x, c.y))
}

pub(crate) fn cell_dist_i64(st: &LeekWarsState, a: i64, b: i64) -> i64 {
    let Ok(aa) = i32::try_from(a) else {
        return i64::MAX;
    };
    let Ok(bb) = i32::try_from(b) else {
        return i64::MAX;
    };
    st.map
        .case_distance(aa, bb)
        .map(|d| d as i64)
        .unwrap_or(i64::MAX)
}

pub(crate) fn spend_tp(st: &mut LeekWarsState, self_id: i64, amount: i64) {
    if let Some(me) = st.entities.get_mut(&self_id) {
        me.tp = (me.tp - amount).max(0);
    }
}

pub(crate) fn spend_mp(st: &mut LeekWarsState, self_id: i64, amount: i64) {
    if let Some(me) = st.entities.get_mut(&self_id) {
        me.mp = (me.mp - amount).max(0);
    }
}

include!("../leekwars_vm.rs");
