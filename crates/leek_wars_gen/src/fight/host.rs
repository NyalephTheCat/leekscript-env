use super::map;
use super::pathfinding;
use super::world::FightWorld;
use super::{apply_effects_on_cells, EffectContext};
use chrono::{Local, LocalResult, TimeZone};
use leekscript_run::{DebugLogHandled, DebugLogKind, InterpretError, InterpreterHost, Value};
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// Official generator: action ids (subset).
const ACTION_SUMMON: i32 = 9;
const ACTION_USE_CHIP: i32 = 12;
const ACTION_MOVE_TO: i32 = 10;
const ACTION_SET_WEAPON: i32 = 13;
const ACTION_USE_WEAPON: i32 = 16;
/// Official generator: `Action.SAY`
const ACTION_SAY: i32 = 203;
// Effect-related actions are logged in `effects.rs`.

/// Official generator: `Effect.TYPE_SUMMON`
const CHIP_EFFECT_SUMMON: i32 = 14;

// Official generator: `Attack.USE_*` result constants.
const USE_CRITICAL: i32 = 2;
const USE_SUCCESS: i32 = 1;
const USE_INVALID_TARGET: i32 = -1;
const USE_NOT_ENOUGH_TP: i32 = -2;
const USE_INVALID_POSITION: i32 = -4;
const USE_INVALID_COOLDOWN: i32 = -3;
const USE_TOO_MANY_SUMMONS: i32 = -5;
const USE_MAX_USES: i32 = -7;
/// Official generator: `Attack.USE_RESURRECT_INVALID_ENTIITY` (typo preserved in LW).
const USE_RESURRECT_INVALID_ENTITY: i32 = -6;
/// Official generator: `AI.ERROR_LOG_COST` (`EntityAI.addSystemLog` uses `opsNoCheck` with this value).
const JAVA_ERROR_LOG_COST: u64 = 10_000;
/// Official generator: `FarmerLog.NO_WEAPON_EQUIPPED` (system log key).
const FARMER_LOG_NO_WEAPON_EQUIPPED: i32 = 1000;
/// Official generator: `AILog.SWARNING` (remapped from `AILog.WARNING` in `EntityAI.addSystemLog`).
const AILOG_SWARNING: i32 = 7;
/// Official generator: `FightConstants.CHIP_RESURRECTION`
const CHIP_RESURRECTION: i32 = 84;
/// Official generator: `FightConstants.CHIP_AWEKENING` (awakening)
const CHIP_AWEKENING: i32 = 415;

/// Leek Wars fight natives backed by [`FightWorld`] (subset; grows toward parity with the official generator).
pub struct FightHost {
    world: Rc<RefCell<FightWorld>>,
    /// Official generator: `EntityAI.addSystemLog` → `opsNoCheck(ERROR_LOG_COST)` accumulated for the current `call_native`.
    native_dispatch_extra_ops: Cell<u64>,
}

impl FightHost {
    pub fn new(world: Rc<RefCell<FightWorld>>) -> Self {
        Self {
            world,
            native_dispatch_extra_ops: Cell::new(0),
        }
    }

    fn fight_local_datetime(w: &FightWorld) -> chrono::DateTime<Local> {
        match Local.timestamp_opt(w.fight_start_unix_secs, 0) {
            LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => dt,
            LocalResult::None => Local::now(),
        }
    }

    fn current_fid(&self) -> i32 {
        self.world.borrow().active_fid
    }

    /// Official generator: `WeaponClass.useWeapon` / `useWeaponOnCell` when `getWeapon() == null`: warning system log + `ERROR_LOG_COST` ops.
    fn charge_no_weapon_equipped_system_log(&self, fid: i32, trace: &str) {
        self.native_dispatch_extra_ops.set(
            self.native_dispatch_extra_ops
                .get()
                .saturating_add(JAVA_ERROR_LOG_COST),
        );
        let mut w = self.world.borrow_mut();
        let log_owner = w.entity(fid).map(|e| e.log_bucket_owner).unwrap_or(0);
        // Official generator: `EntityAI.addSystemLog(int, int)` passes `new String[0]` → FarmerLog includes `[]`.
        let empty_params: &[String] = &[];
        w.push_ai_system_log(
            log_owner,
            fid,
            AILOG_SWARNING,
            trace,
            FARMER_LOG_NO_WEAPON_EQUIPPED,
            Some(empty_params),
        );
    }

    fn verify_java_los(&self, start: i32, end: i32) -> bool {
        // Port of `com.leekwars.generator.maps.Map.verifyLoS(start, end, attack, ignoredCells)`
        // for the common case: needLos=true, ignoredCells=[start].
        let w = self.world.borrow();
        let mw = w.map_w;
        let mh = w.map_h;
        let (sx, sy) = map::cell_xy(mw, start);
        let (ex, ey) = map::cell_xy(mw, end);

        let a = (sy - ey).abs();
        let b = (sx - ex).abs();
        let dx = if sx > ex { -1 } else { 1 };
        let dy = if sy < ey { 1 } else { -1 };

        let mut path: Vec<i32> = Vec::with_capacity(((b + 1) * 2) as usize);
        if b == 0 {
            path.push(0);
            path.push(a + 1);
        } else {
            let d = (a as f64) / (b as f64) / 2.0;
            let mut h = 0i32;
            for i in 0..b {
                let y = 0.5 + ((i * 2 + 1) as f64) * d;
                path.push(h);
                path.push(((y - 0.00001).ceil() as i32) - h);
                h = (y + 0.00001).floor() as i32;
            }
            path.push(h);
            path.push(a + 1 - h);
        }

        for p in (0..path.len()).step_by(2) {
            let col = (p as i32) / 2;
            let start_y_offset = path[p];
            let count = path[p + 1];
            for i in 0..count {
                let cx = sx + col * dx;
                let cy = sy + (start_y_offset + i) * dy;
                let cell = map::cell_id_from_xy(mw, cx, cy);
                if !map::is_valid_cell(mw, mh, cell) {
                    return false;
                }
                if w.is_obstacle_cell(cell) {
                    return false;
                }
                if w.living_entity_on_cell(cell, None).is_some() {
                    // occupied start is ok; occupied end is ok
                    if cell == start {
                        continue;
                    }
                    if cell == end {
                        return true;
                    }
                    return false;
                }
            }
        }
        true
    }

    fn verify_java_range(
        &self,
        caster: i32,
        target: i32,
        launch_type_mask: i32,
        min_range: i32,
        max_range: i32,
    ) -> bool {
        let w = self.world.borrow();
        let mw = w.map_w;
        let (cx, cy) = map::cell_xy(mw, caster);
        let (tx, ty) = map::cell_xy(mw, target);
        let dx = cx - tx;
        let dy = cy - ty;
        let dist = dx.abs() + dy.abs();
        if dist > max_range || dist < min_range {
            return false;
        }
        if caster == target {
            return true;
        }
        // Official generator: `Map.verifyRange`:
        // if ((launchType & 1) == 0 && (dx == 0 || dy == 0)) return false; // line
        // if ((launchType & 2) == 0 && abs(dx) == abs(dy)) return false; // diagonal
        // if ((launchType & 4) == 0 && abs(dx) != abs(dy) && dx != 0 && dy != 0) return false; // rest
        if (launch_type_mask & 1) == 0 && (dx == 0 || dy == 0) {
            return false;
        }
        if (launch_type_mask & 2) == 0 && dx.abs() == dy.abs() {
            return false;
        }
        if (launch_type_mask & 4) == 0 && dx.abs() != dy.abs() && dx != 0 && dy != 0 {
            return false;
        }
        true
    }

    fn mask_circle_offsets(min: i32, max: i32) -> Vec<(i32, i32)> {
        if min > max {
            return Vec::new();
        }
        let mut out = Vec::new();
        if min == 0 {
            out.push((0, 0));
        }
        let start = if min < 1 { 1 } else { min };
        for size in start..=max {
            for i in 0..size {
                out.push((size - i, -i));
            }
            for i in 0..size {
                out.push((-i, -(size - i)));
            }
            for i in 0..size {
                out.push((-(size - i), i));
            }
            for i in 0..size {
                out.push((i, size - i));
            }
        }
        out
    }

    fn mask_plus_offsets(radius: i32) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        out.push((0, 0));
        for size in 1..=radius {
            out.push((size, 0));
            out.push((0, -size));
            out.push((-size, 0));
            out.push((0, size));
        }
        out
    }

    fn mask_x_offsets(radius: i32) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        out.push((0, 0));
        for size in 1..=radius {
            out.push((size, -size));
            out.push((-size, -size));
            out.push((-size, size));
            out.push((size, size));
        }
        out
    }

    fn mask_square_offsets(radius: i32) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        out.extend(Self::mask_circle_offsets(0, radius));
        for d in 0..radius {
            for i in 1..=radius - d {
                out.push((radius + 1 - i, -(d + i)));
            }
            for i in 1..=radius - d {
                out.push((-(d + i), -(radius + 1 - i)));
            }
            for i in 1..=radius - d {
                out.push((-(radius + 1 - i), d + i));
            }
            for i in 1..=radius - d {
                out.push((d + i, radius + 1 - i));
            }
        }
        out
    }

    fn mask_offsets_to_cells(w: &FightWorld, target_cell: i32, offsets: &[(i32, i32)]) -> Vec<i32> {
        let mw = w.map_w;
        let mh = w.map_h;
        let (x, y) = map::cell_xy(mw, target_cell);
        let mut out = Vec::new();
        for &(dx, dy) in offsets {
            let c = map::cell_id_from_xy(mw, x + dx, y + dy);
            if !map::is_valid_cell(mw, mh, c) {
                continue;
            }
            if w.is_obstacle_cell(c) {
                continue;
            }
            out.push(c);
        }
        out
    }

    fn min_distance2_to_cells(w: &FightWorld, from: i32, cells: &[i32]) -> Option<i32> {
        let mut best: Option<i32> = None;
        for &c in cells {
            if !map::is_valid_cell(w.map_w, w.map_h, c) {
                continue;
            }
            let d2 = map::distance2(w.map_w, from, c);
            if best.map(|b| d2 < b).unwrap_or(true) {
                best = Some(d2);
            }
        }
        best
    }

    /// Port of the official generator `Map.getPathAway(start, bad_cells, max_distance)` using our A* and mask offsets.
    fn path_away_from_cells(
        w: &FightWorld,
        start: i32,
        bad_cells: &[i32],
        max_distance: i32,
    ) -> Option<Vec<i32>> {
        if max_distance <= 0 || bad_cells.is_empty() {
            return None;
        }
        let cur_dist = Self::min_distance2_to_cells(w, start, bad_cells)?;

        let offsets = Self::mask_circle_offsets(1, max_distance);
        let candidates = Self::mask_offsets_to_cells(w, start, &offsets);
        let mut potential: Vec<(i32, i32)> = Vec::new(); // (dist2, cell)
        for c in candidates {
            // Official generator: `Cell.available(this)`
            if w.living_entity_on_cell(c, None).is_some() {
                continue;
            }
            let Some(d2) = Self::min_distance2_to_cells(w, c, bad_cells) else {
                continue;
            };
            if d2 > cur_dist {
                potential.push((d2, c));
            }
        }
        if potential.is_empty() {
            return None;
        }
        // Official generator: sorts by distance desc; tie-break deterministically by cell id asc.
        potential.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        for (_d2, cell) in potential {
            let path = pathfinding::get_path_between(w, start, cell, None)?;
            if !path.is_empty() && (path.len() as i32) <= max_distance {
                return Some(path);
            }
        }
        None
    }

    fn line_cells_extended(w: &FightWorld, cell1: i32, cell2: i32) -> Vec<i32> {
        let mw = w.map_w;
        let mh = w.map_h;
        if !map::is_valid_cell(mw, mh, cell1) || !map::is_valid_cell(mw, mh, cell2) {
            return Vec::new();
        }
        let (x1, y1) = map::cell_xy(mw, cell1);
        let (x2, y2) = map::cell_xy(mw, cell2);
        let dx = (x2 - x1).signum();
        let dy = (y2 - y1).signum();
        if dx == 0 && dy == 0 {
            return Vec::new();
        }
        let mut out: Vec<i32> = Vec::new();
        // Forward from cell1 (inclusive)
        let mut x = x1;
        let mut y = y1;
        loop {
            let c = map::cell_id_from_xy(mw, x, y);
            if !map::is_valid_cell(mw, mh, c) {
                break;
            }
            out.push(c);
            x += dx;
            y += dy;
        }
        // Backward from cell1 - step
        let mut x = x1 - dx;
        let mut y = y1 - dy;
        loop {
            let c = map::cell_id_from_xy(mw, x, y);
            if !map::is_valid_cell(mw, mh, c) {
                break;
            }
            out.push(c);
            x -= dx;
            y -= dy;
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Official generator: `Map.getPathTowardLine` goal cell list (forward chain then backward chain; order preserved).
    fn line_cells_astar_goals(w: &FightWorld, cell1: i32, cell2: i32) -> Vec<i32> {
        let mw = w.map_w;
        let mh = w.map_h;
        if !map::is_valid_cell(mw, mh, cell1) || !map::is_valid_cell(mw, mh, cell2) {
            return Vec::new();
        }
        let (x1, y1) = map::cell_xy(mw, cell1);
        let (x2, y2) = map::cell_xy(mw, cell2);
        let dx = (x2 - x1).signum();
        let dy = (y2 - y1).signum();
        if dx == 0 && dy == 0 {
            return Vec::new();
        }
        let mut out: Vec<i32> = Vec::new();
        let mut x = x1;
        let mut y = y1;
        loop {
            let c = map::cell_id_from_xy(mw, x, y);
            if !map::is_valid_cell(mw, mh, c) {
                break;
            }
            out.push(c);
            x += dx;
            y += dy;
        }
        x = x1 - dx;
        y = y1 - dy;
        loop {
            let c = map::cell_id_from_xy(mw, x, y);
            if !map::is_valid_cell(mw, mh, c) {
                break;
            }
            out.push(c);
            x -= dx;
            y -= dy;
        }
        out
    }

    fn area_target_cells(
        w: &FightWorld,
        area: i32,
        launch_cell: i32,
        target_cell: i32,
        min_range: i32,
        max_range: i32,
        need_los: bool,
    ) -> Vec<i32> {
        let mw = w.map_w;
        let mh = w.map_h;
        if !map::is_valid_cell(mw, mh, target_cell) {
            return Vec::new();
        }
        match area {
            1 => vec![target_cell],
            2 => {
                if !map::is_valid_cell(mw, mh, launch_cell) {
                    return Vec::new();
                }
                let (lx, ly) = map::cell_xy(mw, launch_cell);
                let (tx, ty) = map::cell_xy(mw, target_cell);
                let mut dx = 0;
                let mut dy = 0;
                if lx == tx {
                    dy = if ly > ty { -1 } else { 1 };
                } else if ly == ty {
                    dx = if lx > tx { -1 } else { 1 };
                } else {
                    return Vec::new();
                }
                let mut out = Vec::new();
                for i in min_range..=max_range {
                    let cx = lx + dx * i;
                    let cy = ly + dy * i;
                    let c = map::cell_id_from_xy(mw, cx, cy);
                    if !map::is_valid_cell(mw, mh, c) {
                        break;
                    }
                    if need_los && w.is_obstacle_cell(c) {
                        break;
                    }
                    out.push(c);
                }
                out
            }
            13 => {
                // Official generator: `Map.getFirstEntity(from, target, minRange, maxRange)`:
                // step by signum toward target until obstacle/out of range; return first entity cell in range.
                if !map::is_valid_cell(mw, mh, launch_cell) {
                    return Vec::new();
                }
                let (fx, fy) = map::cell_xy(mw, launch_cell);
                let (tx, ty) = map::cell_xy(mw, target_cell);
                let dx = (tx - fx).signum();
                let dy = (ty - fy).signum();
                if dx == 0 && dy == 0 {
                    return Vec::new();
                }
                let mut range = 1;
                while range <= max_range {
                    let cx = fx + dx * range;
                    let cy = fy + dy * range;
                    let c = map::cell_id_from_xy(mw, cx, cy);
                    if !map::is_valid_cell(mw, mh, c) {
                        break;
                    }
                    if w.is_obstacle_cell(c) {
                        break;
                    }
                    if range >= min_range && w.living_entity_on_cell(c, None).is_some() {
                        return vec![c];
                    }
                    range += 1;
                }
                Vec::new()
            }
            14 => {
                // Enemies: all enemy entity cells (caster required).
                let Some(me) = w.entity(w.active_fid) else {
                    return Vec::new();
                };
                w.entities
                    .iter()
                    .filter(|e| !e.dead && e.team != me.team)
                    .map(|e| e.cell)
                    .collect()
            }
            15 => {
                // Allies: all ally entity cells (caster required). Official generator skips entities whose name contains "crystal".
                let Some(me) = w.entity(w.active_fid) else {
                    return Vec::new();
                };
                w.entities
                    .iter()
                    .filter(|e| !e.dead && e.team == me.team && !e.name.contains("crystal"))
                    .map(|e| e.cell)
                    .collect()
            }
            3 => Self::mask_offsets_to_cells(w, target_cell, &Self::mask_circle_offsets(0, 1)),
            4 => Self::mask_offsets_to_cells(w, target_cell, &Self::mask_circle_offsets(0, 2)),
            5 => Self::mask_offsets_to_cells(w, target_cell, &Self::mask_circle_offsets(0, 3)),
            6 => Self::mask_offsets_to_cells(w, target_cell, &Self::mask_plus_offsets(2)),
            7 => Self::mask_offsets_to_cells(w, target_cell, &Self::mask_plus_offsets(3)),
            8 => Self::mask_offsets_to_cells(w, target_cell, &Self::mask_x_offsets(1)),
            9 => Self::mask_offsets_to_cells(w, target_cell, &Self::mask_x_offsets(2)),
            10 => Self::mask_offsets_to_cells(w, target_cell, &Self::mask_x_offsets(3)),
            11 => Self::mask_offsets_to_cells(w, target_cell, &Self::mask_square_offsets(1)),
            12 => Self::mask_offsets_to_cells(w, target_cell, &Self::mask_square_offsets(2)),
            _ => vec![target_cell],
        }
    }

    fn verify_java_los_with_ignored(
        &self,
        start: i32,
        end: i32,
        ignored_cells: &[i32],
    ) -> Option<bool> {
        // Like `verify_java_los`, but allows certain occupied cells to be treated as transparent
        // (official generator `verifyLoS(..., ignoredCells)`).
        let w = self.world.borrow();
        let mw = w.map_w;
        let mh = w.map_h;
        if !map::is_valid_cell(mw, mh, start) || !map::is_valid_cell(mw, mh, end) {
            return None;
        }
        let (sx, sy) = map::cell_xy(mw, start);
        let (ex, ey) = map::cell_xy(mw, end);

        let a = (sy - ey).abs();
        let b = (sx - ex).abs();
        let dx = if sx > ex { -1 } else { 1 };
        let dy = if sy < ey { 1 } else { -1 };

        let mut path: Vec<i32> = Vec::with_capacity(((b + 1) * 2) as usize);
        if b == 0 {
            path.push(0);
            path.push(a + 1);
        } else {
            let d = (a as f64) / (b as f64) / 2.0;
            let mut h = 0i32;
            for i in 0..b {
                let y = 0.5 + ((i * 2 + 1) as f64) * d;
                path.push(h);
                path.push(((y - 0.00001).ceil() as i32) - h);
                h = (y + 0.00001).floor() as i32;
            }
            path.push(h);
            path.push(a + 1 - h);
        }

        for p in (0..path.len()).step_by(2) {
            let col = (p as i32) / 2;
            let start_y_offset = path[p];
            let count = path[p + 1];
            for i in 0..count {
                let cx = sx + col * dx;
                let cy = sy + (start_y_offset + i) * dy;
                let cell = map::cell_id_from_xy(mw, cx, cy);
                if !map::is_valid_cell(mw, mh, cell) {
                    return Some(false);
                }
                if w.is_obstacle_cell(cell) {
                    return Some(false);
                }
                if w.living_entity_on_cell(cell, None).is_some() {
                    if cell == start {
                        continue;
                    }
                    if cell == end {
                        return Some(true);
                    }
                    if ignored_cells.contains(&cell) {
                        continue;
                    }
                    return Some(false);
                }
            }
        }
        Some(true)
    }

    fn generate_critical(w: &mut FightWorld, caster_fid: i32) -> bool {
        let agi = w.entity(caster_fid).map(|e| e.agility).unwrap_or(0) as f64;
        let p = (agi / 1000.0).clamp(0.0, 1.0);
        w.rng.next_double01() < p
    }

    fn chip_has_resurrect_effect(cs: &super::chips::ChipStats) -> bool {
        cs.effects.iter().any(|e| e.id == 15)
    }

    fn feature_array_from_effect(eff: &super::chips::ChipEffect) -> Value {
        // Official generator: `EntityAI.getFeatureArray(EffectParameters)` format:
        // [type, min, max, turns, targets, modifiers]
        let minv = eff.value1.round() as i64;
        let maxv = (eff.value1 + eff.value2).round() as i64;
        Value::array_from(vec![
            Value::Integer(eff.id as i64),
            Value::Integer(minv),
            Value::Integer(maxv),
            Value::Integer(eff.turns as i64),
            Value::Integer(eff.targets as i64),
            Value::Integer(eff.modifiers as i64),
        ])
    }

    fn chip_use_blocked_by_cooldown_or_cap(
        w: &FightWorld,
        caster_fid: i32,
        cs: &super::chips::ChipStats,
    ) -> Option<i64> {
        if w.chip_on_cooldown(caster_fid, cs) {
            return Some(USE_INVALID_COOLDOWN as i64);
        }
        if cs.max_uses >= 0 {
            let n = w
                .entity(caster_fid)
                .map(|e| *e.chip_uses_turn.get(&cs.chip_id).unwrap_or(&0))
                .unwrap_or(0);
            if n >= cs.max_uses {
                return Some(USE_MAX_USES as i64);
            }
        }
        None
    }

    /// Official generator: `Fight.useChip` / `State.summonEntity`: chips whose attack includes `TYPE_SUMMON` only summon (no `applyOnCell` effects).
    /// Returns `Some(exit value)` when this chip is handled as a summon; `None` to fall through to normal effect application.
    fn use_chip_if_summon(
        &self,
        w: &mut FightWorld,
        caster_fid: i32,
        target_cell: i32,
        cs: &super::chips::ChipStats,
        name_override: Option<String>,
    ) -> Option<i64> {
        let bulb_id = cs
            .effects
            .iter()
            .find(|e| e.id == CHIP_EFFECT_SUMMON)
            .map(|e| e.value1 as i32)?;
        if let Some(code) = Self::chip_use_blocked_by_cooldown_or_cap(w, caster_fid, cs) {
            return Some(code);
        }
        let team = w.entity(caster_fid)?.team;
        if w.team_summon_count(team) >= FightWorld::SUMMON_LIMIT {
            return Some(USE_TOO_MANY_SUMMONS as i64);
        }
        if !w.cell_available_for_summon(target_cell) {
            return Some(USE_INVALID_POSITION as i64);
        }
        if !w.summons_by_id.contains_key(&bulb_id) {
            return Some(USE_INVALID_TARGET as i64);
        }
        let critical = Self::generate_critical(w, caster_fid);
        let result = if critical { USE_CRITICAL } else { USE_SUCCESS };
        w.log_action(json!([
            ACTION_USE_CHIP,
            cs.template_id,
            target_cell,
            result
        ]));
        match w.summon_bulb(caster_fid, bulb_id, target_cell, critical) {
            Some(summon_fid) => {
                if let Some(name) = name_override {
                    if !name.is_empty() {
                        if let Some(s) = w.entity_mut(summon_fid) {
                            s.name = name;
                        }
                    }
                }
                w.log_action(json!([
                    ACTION_SUMMON,
                    caster_fid,
                    summon_fid,
                    target_cell,
                    result
                ]));
                if let Some(me) = w.entity_mut(caster_fid) {
                    me.tp = (me.tp - cs.cost).max(0);
                }
                w.register_chip_use_after_success(caster_fid, cs);
                Some(result as i64)
            }
            None => Some(USE_INVALID_POSITION as i64),
        }
    }
}

impl InterpreterHost for FightHost {
    fn call_native(
        &mut self,
        name: &str,
        args: &[Value],
        system_log_trace: Option<&str>,
    ) -> Result<Option<Value>, InterpretError> {
        self.native_dispatch_extra_ops.set(0);
        let trace = system_log_trace.unwrap_or("");
        let fid = self.current_fid();
        match name {
            "say" => {
                // Official generator: `EntityClass.say` + `ActionSay` `[203, message]`.
                const SAY_LIMIT: i32 = 2;
                const SAY_MAX_LEN: usize = 100;
                let msg_raw = args.get(0).map(value_debug_string).unwrap_or_default();
                let mut w = self.world.borrow_mut();
                let Some(ent) = w.entity_mut(fid) else {
                    return Ok(Some(Value::Bool(false)));
                };
                if ent.tp < 1 {
                    return Ok(Some(Value::Bool(false)));
                }
                if ent.says_turn >= SAY_LIMIT {
                    return Ok(Some(Value::Bool(false)));
                }
                ent.tp -= 1;
                ent.says_turn += 1;
                let mut message = msg_raw;
                if message.len() > SAY_MAX_LEN {
                    message.truncate(SAY_MAX_LEN);
                }
                let message = message.replace('\t', "    ");
                w.log_action(json!([ACTION_SAY, message.clone()]));
                let targets: Vec<i32> = w
                    .entities
                    .iter()
                    .filter(|e| !e.dead && e.fid != fid && !e.ai_path.is_empty())
                    .map(|e| e.fid)
                    .collect();
                for tfid in targets {
                    w.say_inbox
                        .entry(tfid)
                        .or_default()
                        .push((fid, message.clone()));
                }
                Ok(Some(Value::Bool(true)))
            }
            "sendAll" => {
                // Official generator: send message to all allies (team inbox). Message format: [author, type, params]
                let msg_type = args.get(0).and_then(value_as_i64).unwrap_or(0) as i32;
                let params = args.get(1).cloned().unwrap_or(Value::Null);
                let fids = {
                    let w = self.world.borrow();
                    let team = w.entity(fid).map(|e| e.team).unwrap_or(0);
                    w.entities
                        .iter()
                        .filter(|e| e.team == team && e.fid != fid && !e.ai_path.is_empty())
                        .map(|e| e.fid)
                        .collect::<Vec<_>>()
                };
                let msg = Value::array_from(vec![
                    Value::Integer(fid as i64),
                    Value::Integer(msg_type as i64),
                    params,
                ]);
                let mut w = self.world.borrow_mut();
                for tfid in fids {
                    w.inbox.entry(tfid).or_default().push(msg.clone());
                }
                Ok(Some(Value::Null))
            }
            "sendTo" => {
                let target = args.get(0).and_then(value_as_i64).unwrap_or(-1) as i32;
                let msg_type = args.get(1).and_then(value_as_i64).unwrap_or(0) as i32;
                let params = args.get(2).cloned().unwrap_or(Value::Null);
                if target == fid {
                    return Ok(Some(Value::Bool(false)));
                }
                let msg = Value::array_from(vec![
                    Value::Integer(fid as i64),
                    Value::Integer(msg_type as i64),
                    params,
                ]);
                let mut w = self.world.borrow_mut();
                let ok = w.entity(target).is_some_and(|e| !e.ai_path.is_empty());
                if ok {
                    w.inbox.entry(target).or_default().push(msg);
                }
                Ok(Some(Value::Bool(ok)))
            }
            "listen" => {
                let w = self.world.borrow();
                let rows = w.say_inbox.get(&fid).cloned().unwrap_or_default();
                let mut out = Vec::with_capacity(rows.len());
                for (author, msg) in rows {
                    out.push(Value::array_from(vec![
                        Value::Integer(author as i64),
                        Value::String(msg.into()),
                    ]));
                }
                Ok(Some(Value::array_from(out)))
            }
            "getMessages" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let msgs = w.inbox.get(&who).cloned().unwrap_or_default();
                Ok(Some(Value::array_from(msgs)))
            }
            "getMessageAuthor" => {
                let msg = args.get(0).or_else(|| args.get(1));
                let Some(Value::Array(arr)) = msg else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let a = arr.borrow();
                let v = a.get(0).and_then(value_as_i64).unwrap_or(-1);
                Ok(Some(Value::Integer(v)))
            }
            "getMessageType" => {
                let msg = args.get(0).or_else(|| args.get(1));
                let Some(Value::Array(arr)) = msg else {
                    return Ok(Some(Value::Integer(0)));
                };
                let a = arr.borrow();
                let v = a.get(1).and_then(value_as_i64).unwrap_or(0);
                Ok(Some(Value::Integer(v)))
            }
            "getMessageParams" => {
                let msg = args.get(0).or_else(|| args.get(1));
                let Some(Value::Array(arr)) = msg else {
                    return Ok(Some(Value::Null));
                };
                let a = arr.borrow();
                Ok(Some(a.get(2).cloned().unwrap_or(Value::Null)))
            }
            "pause" => Ok(Some(Value::Null)),
            "mark" => Ok(Some(Value::Bool(true))),
            "markText" => Ok(Some(Value::Bool(true))),
            "clearMarks" => Ok(Some(Value::Null)),
            "show" => Ok(Some(Value::Bool(true))),
            "setRegister" => {
                let key = args.get(0).map(value_debug_string).unwrap_or_default();
                let value = args.get(1).map(value_debug_string).unwrap_or_default();
                self.world.borrow_mut().registers.insert(key, value);
                Ok(Some(Value::Bool(true)))
            }
            "getRegister" => {
                let key = args.get(0).map(value_debug_string).unwrap_or_default();
                let w = self.world.borrow();
                let Some(v) = w.registers.get(&key) else {
                    return Ok(Some(Value::Null));
                };
                Ok(Some(Value::String(v.clone().into())))
            }
            "deleteRegister" => {
                let key = args.get(0).map(value_debug_string).unwrap_or_default();
                self.world.borrow_mut().registers.remove(&key);
                Ok(Some(Value::Null))
            }
            "getRegisters" => {
                let w = self.world.borrow();
                let mut keys: Vec<&String> = w.registers.keys().collect();
                keys.sort();
                let pairs = keys
                    .into_iter()
                    .map(|k| {
                        let v = w.registers.get(k).cloned().unwrap_or_default();
                        (Value::String(k.clone().into()), Value::String(v.into()))
                    })
                    .collect::<Vec<_>>();
                Ok(Some(Value::map_from(pairs)))
            }
            "getLife" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.life as i64).unwrap_or(0);
                Ok(Some(Value::Integer(v)))
            }
            "getType" => {
                // Official generator: `EntityClass.getType`: returns `entity.getType() + 1`.
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.entity_type + 1).unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getMobType" => {
                // Official generator: `EntityClass.getMobType`: skin if `TYPE_MOB`, else -1.
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = match w.entity(who) {
                    Some(e) if e.entity_type == 4 => e.skin, // Official generator: `Entity.TYPE_MOB`
                    _ => -1,
                };
                Ok(Some(Value::Integer(v as i64)))
            }
            "getAIId" | "getAIID" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.ai_id).unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getAIName" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.ai_name.clone()).unwrap_or_default();
                Ok(Some(Value::String(v.into())))
            }
            "getBirthTurn" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.birth_turn).unwrap_or(1);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getCores" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.cores).unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getRAM" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.ram).unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getFarmerId" | "getFarmerID" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.farmer_id).unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getFarmerName" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w
                    .entity(who)
                    .map(|e| e.farmer_name.clone())
                    .unwrap_or_default();
                Ok(Some(Value::String(v.into())))
            }
            "getFarmerCountry" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w
                    .entity(who)
                    .map(|e| e.farmer_country.clone())
                    .unwrap_or_else(|| "?".into());
                Ok(Some(Value::String(v.into())))
            }
            "getTeamName" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w
                    .entity(who)
                    .map(|e| e.team_name.clone())
                    .unwrap_or_default();
                Ok(Some(Value::String(v.into())))
            }
            "getStat" => {
                // Official generator: `Entity.getStat(stat)` where `stat` is a STAT_* constant.
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.len() < 2 {
                    return Err(InterpretError::invalid_parameter_count(2, user_args.len()));
                }
                let who = value_as_i64(&user_args[0]).unwrap_or(fid as i64) as i32;
                let stat = value_as_i64(&user_args[1]).unwrap_or(0) as i32;
                let w = self.world.borrow();
                let Some(e) = w.entity(who) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let v = match stat {
                    0 => e.total_life,       // STAT_LIFE
                    1 => e.total_tp,         // STAT_TP
                    2 => e.total_mp,         // STAT_MP
                    3 => e.strength,         // STAT_STRENGTH
                    4 => e.agility,          // STAT_AGILITY
                    5 => e.frequency,        // STAT_FREQUENCY
                    6 => e.wisdom,           // STAT_WISDOM
                    9 => e.absolute_shield,  // STAT_ABSOLUTE_SHIELD
                    10 => e.relative_shield, // STAT_RELATIVE_SHIELD
                    11 => e.resistance,      // STAT_RESISTANCE
                    12 => e.science,         // STAT_SCIENCE
                    13 => e.magic,           // STAT_MAGIC
                    14 => e.damage_return,   // STAT_DAMAGE_RETURN
                    15 => e.power,           // STAT_POWER
                    16 => e.cores,           // STAT_CORES
                    17 => e.ram,             // STAT_RAM
                    _ => 0,
                };
                Ok(Some(Value::Integer(v as i64)))
            }
            "getPower" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.power).unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getPassiveEffects" => {
                // Official generator: `Entity.getPassiveEffects()` includes weapon passive effects etc.
                // Not yet tracked in this port.
                Ok(Some(Value::array_from(Vec::new())))
            }
            "getCell" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.cell as i64).unwrap_or(-1);
                Ok(Some(Value::Integer(v)))
            }
            "getTP" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.tp as i64).unwrap_or(0);
                Ok(Some(Value::Integer(v)))
            }
            "getTotalTP" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.total_tp as i64).unwrap_or(0);
                Ok(Some(Value::Integer(v)))
            }
            "getMP" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.mp as i64).unwrap_or(0);
                Ok(Some(Value::Integer(v)))
            }
            "getTotalMP" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let v = w.entity(who).map(|e| e.total_mp as i64).unwrap_or(0);
                Ok(Some(Value::Integer(v)))
            }
            "getCellX" => {
                let c = int_user_arg(args, "getCellX")? as i32;
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, c) {
                    return Ok(Some(Value::Null));
                }
                let (ix, _) = map::cell_xy(w.map_w, c);
                Ok(Some(Value::Integer((ix - w.map_w + 1) as i64)))
            }
            "getCellY" => {
                let c = int_user_arg(args, "getCellY")? as i32;
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, c) {
                    return Ok(Some(Value::Null));
                }
                let (_, iy) = map::cell_xy(w.map_w, c);
                Ok(Some(Value::Integer(iy as i64)))
            }
            "getCellFromXY" => {
                let (ax, iy) = pair_user_ints(args, "getCellFromXY")?;
                let w = self.world.borrow();
                let ix = ax as i32 + w.map_w - 1;
                let id = map::cell_id_from_xy(w.map_w, ix, iy as i32);
                if !map::is_valid_cell(w.map_w, w.map_h, id) {
                    return Ok(Some(Value::Null));
                }
                Ok(Some(Value::Integer(id as i64)))
            }
            "getDistance" => {
                let (c1, c2) = pair_user_ints(args, "getDistance")?;
                let w = self.world.borrow();
                let c1 = c1 as i32;
                let c2 = c2 as i32;
                if !map::is_valid_cell(w.map_w, w.map_h, c1)
                    || !map::is_valid_cell(w.map_w, w.map_h, c2)
                {
                    return Ok(Some(Value::Real(-1.0)));
                }
                let d2 = map::distance2(w.map_w, c1, c2) as f64;
                Ok(Some(Value::Real(d2.sqrt())))
            }
            "getCellDistance" => {
                let (c1, c2) = pair_user_ints(args, "getCellDistance")?;
                let w = self.world.borrow();
                let c1 = c1 as i32;
                let c2 = c2 as i32;
                if !map::is_valid_cell(w.map_w, w.map_h, c1)
                    || !map::is_valid_cell(w.map_w, w.map_h, c2)
                {
                    return Ok(Some(Value::Integer(-1)));
                }
                let d = map::case_distance(w.map_w, c1, c2) as i64;
                Ok(Some(Value::Integer(d)))
            }
            "getEffects" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let Some(e) = w.entity(who) else {
                    return Ok(Some(Value::array_from(Vec::new())));
                };
                let effects = e
                    .effects
                    .iter()
                    .map(|eff| {
                        Value::array_from(vec![
                            Value::Integer(eff.id as i64),
                            Value::Integer(eff.value as i64),
                            Value::Integer(eff.caster_fid as i64),
                            Value::Integer(eff.turns as i64),
                            Value::Bool(eff.critical),
                            Value::Integer(eff.item_id as i64),
                            Value::Integer(who as i64),
                            Value::Integer(eff.modifiers as i64),
                        ])
                    })
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(effects)))
            }
            "getLaunchedEffects" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let launched = w
                    .entity(who)
                    .map(|e| e.launched_effect_log_ids.clone())
                    .unwrap_or_default();
                if launched.is_empty() {
                    return Ok(Some(Value::array_from(Vec::new())));
                }
                let mut out: Vec<Value> = Vec::new();
                for target in &w.entities {
                    if target.dead {
                        continue;
                    }
                    for eff in &target.effects {
                        if launched.contains(&eff.log_id) {
                            out.push(Value::array_from(vec![
                                Value::Integer(eff.id as i64),
                                Value::Integer(eff.value as i64),
                                Value::Integer(eff.caster_fid as i64),
                                Value::Integer(eff.turns as i64),
                                Value::Bool(eff.critical),
                                Value::Integer(eff.item_id as i64),
                                Value::Integer(target.fid as i64),
                                Value::Integer(eff.modifiers as i64),
                            ]));
                        }
                    }
                }
                Ok(Some(Value::array_from(out)))
            }
            "getStates" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let Some(e) = w.entity(who) else {
                    return Ok(Some(Value::set_from(Vec::new())));
                };
                let mut elems: Vec<Value> = Vec::new();
                for eff in &e.effects {
                    if eff.id == 59 {
                        if let Some(sid) = eff.state_id {
                            let v = Value::Integer(sid as i64);
                            if !elems.contains(&v) {
                                elems.push(v);
                            }
                        }
                    }
                }
                Ok(Some(Value::set_from(elems)))
            }
            "getEntityTurnOrder" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let pos = w
                    .turn_fids
                    .iter()
                    .position(|&x| x == who)
                    .map(|i| (i + 1) as i64)
                    .unwrap_or(0);
                Ok(Some(Value::Integer(pos)))
            }
            "getNextPlayer" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let n = w.turn_fids.len();
                if n == 0 {
                    return Ok(Some(Value::Integer(-1)));
                }
                let Some(i) = w.turn_fids.iter().position(|&x| x == who) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let next = w.turn_fids[(i + 1) % n] as i64;
                Ok(Some(Value::Integer(next)))
            }
            "getPreviousPlayer" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let n = w.turn_fids.len();
                if n == 0 {
                    return Ok(Some(Value::Integer(-1)));
                }
                let Some(i) = w.turn_fids.iter().position(|&x| x == who) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let prev = w.turn_fids[(i + n - 1) % n] as i64;
                Ok(Some(Value::Integer(prev)))
            }
            "getTurn" => {
                let w = self.world.borrow();
                Ok(Some(Value::Integer(w.active_turn as i64)))
            }
            "getPathLength" => {
                // Official generator: `FieldClass.getPathLength(startCell, endCell, cells_to_ignore?)`.
                let (c1, c2) = pair_user_ints(args, "getPathLength")?;
                let c1 = c1 as i32;
                let c2 = c2 as i32;
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, c1)
                    || !map::is_valid_cell(w.map_w, w.map_h, c2)
                {
                    return Ok(Some(Value::Null));
                }
                if c1 == c2 {
                    return Ok(Some(Value::Integer(0)));
                }
                // Optional ignore arg (3rd or 4th depending on implicit `this`): accept Array of cell ids.
                let ignore_cells: Vec<i32> = match args.len() {
                    n if n >= 4 => value_cells_vec(&args[3]),
                    3 => value_cells_vec(&args[2]),
                    _ => Vec::new(),
                };
                let path = pathfinding::get_path_between(
                    &w,
                    c1,
                    c2,
                    (!ignore_cells.is_empty()).then_some(&ignore_cells),
                );
                Ok(Some(match path {
                    None => Value::Null,
                    Some(p) => Value::Integer(p.len() as i64),
                }))
            }
            "getPath" => {
                // Official generator: `FieldClass.getPath(startCell, endCell, cells_to_ignore?)`.
                let (c1, c2) = pair_user_ints(args, "getPath")?;
                let c1 = c1 as i32;
                let c2 = c2 as i32;
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, c1)
                    || !map::is_valid_cell(w.map_w, w.map_h, c2)
                {
                    return Ok(Some(Value::Null));
                }
                if c1 == c2 {
                    return Ok(Some(Value::array_from(Vec::new())));
                }
                let ignore_cells: Vec<i32> = match args.len() {
                    n if n >= 4 => value_cells_vec(&args[3]),
                    3 => value_cells_vec(&args[2]),
                    _ => Vec::new(),
                };
                let path = pathfinding::get_path_between(
                    &w,
                    c1,
                    c2,
                    (!ignore_cells.is_empty()).then_some(&ignore_cells),
                );
                Ok(Some(match path {
                    None => Value::Null,
                    Some(p) => {
                        Value::array_from(p.into_iter().map(|c| Value::Integer(c as i64)).collect())
                    }
                }))
            }
            "isAlive" => {
                let who = int_user_arg(args, "isAlive")? as i32;
                let w = self.world.borrow();
                let ok = w.entity(who).is_some_and(|e| !e.dead);
                Ok(Some(Value::Bool(ok)))
            }
            "isSummon" => {
                let who = int_user_arg(args, "isSummon")? as i32;
                let w = self.world.borrow();
                let ok = w.entity(who).is_some_and(|e| e.is_summon);
                Ok(Some(Value::Bool(ok)))
            }
            "getSummoner" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who).and_then(|e| e.summoner_fid) {
                    Some(id) => Value::Integer(id as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "isDead" => {
                let who = int_user_arg(args, "isDead")? as i32;
                let w = self.world.borrow();
                let ok = w.entity(who).is_some_and(|e| e.dead);
                Ok(Some(Value::Bool(ok)))
            }
            "isStatic" => {
                let who = int_user_arg(args, "isStatic")? as i32;
                let w = self.world.borrow();
                let ok = w.entity(who).is_some_and(|e| {
                    e.effects
                        .iter()
                        .any(|eff| eff.id == 59 && eff.state_id == Some(11))
                });
                Ok(Some(Value::Bool(ok)))
            }
            "isEnemy" => {
                let who = int_user_arg(args, "isEnemy")? as i32;
                let w = self.world.borrow();
                let ok = match w.entity(fid) {
                    Some(me) => w.entity(who).is_some_and(|o| o.team != me.team),
                    None => false,
                };
                Ok(Some(Value::Bool(ok)))
            }
            "isAlly" => {
                let who = int_user_arg(args, "isAlly")? as i32;
                let w = self.world.borrow();
                let ok = match w.entity(fid) {
                    Some(me) => w.entity(who).is_some_and(|o| o.team == me.team),
                    None => false,
                };
                Ok(Some(Value::Bool(ok)))
            }
            "getLeek" | "getEntity" => Ok(Some(Value::Integer(fid as i64))),
            "getLeekID" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.leek_id as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getTeamID" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.team_id as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getSide" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.team as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getStrength" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.strength as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getForce" => {
                // Deprecated alias for strength.
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.strength as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getAgility" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.agility as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getWisdom" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.wisdom as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getResistance" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.resistance as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getScience" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.science as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getMagic" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.magic as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getAbsoluteShield" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.absolute_shield as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getRelativeShield" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.relative_shield as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getDamageReturn" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.damage_return as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getFrequency" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.frequency as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getItemMaxUses" => {
                let item = arg0_as_i64_strict(args, "getItemMaxUses")? as i32;
                let w = self.world.borrow();
                if let Some(cs) = w.chips_by_id.get(&item) {
                    return Ok(Some(Value::Integer(cs.max_uses as i64)));
                }
                Ok(Some(Value::Integer(-1)))
            }
            "getLevel" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.level as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getTotalLife" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::Integer(e.total_life as i64),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getName" => {
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = match w.entity(who) {
                    Some(e) => Value::String(e.name.clone()),
                    None => Value::Null,
                };
                Ok(Some(out))
            }
            "getCellContent" => {
                let c = int_user_arg(args, "getCellContent")? as i32;
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, c) {
                    return Ok(Some(Value::Integer(-1)));
                }
                // Official generator: `FieldClass.getCellContent`: 2=obstacle, 1=entity, 0=empty
                let v = if w.is_obstacle_cell(c) {
                    2
                } else if w.living_entity_on_cell(c, None).is_some() {
                    1
                } else {
                    0
                };
                Ok(Some(Value::Integer(v)))
            }
            "getObstacles" => {
                let w = self.world.borrow();
                let cells = w
                    .obstacles
                    .keys()
                    .map(|&c| Value::Integer(c as i64))
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(cells)))
            }
            "getMapType" => {
                // Official generator: `FieldClass.getMapType`: map.getType() + 2 (Nexus is -1 so it's +2).
                let w = self.world.borrow();
                Ok(Some(Value::Integer((w.map_type + 2) as i64)))
            }
            "getFightBoss" => {
                let w = self.world.borrow();
                Ok(Some(Value::Integer(w.fight_boss as i64)))
            }
            "getFightContext" => {
                let w = self.world.borrow();
                Ok(Some(Value::Integer(w.fight_context as i64)))
            }
            "getFightId" => {
                let w = self.world.borrow();
                Ok(Some(Value::Integer(w.fight_id as i64)))
            }
            "getFightType" => {
                let w = self.world.borrow();
                Ok(Some(Value::Integer(w.fight_type as i64)))
            }
            "getDate" => {
                let w = self.world.borrow();
                let dt = Self::fight_local_datetime(&w);
                Ok(Some(Value::String(
                    format!("{}", dt.format("%d/%m/%Y")).into(),
                )))
            }
            "getTime" => {
                let w = self.world.borrow();
                let dt = Self::fight_local_datetime(&w);
                Ok(Some(Value::String(
                    format!("{}", dt.format("%H:%M:%S")).into(),
                )))
            }
            "getTimestamp" => {
                let w = self.world.borrow();
                Ok(Some(Value::Integer(w.fight_start_unix_secs)))
            }
            "include" => {
                // Official generator: loads another AI source at relative path; not supported in this Rust port.
                let _path = args.get(0).map(value_debug_string).unwrap_or_default();
                Ok(Some(Value::Null))
            }
            "weaponNeedLos" => {
                // Official generator: `WeaponClass.weaponNeedLos(id=-1 uses equipped)`.
                let wid = arg0_as_i64_loose(args, "weaponNeedLos")? as i32;
                let w = self.world.borrow();
                let item_id = if wid == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    wid
                };
                let need = w
                    .weapons_by_item
                    .get(&item_id)
                    .map(|ws| ws.los)
                    .unwrap_or(false);
                Ok(Some(Value::Bool(need)))
            }
            "getChips" => {
                // Official generator: `FieldClass.getChips()`: chips of the current entity.
                let chips = {
                    let w = self.world.borrow();
                    let e = w.entity(fid).ok_or_else(|| InterpretError {
                        reference: "INTERNAL_ERROR",
                        message: "entity missing".into(),
                    })?;
                    e.chips
                        .iter()
                        .map(|&i| Value::Integer(i as i64))
                        .collect::<Vec<_>>()
                };
                Ok(Some(Value::array_from(chips)))
            }
            "getAllChips" => {
                let w = self.world.borrow();
                let mut ids: Vec<i32> = w.chips_by_id.keys().copied().collect();
                ids.sort_unstable();
                Ok(Some(Value::array_from(
                    ids.into_iter()
                        .map(|id| Value::Integer(id as i64))
                        .collect(),
                )))
            }
            "getBulbStats" => {
                // Official generator: `FightClass.getBulbStats(chipId)` → map STAT_* -> [min,max]
                let chip_id = arg0_as_i64_strict(args, "getBulbStats")? as i32;
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&chip_id) else {
                    return Ok(Some(Value::Null));
                };
                let Some(bulb_id) = cs
                    .effects
                    .iter()
                    .find(|e| e.id == CHIP_EFFECT_SUMMON)
                    .map(|e| e.value1 as i32)
                else {
                    return Ok(Some(Value::Null));
                };
                let Some(tpl) = w.summons_by_id.get(&bulb_id) else {
                    return Ok(Some(Value::Null));
                };
                let pairs = vec![
                    (
                        Value::Integer(0),
                        Value::array_from(vec![
                            Value::Integer(tpl.life.0 as i64),
                            Value::Integer(tpl.life.1 as i64),
                        ]),
                    ),
                    (
                        Value::Integer(1),
                        Value::array_from(vec![
                            Value::Integer(tpl.tp.0 as i64),
                            Value::Integer(tpl.tp.1 as i64),
                        ]),
                    ),
                    (
                        Value::Integer(2),
                        Value::array_from(vec![
                            Value::Integer(tpl.mp.0 as i64),
                            Value::Integer(tpl.mp.1 as i64),
                        ]),
                    ),
                    (
                        Value::Integer(3),
                        Value::array_from(vec![
                            Value::Integer(tpl.strength.0 as i64),
                            Value::Integer(tpl.strength.1 as i64),
                        ]),
                    ),
                    (
                        Value::Integer(4),
                        Value::array_from(vec![
                            Value::Integer(tpl.agility.0 as i64),
                            Value::Integer(tpl.agility.1 as i64),
                        ]),
                    ),
                    (
                        Value::Integer(6),
                        Value::array_from(vec![
                            Value::Integer(tpl.wisdom.0 as i64),
                            Value::Integer(tpl.wisdom.1 as i64),
                        ]),
                    ),
                    (
                        Value::Integer(11),
                        Value::array_from(vec![
                            Value::Integer(tpl.resistance.0 as i64),
                            Value::Integer(tpl.resistance.1 as i64),
                        ]),
                    ),
                    (
                        Value::Integer(12),
                        Value::array_from(vec![
                            Value::Integer(tpl.science.0 as i64),
                            Value::Integer(tpl.science.1 as i64),
                        ]),
                    ),
                    (
                        Value::Integer(13),
                        Value::array_from(vec![
                            Value::Integer(tpl.magic.0 as i64),
                            Value::Integer(tpl.magic.1 as i64),
                        ]),
                    ),
                ];
                Ok(Some(Value::map_from(pairs)))
            }
            "getBulbCharacteristics" => {
                // Deprecated alias.
                let v = self.call_native("getBulbStats", args, None)?;
                Ok(v)
            }
            "getBulbChips" => {
                let chip_id = arg0_as_i64_strict(args, "getBulbChips")? as i32;
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&chip_id) else {
                    return Ok(Some(Value::Null));
                };
                let Some(bulb_id) = cs
                    .effects
                    .iter()
                    .find(|e| e.id == CHIP_EFFECT_SUMMON)
                    .map(|e| e.value1 as i32)
                else {
                    return Ok(Some(Value::Null));
                };
                let Some(tpl) = w.summons_by_id.get(&bulb_id) else {
                    return Ok(Some(Value::Null));
                };
                Ok(Some(Value::array_from(
                    tpl.chips
                        .iter()
                        .copied()
                        .map(|c| Value::Integer(c as i64))
                        .collect(),
                )))
            }
            "getBulbType" => {
                let who = arg0_as_i64_strict(args, "getBulbType")? as i32;
                let w = self.world.borrow();
                let Some(e) = w.entity(who) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                if !e.is_summon {
                    return Ok(Some(Value::Integer(-1)));
                }
                // In this port, summoned bulbs have `leek_id = -bulb_template_id`.
                Ok(Some(Value::Integer((-e.leek_id) as i64)))
            }
            "getAllies" => {
                // Official generator: `FightClass.getAllies`: team entities, including dead.
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::array_from(Vec::new())));
                };
                let team = me.team;
                let out = w
                    .entities
                    .iter()
                    .filter(|e| e.team == team)
                    .map(|e| Value::Integer(e.fid as i64))
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(out)))
            }
            "getAlliesCount" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let team = me.team;
                let n = w.entities.iter().filter(|e| e.team == team).count() as i64;
                Ok(Some(Value::Integer(n)))
            }
            "getAliveAllies" => {
                // Official generator: `FightClass.getAliveAllies`: team entities, alive only.
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::array_from(Vec::new())));
                };
                let team = me.team;
                let out = w
                    .entities
                    .iter()
                    .filter(|e| e.team == team && !e.dead)
                    .map(|e| Value::Integer(e.fid as i64))
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(out)))
            }
            "getAliveAlliesCount" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let team = me.team;
                let n = w
                    .entities
                    .iter()
                    .filter(|e| e.team == team && !e.dead)
                    .count() as i64;
                Ok(Some(Value::Integer(n)))
            }
            "getDeadAllies" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::array_from(Vec::new())));
                };
                let team = me.team;
                let out = w
                    .entities
                    .iter()
                    .filter(|e| e.team == team && e.dead)
                    .map(|e| Value::Integer(e.fid as i64))
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(out)))
            }
            "getDeadAlliesCount" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let team = me.team;
                let n = w
                    .entities
                    .iter()
                    .filter(|e| e.team == team && e.dead)
                    .count() as i64;
                Ok(Some(Value::Integer(n)))
            }
            "getAlliesLife" => {
                // Official generator: sums alive team entities' current life.
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let team = me.team;
                let life: i64 = w
                    .entities
                    .iter()
                    .filter(|e| e.team == team && !e.dead)
                    .map(|e| e.life as i64)
                    .sum();
                Ok(Some(Value::Integer(life)))
            }
            "getEnemies" => {
                // Official generator: `FightClass.getEnemies`: enemies, including dead.
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::array_from(Vec::new())));
                };
                let team = me.team;
                let out = w
                    .entities
                    .iter()
                    .filter(|e| e.team != team)
                    .map(|e| Value::Integer(e.fid as i64))
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(out)))
            }
            "getEnemiesCount" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let team = me.team;
                let n = w.entities.iter().filter(|e| e.team != team).count() as i64;
                Ok(Some(Value::Integer(n)))
            }
            "getAliveEnemies" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::array_from(Vec::new())));
                };
                let team = me.team;
                let out = w
                    .entities
                    .iter()
                    .filter(|e| e.team != team && !e.dead)
                    .map(|e| Value::Integer(e.fid as i64))
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(out)))
            }
            "getAliveEnemiesCount" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let team = me.team;
                let n = w
                    .entities
                    .iter()
                    .filter(|e| e.team != team && !e.dead)
                    .count() as i64;
                Ok(Some(Value::Integer(n)))
            }
            "getDeadEnemies" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::array_from(Vec::new())));
                };
                let team = me.team;
                let out = w
                    .entities
                    .iter()
                    .filter(|e| e.team != team && e.dead)
                    .map(|e| Value::Integer(e.fid as i64))
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(out)))
            }
            "getDeadEnemiesCount" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let team = me.team;
                let n = w
                    .entities
                    .iter()
                    .filter(|e| e.team != team && e.dead)
                    .count() as i64;
                Ok(Some(Value::Integer(n)))
            }
            "getEnemiesLife" => {
                // Official generator: sums alive enemies' current life.
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let team = me.team;
                let life: i64 = w
                    .entities
                    .iter()
                    .filter(|e| e.team != team && !e.dead)
                    .map(|e| e.life as i64)
                    .sum();
                Ok(Some(Value::Integer(life)))
            }
            "getSummons" => {
                // Official generator: `EntityClass.getSummons(entity?)`: living summons only.
                let who = entity_arg_or_current(args, fid)?;
                let w = self.world.borrow();
                let out = w
                    .entities
                    .iter()
                    .filter(|e| e.is_summon && !e.dead && e.summoner_fid == Some(who))
                    .map(|e| Value::Integer(e.fid as i64))
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(out)))
            }
            "getAlliedTurret" => {
                // Official generator: return allied turret fid or null.
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Null));
                };
                let team = me.team;
                let turret = w
                    .entities
                    .iter()
                    .find(|e| !e.dead && e.team == team && e.entity_type == 2)
                    .map(|e| Value::Integer(e.fid as i64))
                    .unwrap_or(Value::Null);
                Ok(Some(turret))
            }
            "getEnemyTurret" => {
                // Official generator: return enemy turret fid or null.
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Null));
                };
                let team = me.team;
                let turret = w
                    .entities
                    .iter()
                    .find(|e| !e.dead && e.team != team && e.entity_type == 2)
                    .map(|e| Value::Integer(e.fid as i64))
                    .unwrap_or(Value::Null);
                Ok(Some(turret))
            }
            "getNearestAlly" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let from = me.cell;
                let team = me.team;
                let mut best: Option<(i32, i32)> = None; // (d2, fid)
                for e in &w.entities {
                    if e.dead || e.team != team || e.fid == fid {
                        continue;
                    }
                    let d2 = map::distance2(w.map_w, from, e.cell);
                    if best.map(|(bd, _)| d2 < bd).unwrap_or(true) {
                        best = Some((d2, e.fid));
                    }
                }
                Ok(Some(Value::Integer(
                    best.map(|(_, id)| id as i64).unwrap_or(-1),
                )))
            }
            "getNearestAllyTo" => {
                let who = arg0_as_i64_strict(args, "getNearestAllyTo")? as i32;
                let w = self.world.borrow();
                let Some(src) = w.entity(who) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let from = src.cell;
                let team = src.team;
                let mut best: Option<(i32, i32)> = None;
                for e in &w.entities {
                    if e.dead || e.team != team || e.fid == who {
                        continue;
                    }
                    let d2 = map::distance2(w.map_w, from, e.cell);
                    if best.map(|(bd, _)| d2 < bd).unwrap_or(true) {
                        best = Some((d2, e.fid));
                    }
                }
                Ok(Some(Value::Integer(
                    best.map(|(_, id)| id as i64).unwrap_or(-1),
                )))
            }
            "getNearestAllyToCell" => {
                let cell = arg0_as_i64_strict(args, "getNearestAllyToCell")? as i32;
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, cell) {
                    return Ok(Some(Value::Integer(-1)));
                }
                let team = me.team;
                let mut best: Option<(i32, i32)> = None;
                for e in &w.entities {
                    if e.dead || e.team != team {
                        continue;
                    }
                    let d2 = map::distance2(w.map_w, cell, e.cell);
                    if best.map(|(bd, _)| d2 < bd).unwrap_or(true) {
                        best = Some((d2, e.fid));
                    }
                }
                Ok(Some(Value::Integer(
                    best.map(|(_, id)| id as i64).unwrap_or(-1),
                )))
            }
            "getNearestEnemy" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let from = me.cell;
                let team = me.team;
                let mut best: Option<(i32, i32)> = None;
                for e in &w.entities {
                    if e.dead || e.team == team {
                        continue;
                    }
                    let d2 = map::distance2(w.map_w, from, e.cell);
                    if best.map(|(bd, _)| d2 < bd).unwrap_or(true) {
                        best = Some((d2, e.fid));
                    }
                }
                Ok(Some(Value::Integer(
                    best.map(|(_, id)| id as i64).unwrap_or(-1),
                )))
            }
            "getNearestEnemyTo" => {
                let who = arg0_as_i64_strict(args, "getNearestEnemyTo")? as i32;
                let w = self.world.borrow();
                let Some(src) = w.entity(who) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let from = src.cell;
                let team = src.team;
                let mut best: Option<(i32, i32)> = None;
                for e in &w.entities {
                    if e.dead || e.team == team {
                        continue;
                    }
                    let d2 = map::distance2(w.map_w, from, e.cell);
                    if best.map(|(bd, _)| d2 < bd).unwrap_or(true) {
                        best = Some((d2, e.fid));
                    }
                }
                Ok(Some(Value::Integer(
                    best.map(|(_, id)| id as i64).unwrap_or(-1),
                )))
            }
            "getNearestEnemyToCell" => {
                let cell = arg0_as_i64_strict(args, "getNearestEnemyToCell")? as i32;
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, cell) {
                    return Ok(Some(Value::Integer(-1)));
                }
                let team = me.team;
                let mut best: Option<(i32, i32)> = None;
                for e in &w.entities {
                    if e.dead || e.team == team {
                        continue;
                    }
                    let d2 = map::distance2(w.map_w, cell, e.cell);
                    if best.map(|(bd, _)| d2 < bd).unwrap_or(true) {
                        best = Some((d2, e.fid));
                    }
                }
                Ok(Some(Value::Integer(
                    best.map(|(_, id)| id as i64).unwrap_or(-1),
                )))
            }
            "getFarthestAlly" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let from = me.cell;
                let team = me.team;
                let mut best: Option<(i32, i32)> = None; // (d2, fid)
                for e in &w.entities {
                    if e.dead || e.team != team || e.fid == fid {
                        continue;
                    }
                    let d2 = map::distance2(w.map_w, from, e.cell);
                    if best.map(|(bd, _)| d2 > bd).unwrap_or(true) {
                        best = Some((d2, e.fid));
                    }
                }
                Ok(Some(Value::Integer(
                    best.map(|(_, id)| id as i64).unwrap_or(-1),
                )))
            }
            "getFarthestEnemy" => {
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let from = me.cell;
                let team = me.team;
                let mut best: Option<(i32, i32)> = None; // (d2, fid)
                for e in &w.entities {
                    if e.dead || e.team == team {
                        continue;
                    }
                    let d2 = map::distance2(w.map_w, from, e.cell);
                    if best.map(|(bd, _)| d2 > bd).unwrap_or(true) {
                        best = Some((d2, e.fid));
                    }
                }
                Ok(Some(Value::Integer(
                    best.map(|(_, id)| id as i64).unwrap_or(-1),
                )))
            }
            "chipNeedLos" => {
                // Official generator: `ChipClass.chipNeedLos(id)`.
                let cid = arg0_as_i64_strict(args, "chipNeedLos")? as i32;
                let w = self.world.borrow();
                let need = w.chips_by_id.get(&cid).map(|cs| cs.los).unwrap_or(false);
                Ok(Some(Value::Bool(need)))
            }
            "getChipCost" => {
                let cid = arg0_as_i64_strict(args, "getChipCost")? as i32;
                let w = self.world.borrow();
                let v = w.chips_by_id.get(&cid).map(|cs| cs.cost).unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getChipName" => {
                let cid = arg0_as_i64_strict(args, "getChipName")? as i32;
                let w = self.world.borrow();
                let v = w
                    .chips_by_id
                    .get(&cid)
                    .map(|cs| cs.name.clone())
                    .unwrap_or_default();
                Ok(Some(Value::String(v.into())))
            }
            "getChipCooldown" => {
                let cid = arg0_as_i64_strict(args, "getChipCooldown")? as i32;
                let w = self.world.borrow();
                let v = w.chips_by_id.get(&cid).map(|cs| cs.cooldown).unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getChipMaxUses" => {
                let cid = arg0_as_i64_strict(args, "getChipMaxUses")? as i32;
                let w = self.world.borrow();
                let v = w.chips_by_id.get(&cid).map(|cs| cs.max_uses).unwrap_or(-1);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getChipEffects" => {
                let cid = arg0_as_i64_strict(args, "getChipEffects")? as i32;
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&cid) else {
                    return Ok(Some(Value::Null));
                };
                let arr = cs
                    .effects
                    .iter()
                    .map(Self::feature_array_from_effect)
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(arr)))
            }
            "getChipFailure" => Ok(Some(Value::Integer(0))),
            "getChipFailureRate" => {
                // Not modeled in generator chip JSON; matches deprecated zero failure surface.
                Ok(Some(Value::Integer(0)))
            }
            "isInlineChip" => {
                let cid = arg0_as_i64_strict(args, "isInlineChip")? as i32;
                let w = self.world.borrow();
                let launch = w
                    .chips_by_id
                    .get(&cid)
                    .map(|cs| cs.launch_type)
                    .unwrap_or(0);
                Ok(Some(Value::Bool(launch == 1)))
            }
            "isChip" => {
                let v = arg0_as_i64_strict(args, "isChip")? as i32;
                let w = self.world.borrow();
                Ok(Some(Value::Bool(w.chips_by_id.contains_key(&v))))
            }
            "getChipEffectiveArea" => {
                // Official generator: `ChipClass.getChipEffectiveArea(chip, targetCell, fromCell?)`
                let cid = arg0_as_i64_strict(args, "getChipEffectiveArea")? as i32;
                let (target_cell, from_cell) = match args.len() {
                    n if n >= 4 => (
                        value_as_i64(args.get(2).unwrap()).unwrap_or(-1) as i32,
                        value_as_i64(args.get(3).unwrap()).unwrap_or(-1) as i32,
                    ),
                    3 => (
                        value_as_i64(args.get(1).unwrap()).unwrap_or(-1) as i32,
                        value_as_i64(args.get(2).unwrap()).unwrap_or(-1) as i32,
                    ),
                    _ => {
                        let target = value_as_i64(args.get(1).unwrap()).unwrap_or(-1) as i32;
                        let w = self.world.borrow();
                        let from = w.entity(fid).map(|e| e.cell).unwrap_or(-1);
                        (target, from)
                    }
                };
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&cid) else {
                    return Ok(Some(Value::Null));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, target_cell) {
                    return Ok(Some(Value::Null));
                }
                if !map::is_valid_cell(w.map_w, w.map_h, from_cell) {
                    return Ok(Some(Value::Null));
                }
                let cells = Self::area_target_cells(
                    &w,
                    cs.area,
                    from_cell,
                    target_cell,
                    cs.min_range,
                    cs.max_range,
                    cs.los,
                );
                Ok(Some(Value::array_from(
                    cells
                        .into_iter()
                        .map(|c| Value::Integer(c as i64))
                        .collect(),
                )))
            }
            "getChipTargets" => {
                // Official generator: `ChipClass.getChipTargets(chip, cell)` → entities in the attack area on that cell.
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.len() < 2 {
                    return Err(InterpretError::invalid_parameter_count(2, user_args.len()));
                }
                let cid = value_as_i64(&user_args[0]).unwrap_or(-1) as i32;
                let cell = value_as_i64(&user_args[1]).unwrap_or(-1) as i32;
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&cid) else {
                    return Ok(Some(Value::Null));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, cell) || w.is_obstacle_cell(cell) {
                    return Ok(Some(Value::Null));
                }
                let launch_cell = w.entity(fid).map(|e| e.cell).unwrap_or(cell);
                let cells = Self::area_target_cells(
                    &w,
                    cs.area,
                    launch_cell,
                    cell,
                    cs.min_range,
                    cs.max_range,
                    cs.los,
                );
                let mut out: Vec<Value> = Vec::new();
                for c in cells {
                    if let Some(tfid) = w.living_entity_on_cell(c, None) {
                        if !out.contains(&Value::Integer(tfid as i64)) {
                            out.push(Value::Integer(tfid as i64));
                        }
                    }
                }
                Ok(Some(Value::array_from(out)))
            }
            "getCooldown" => {
                // Official generator: `FightClass.getCooldown(chip, entity)` returns the current remaining cooldown.
                let chip = arg0_as_i64_strict(args, "getCooldown")? as i32;
                let entity = match args.len() {
                    n if n >= 2 => value_as_i64(args.get(1).unwrap()).unwrap_or(fid as i64) as i32,
                    _ => fid,
                };
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&chip) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let v = if cs.team_cooldown {
                    let team = w.entity(entity).map(|e| e.team).unwrap_or(-1);
                    if team < 0 {
                        0
                    } else {
                        w.team_chip_cooldowns
                            .get(team as usize)
                            .and_then(|m| m.get(&chip))
                            .copied()
                            .unwrap_or(0)
                    }
                } else {
                    w.entity(entity)
                        .and_then(|e| e.chip_cooldowns.get(&chip).copied())
                        .unwrap_or(0)
                };
                Ok(Some(Value::Integer(v as i64)))
            }
            "getCurrentCooldown" => {
                // Alias used by some legacy AIs; same semantics as `getCooldown`.
                let chip = arg0_as_i64_strict(args, "getCurrentCooldown")? as i32;
                let entity = match args.len() {
                    n if n >= 2 => value_as_i64(args.get(1).unwrap()).unwrap_or(fid as i64) as i32,
                    _ => fid,
                };
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&chip) else {
                    return Ok(Some(Value::Integer(0)));
                };
                let v = if cs.team_cooldown {
                    let team = w.entity(entity).map(|e| e.team).unwrap_or(-1);
                    if team < 0 {
                        0
                    } else {
                        w.team_chip_cooldowns
                            .get(team as usize)
                            .and_then(|m| m.get(&chip))
                            .copied()
                            .unwrap_or(0)
                    }
                } else {
                    w.entity(entity)
                        .and_then(|e| e.chip_cooldowns.get(&chip).copied())
                        .unwrap_or(0)
                };
                Ok(Some(Value::Integer(v as i64)))
            }
            "getItemUses" => {
                // Official generator: `Entity.getItemUses(itemID)` counts uses *this turn*.
                // We currently track per-entity chip uses; other item types return 0.
                let item = arg0_as_i64_strict(args, "getItemUses")? as i32;
                let entity = match args.len() {
                    n if n >= 2 => value_as_i64(args.get(1).unwrap()).unwrap_or(fid as i64) as i32,
                    _ => fid,
                };
                let w = self.world.borrow();
                let v = w
                    .entity(entity)
                    .and_then(|e| e.chip_uses_turn.get(&item).copied())
                    .unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getWeaponTargets" => {
                // Official generator: `WeaponClass.getWeaponTargets(weapon?, cell)` → entities in the weapon area.
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.is_empty() {
                    return Err(InterpretError::invalid_parameter_count(1, user_args.len()));
                }
                let (weapon, cell) = if user_args.len() >= 2 {
                    (
                        value_as_i64(&user_args[0]).unwrap_or(-1) as i32,
                        value_as_i64(&user_args[1]).unwrap_or(-1) as i32,
                    )
                } else {
                    (-1, value_as_i64(&user_args[0]).unwrap_or(-1) as i32)
                };
                let w = self.world.borrow();
                let weapon_item = if weapon == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    weapon
                };
                let Some(ws) = w.weapons_by_item.get(&weapon_item) else {
                    return Ok(Some(Value::Null));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, cell) || w.is_obstacle_cell(cell) {
                    return Ok(Some(Value::Null));
                }
                let launch_cell = w.entity(fid).map(|e| e.cell).unwrap_or(cell);
                let cells = Self::area_target_cells(&w, ws.area, launch_cell, cell, 1, 50, false);
                let mut out: Vec<Value> = Vec::new();
                for c in cells {
                    if let Some(tfid) = w.living_entity_on_cell(c, None) {
                        if !out.contains(&Value::Integer(tfid as i64)) {
                            out.push(Value::Integer(tfid as i64));
                        }
                    }
                }
                Ok(Some(Value::array_from(out)))
            }
            "getWeaponEffectiveArea" => {
                // Official generator: `WeaponClass.getWeaponEffectiveArea(targetCell, fromCell)` and overload with weapon id.
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.len() < 2 {
                    return Err(InterpretError::invalid_parameter_count(2, user_args.len()));
                }
                let (weapon, target_cell, from_cell) = if user_args.len() >= 3 {
                    (
                        value_as_i64(&user_args[0]).unwrap_or(-1) as i32,
                        value_as_i64(&user_args[1]).unwrap_or(-1) as i32,
                        value_as_i64(&user_args[2]).unwrap_or(-1) as i32,
                    )
                } else {
                    let w = self.world.borrow();
                    (
                        w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1),
                        value_as_i64(&user_args[0]).unwrap_or(-1) as i32,
                        value_as_i64(&user_args[1]).unwrap_or(-1) as i32,
                    )
                };
                let w = self.world.borrow();
                let Some(ws) = w.weapons_by_item.get(&weapon) else {
                    return Ok(Some(Value::Null));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, target_cell) {
                    return Ok(Some(Value::Null));
                }
                if !map::is_valid_cell(w.map_w, w.map_h, from_cell) {
                    return Ok(Some(Value::Null));
                }
                let cells =
                    Self::area_target_cells(&w, ws.area, from_cell, target_cell, 1, 50, false);
                Ok(Some(Value::array_from(
                    cells
                        .into_iter()
                        .map(|c| Value::Integer(c as i64))
                        .collect(),
                )))
            }
            "isWeapon" => {
                let v = arg0_as_i64_strict(args, "isWeapon")? as i32;
                let w = self.world.borrow();
                Ok(Some(Value::Bool(w.weapons_by_item.contains_key(&v))))
            }
            "isOnSameLine" => {
                let (c1, c2) = pair_user_ints(args, "isOnSameLine")?;
                let c1 = c1 as i32;
                let c2 = c2 as i32;
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, c1)
                    || !map::is_valid_cell(w.map_w, w.map_h, c2)
                {
                    return Ok(Some(Value::Bool(false)));
                }
                let (x1, y1) = map::cell_xy(w.map_w, c1);
                let (x2, y2) = map::cell_xy(w.map_w, c2);
                Ok(Some(Value::Bool(x1 == x2 || y1 == y2)))
            }
            "getCellsToUseChip" => {
                // Official generator: `FightClass.getCellsToUseChip(chip, targetEntity, ignoredCells?)`
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.len() < 2 {
                    return Err(InterpretError::invalid_parameter_count(2, user_args.len()));
                }
                let cid = value_as_i64(&user_args[0]).unwrap_or(-1) as i32;
                let target_fid = value_as_i64(&user_args[1]).unwrap_or(-1) as i32;
                let ignored_cells: Vec<i32> = if user_args.len() >= 3 {
                    value_cells_vec(&user_args[2])
                } else {
                    vec![self
                        .world
                        .borrow()
                        .entity(fid)
                        .map(|e| e.cell)
                        .unwrap_or(-1)]
                };
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&cid) else {
                    return Ok(Some(Value::Null));
                };
                let Some(t) = w.entity(target_fid) else {
                    return Ok(Some(Value::Null));
                };
                if t.dead {
                    return Ok(Some(Value::Null));
                }
                let target_cell = t.cell;
                let mut out: Vec<Value> = Vec::new();
                let n = map::nb_cells(w.map_w, w.map_h);
                for start in 0..n {
                    let start = start as i32;
                    if !map::is_valid_cell(w.map_w, w.map_h, start) || w.is_obstacle_cell(start) {
                        continue;
                    }
                    let occupied = w.living_entity_on_cell(start, None).is_some();
                    if occupied && !ignored_cells.contains(&start) {
                        continue;
                    }
                    if !self.verify_java_range(
                        start,
                        target_cell,
                        cs.launch_type,
                        cs.min_range,
                        cs.max_range,
                    ) {
                        continue;
                    }
                    if cs.los {
                        let Some(ok) =
                            self.verify_java_los_with_ignored(start, target_cell, &ignored_cells)
                        else {
                            continue;
                        };
                        if !ok {
                            continue;
                        }
                    }
                    out.push(Value::Integer(start as i64));
                }
                Ok(Some(Value::array_from(out)))
            }
            "getCellsToUseChipOnCell" => {
                // Official generator: `FightClass.getCellsToUseChipOnCell(chip, targetCell, ignoredCells?)`
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.len() < 2 {
                    return Err(InterpretError::invalid_parameter_count(2, user_args.len()));
                }
                let cid = value_as_i64(&user_args[0]).unwrap_or(-1) as i32;
                let target_cell = value_as_i64(&user_args[1]).unwrap_or(-1) as i32;
                let ignored_cells: Vec<i32> = if user_args.len() >= 3 {
                    value_cells_vec(&user_args[2])
                } else {
                    vec![self
                        .world
                        .borrow()
                        .entity(fid)
                        .map(|e| e.cell)
                        .unwrap_or(-1)]
                };
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&cid) else {
                    return Ok(Some(Value::Null));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, target_cell)
                    || w.is_obstacle_cell(target_cell)
                {
                    return Ok(Some(Value::Null));
                }
                let mut out: Vec<Value> = Vec::new();
                let n = map::nb_cells(w.map_w, w.map_h);
                for start in 0..n {
                    let start = start as i32;
                    if w.is_obstacle_cell(start) {
                        continue;
                    }
                    let occupied = w.living_entity_on_cell(start, None).is_some();
                    if occupied && !ignored_cells.contains(&start) {
                        continue;
                    }
                    if !self.verify_java_range(
                        start,
                        target_cell,
                        cs.launch_type,
                        cs.min_range,
                        cs.max_range,
                    ) {
                        continue;
                    }
                    if cs.los {
                        let Some(ok) =
                            self.verify_java_los_with_ignored(start, target_cell, &ignored_cells)
                        else {
                            continue;
                        };
                        if !ok {
                            continue;
                        }
                    }
                    out.push(Value::Integer(start as i64));
                }
                Ok(Some(Value::array_from(out)))
            }
            "getCellsToUseWeapon" => {
                // Official generator: `FightClass.getCellsToUseWeapon(weapon?, targetEntity, ignoredCells?)`
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.is_empty() {
                    return Err(InterpretError::invalid_parameter_count(1, user_args.len()));
                }
                let (weapon, target_fid, ignored_opt) = if user_args.len() >= 2 {
                    (
                        value_as_i64(&user_args[0]).unwrap_or(-1) as i32,
                        value_as_i64(&user_args[1]).unwrap_or(-1) as i32,
                        user_args.get(2),
                    )
                } else {
                    (-1, value_as_i64(&user_args[0]).unwrap_or(-1) as i32, None)
                };
                let ignored_cells: Vec<i32> = if let Some(v) = ignored_opt {
                    value_cells_vec(v)
                } else {
                    vec![self
                        .world
                        .borrow()
                        .entity(fid)
                        .map(|e| e.cell)
                        .unwrap_or(-1)]
                };
                let w = self.world.borrow();
                let weapon_item = if weapon == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    weapon
                };
                let Some(ws) = w.weapons_by_item.get(&weapon_item) else {
                    return Ok(Some(Value::Null));
                };
                let Some(t) = w.entity(target_fid) else {
                    return Ok(Some(Value::Null));
                };
                if t.dead {
                    return Ok(Some(Value::Null));
                }
                let target_cell = t.cell;
                let mut out: Vec<Value> = Vec::new();
                let n = map::nb_cells(w.map_w, w.map_h);
                for start in 0..n {
                    let start = start as i32;
                    if w.is_obstacle_cell(start) {
                        continue;
                    }
                    let occupied = w.living_entity_on_cell(start, None).is_some();
                    if occupied && !ignored_cells.contains(&start) {
                        continue;
                    }
                    if !self.verify_java_range(
                        start,
                        target_cell,
                        ws.launch_type,
                        ws.min_range,
                        ws.max_range,
                    ) {
                        continue;
                    }
                    if ws.los {
                        let Some(ok) =
                            self.verify_java_los_with_ignored(start, target_cell, &ignored_cells)
                        else {
                            continue;
                        };
                        if !ok {
                            continue;
                        }
                    }
                    out.push(Value::Integer(start as i64));
                }
                Ok(Some(Value::array_from(out)))
            }
            "getCellsToUseWeaponOnCell" => {
                // Official generator: `FightClass.getCellsToUseWeaponOnCell(weapon?, targetCell, ignoredCells?)`
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.is_empty() {
                    return Err(InterpretError::invalid_parameter_count(1, user_args.len()));
                }
                let (weapon, target_cell, ignored_opt) = if user_args.len() >= 2 {
                    (
                        value_as_i64(&user_args[0]).unwrap_or(-1) as i32,
                        value_as_i64(&user_args[1]).unwrap_or(-1) as i32,
                        user_args.get(2),
                    )
                } else {
                    (-1, value_as_i64(&user_args[0]).unwrap_or(-1) as i32, None)
                };
                let ignored_cells: Vec<i32> = if let Some(v) = ignored_opt {
                    value_cells_vec(v)
                } else {
                    vec![self
                        .world
                        .borrow()
                        .entity(fid)
                        .map(|e| e.cell)
                        .unwrap_or(-1)]
                };
                let w = self.world.borrow();
                let weapon_item = if weapon == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    weapon
                };
                let Some(ws) = w.weapons_by_item.get(&weapon_item) else {
                    return Ok(Some(Value::Null));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, target_cell)
                    || w.is_obstacle_cell(target_cell)
                {
                    return Ok(Some(Value::Null));
                }
                let mut out: Vec<Value> = Vec::new();
                let n = map::nb_cells(w.map_w, w.map_h);
                for start in 0..n {
                    let start = start as i32;
                    if w.is_obstacle_cell(start) {
                        continue;
                    }
                    let occupied = w.living_entity_on_cell(start, None).is_some();
                    if occupied && !ignored_cells.contains(&start) {
                        continue;
                    }
                    if !self.verify_java_range(
                        start,
                        target_cell,
                        ws.launch_type,
                        ws.min_range,
                        ws.max_range,
                    ) {
                        continue;
                    }
                    if ws.los {
                        let Some(ok) =
                            self.verify_java_los_with_ignored(start, target_cell, &ignored_cells)
                        else {
                            continue;
                        };
                        if !ok {
                            continue;
                        }
                    }
                    out.push(Value::Integer(start as i64));
                }
                Ok(Some(Value::array_from(out)))
            }
            "getCellToUseChip" => {
                // Official generator: `FightClass.getCellToUseChip(chip, targetEntity, ignoredCells?)` → first valid start cell, or -1.
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.len() < 2 {
                    return Err(InterpretError::invalid_parameter_count(2, user_args.len()));
                }
                let cid = value_as_i64(&user_args[0]).unwrap_or(-1) as i32;
                let target_fid = value_as_i64(&user_args[1]).unwrap_or(-1) as i32;
                let ignored_cells: Vec<i32> = if user_args.len() >= 3 {
                    value_cells_vec(&user_args[2])
                } else {
                    vec![self
                        .world
                        .borrow()
                        .entity(fid)
                        .map(|e| e.cell)
                        .unwrap_or(-1)]
                };
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&cid) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let Some(t) = w.entity(target_fid) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                if t.dead {
                    return Ok(Some(Value::Integer(-1)));
                }
                let target_cell = t.cell;
                let n = map::nb_cells(w.map_w, w.map_h);
                for start in 0..n {
                    let start = start as i32;
                    if !map::is_valid_cell(w.map_w, w.map_h, start) || w.is_obstacle_cell(start) {
                        continue;
                    }
                    let occupied = w.living_entity_on_cell(start, None).is_some();
                    if occupied && !ignored_cells.contains(&start) {
                        continue;
                    }
                    if !self.verify_java_range(
                        start,
                        target_cell,
                        cs.launch_type,
                        cs.min_range,
                        cs.max_range,
                    ) {
                        continue;
                    }
                    if cs.los {
                        let Some(ok) =
                            self.verify_java_los_with_ignored(start, target_cell, &ignored_cells)
                        else {
                            continue;
                        };
                        if !ok {
                            continue;
                        }
                    }
                    return Ok(Some(Value::Integer(start as i64)));
                }
                Ok(Some(Value::Integer(-1)))
            }
            "getCellToUseChipOnCell" => {
                // Official generator: `FightClass.getCellToUseChipOnCell(chip, targetCell, ignoredCells?)` → first valid start cell, or -1.
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.len() < 2 {
                    return Err(InterpretError::invalid_parameter_count(2, user_args.len()));
                }
                let cid = value_as_i64(&user_args[0]).unwrap_or(-1) as i32;
                let target_cell = value_as_i64(&user_args[1]).unwrap_or(-1) as i32;
                let ignored_cells: Vec<i32> = if user_args.len() >= 3 {
                    value_cells_vec(&user_args[2])
                } else {
                    vec![self
                        .world
                        .borrow()
                        .entity(fid)
                        .map(|e| e.cell)
                        .unwrap_or(-1)]
                };
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&cid) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, target_cell)
                    || w.is_obstacle_cell(target_cell)
                {
                    return Ok(Some(Value::Integer(-1)));
                }
                let n = map::nb_cells(w.map_w, w.map_h);
                for start in 0..n {
                    let start = start as i32;
                    if w.is_obstacle_cell(start) {
                        continue;
                    }
                    let occupied = w.living_entity_on_cell(start, None).is_some();
                    if occupied && !ignored_cells.contains(&start) {
                        continue;
                    }
                    if !self.verify_java_range(
                        start,
                        target_cell,
                        cs.launch_type,
                        cs.min_range,
                        cs.max_range,
                    ) {
                        continue;
                    }
                    if cs.los {
                        let Some(ok) =
                            self.verify_java_los_with_ignored(start, target_cell, &ignored_cells)
                        else {
                            continue;
                        };
                        if !ok {
                            continue;
                        }
                    }
                    return Ok(Some(Value::Integer(start as i64)));
                }
                Ok(Some(Value::Integer(-1)))
            }
            "getCellToUseWeapon" => {
                // Official generator: `FightClass.getCellToUseWeapon(weapon?, targetEntity, ignoredCells?)` → first valid start cell, or -1.
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.is_empty() {
                    return Err(InterpretError::invalid_parameter_count(1, user_args.len()));
                }
                let (weapon, target_fid, ignored_opt) = if user_args.len() >= 2 {
                    (
                        value_as_i64(&user_args[0]).unwrap_or(-1) as i32,
                        value_as_i64(&user_args[1]).unwrap_or(-1) as i32,
                        user_args.get(2),
                    )
                } else {
                    (-1, value_as_i64(&user_args[0]).unwrap_or(-1) as i32, None)
                };
                let ignored_cells: Vec<i32> = if let Some(v) = ignored_opt {
                    value_cells_vec(v)
                } else {
                    vec![self
                        .world
                        .borrow()
                        .entity(fid)
                        .map(|e| e.cell)
                        .unwrap_or(-1)]
                };
                let w = self.world.borrow();
                let weapon_item = if weapon == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    weapon
                };
                let Some(ws) = w.weapons_by_item.get(&weapon_item) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                let Some(t) = w.entity(target_fid) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                if t.dead {
                    return Ok(Some(Value::Integer(-1)));
                }
                let target_cell = t.cell;
                let n = map::nb_cells(w.map_w, w.map_h);
                for start in 0..n {
                    let start = start as i32;
                    if w.is_obstacle_cell(start) {
                        continue;
                    }
                    let occupied = w.living_entity_on_cell(start, None).is_some();
                    if occupied && !ignored_cells.contains(&start) {
                        continue;
                    }
                    if !self.verify_java_range(
                        start,
                        target_cell,
                        ws.launch_type,
                        ws.min_range,
                        ws.max_range,
                    ) {
                        continue;
                    }
                    if ws.los {
                        let Some(ok) =
                            self.verify_java_los_with_ignored(start, target_cell, &ignored_cells)
                        else {
                            continue;
                        };
                        if !ok {
                            continue;
                        }
                    }
                    return Ok(Some(Value::Integer(start as i64)));
                }
                Ok(Some(Value::Integer(-1)))
            }
            "getCellToUseWeaponOnCell" => {
                // Official generator: `FightClass.getCellToUseWeaponOnCell(weapon?, targetCell, ignoredCells?)` → first valid start cell, or -1.
                let user_args: &[Value] = if args.len() >= 2 { &args[1..] } else { args };
                if user_args.is_empty() {
                    return Err(InterpretError::invalid_parameter_count(1, user_args.len()));
                }
                let (weapon, target_cell, ignored_opt) = if user_args.len() >= 2 {
                    (
                        value_as_i64(&user_args[0]).unwrap_or(-1) as i32,
                        value_as_i64(&user_args[1]).unwrap_or(-1) as i32,
                        user_args.get(2),
                    )
                } else {
                    (-1, value_as_i64(&user_args[0]).unwrap_or(-1) as i32, None)
                };
                let ignored_cells: Vec<i32> = if let Some(v) = ignored_opt {
                    value_cells_vec(v)
                } else {
                    vec![self
                        .world
                        .borrow()
                        .entity(fid)
                        .map(|e| e.cell)
                        .unwrap_or(-1)]
                };
                let w = self.world.borrow();
                let weapon_item = if weapon == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    weapon
                };
                let Some(ws) = w.weapons_by_item.get(&weapon_item) else {
                    return Ok(Some(Value::Integer(-1)));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, target_cell)
                    || w.is_obstacle_cell(target_cell)
                {
                    return Ok(Some(Value::Integer(-1)));
                }
                let n = map::nb_cells(w.map_w, w.map_h);
                for start in 0..n {
                    let start = start as i32;
                    if w.is_obstacle_cell(start) {
                        continue;
                    }
                    let occupied = w.living_entity_on_cell(start, None).is_some();
                    if occupied && !ignored_cells.contains(&start) {
                        continue;
                    }
                    if !self.verify_java_range(
                        start,
                        target_cell,
                        ws.launch_type,
                        ws.min_range,
                        ws.max_range,
                    ) {
                        continue;
                    }
                    if ws.los {
                        let Some(ok) =
                            self.verify_java_los_with_ignored(start, target_cell, &ignored_cells)
                        else {
                            continue;
                        };
                        if !ok {
                            continue;
                        }
                    }
                    return Ok(Some(Value::Integer(start as i64)));
                }
                Ok(Some(Value::Integer(-1)))
            }
            "getChipMinRange" => {
                let cid = arg0_as_i64_strict(args, "getChipMinRange")? as i32;
                let w = self.world.borrow();
                let v = w.chips_by_id.get(&cid).map(|cs| cs.min_range).unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getChipMinScope" => {
                // Alias.
                let v = self.call_native("getChipMinRange", args, None)?;
                Ok(v)
            }
            "getChipMaxRange" => {
                let cid = arg0_as_i64_strict(args, "getChipMaxRange")? as i32;
                let w = self.world.borrow();
                let v = w.chips_by_id.get(&cid).map(|cs| cs.max_range).unwrap_or(0);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getChipMaxScope" => {
                // Alias.
                let v = self.call_native("getChipMaxRange", args, None)?;
                Ok(v)
            }
            "getChipLaunchType" => {
                let cid = arg0_as_i64_strict(args, "getChipLaunchType")? as i32;
                let w = self.world.borrow();
                let v = w
                    .chips_by_id
                    .get(&cid)
                    .map(|cs| cs.launch_type)
                    .unwrap_or(7);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getChipArea" => {
                let cid = arg0_as_i64_strict(args, "getChipArea")? as i32;
                let w = self.world.borrow();
                let Some(cs) = w.chips_by_id.get(&cid) else {
                    return Ok(Some(Value::Null));
                };
                Ok(Some(Value::Integer(cs.area as i64)))
            }
            "canUseChipOnCell" => {
                let (cid, cell) = pair_user_ints(args, "canUseChipOnCell")?;
                let cid = cid as i32;
                let cell = cell as i32;
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Bool(false)));
                };
                let Some(cs) = w.chips_by_id.get(&cid) else {
                    return Ok(Some(Value::Bool(false)));
                };
                if !me.chips.contains(&cid) {
                    return Ok(Some(Value::Bool(false)));
                }
                if Self::chip_use_blocked_by_cooldown_or_cap(&w, fid, cs).is_some() {
                    return Ok(Some(Value::Bool(false)));
                }
                if !map::is_valid_cell(w.map_w, w.map_h, cell) || w.is_obstacle_cell(cell) {
                    return Ok(Some(Value::Bool(false)));
                }
                let ok = self.verify_java_range(
                    me.cell,
                    cell,
                    cs.launch_type,
                    cs.min_range,
                    cs.max_range,
                ) && (!cs.los || self.verify_java_los(me.cell, cell));
                Ok(Some(Value::Bool(ok)))
            }
            "canUseChip" => {
                let (cid, target_fid) = pair_user_ints(args, "canUseChip")?;
                let cid = cid as i32;
                let target_fid = target_fid as i32;
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Bool(false)));
                };
                let Some(t) = w.entity(target_fid) else {
                    return Ok(Some(Value::Bool(false)));
                };
                if t.dead {
                    return Ok(Some(Value::Bool(false)));
                }
                let Some(cs) = w.chips_by_id.get(&cid) else {
                    return Ok(Some(Value::Bool(false)));
                };
                if !me.chips.contains(&cid) {
                    return Ok(Some(Value::Bool(false)));
                }
                if Self::chip_use_blocked_by_cooldown_or_cap(&w, fid, cs).is_some() {
                    return Ok(Some(Value::Bool(false)));
                }
                let ok = self.verify_java_range(
                    me.cell,
                    t.cell,
                    cs.launch_type,
                    cs.min_range,
                    cs.max_range,
                ) && (!cs.los || self.verify_java_los(me.cell, t.cell));
                Ok(Some(Value::Bool(ok)))
            }
            "summon" => {
                // Official generator: `FightClass.summon(chip, cell, callback, name?)`.
                // This Rust port does not yet support per-summon runtime AIs; we summon using the chip template like `useChipOnCell`,
                // and ignore the callback (but do apply the optional name).
                if args.len() < 3 {
                    return Err(InterpretError::invalid_parameter_count(3, args.len()));
                }
                let cid = value_as_i64(&args[0]).unwrap_or(-1) as i32;
                let cell = value_as_i64(&args[1]).unwrap_or(-1) as i32;
                // args[2] is the callback function in the official generator; ignored for now.
                let name_override = args.get(3).and_then(|v| match v {
                    Value::String(s) => Some(s.to_string()),
                    Value::Null => None,
                    _ => Some(value_debug_string(v)),
                });

                let (me_cell, tp, has_chip, cs) = {
                    let w = self.world.borrow();
                    let Some(me) = w.entity(fid) else {
                        return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                    };
                    let has_chip = me.chips.contains(&cid);
                    let cs = w.chips_by_id.get(&cid).cloned();
                    (me.cell, me.tp, has_chip, cs)
                };
                let Some(cs) = cs else {
                    return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                };
                if !has_chip {
                    return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                }
                if cs.cost > 0 && cs.cost > tp {
                    return Ok(Some(Value::Integer(USE_NOT_ENOUGH_TP as i64)));
                }
                {
                    let w = self.world.borrow();
                    if let Some(code) = Self::chip_use_blocked_by_cooldown_or_cap(&w, fid, &cs) {
                        return Ok(Some(Value::Integer(code)));
                    }
                }
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, cell) || w.is_obstacle_cell(cell) {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                if !self.verify_java_range(
                    me_cell,
                    cell,
                    cs.launch_type,
                    cs.min_range,
                    cs.max_range,
                ) || (cs.los && !self.verify_java_los(me_cell, cell))
                {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                drop(w);

                let use_result = {
                    let mut w = self.world.borrow_mut();
                    if let Some(code) =
                        self.use_chip_if_summon(&mut w, fid, cell, &cs, name_override)
                    {
                        return Ok(Some(Value::Integer(code)));
                    }
                    // Not a summon chip: fall back to normal chip usage semantics on that cell.
                    let critical = Self::generate_critical(&mut w, fid);
                    let result = if critical { USE_CRITICAL } else { USE_SUCCESS };
                    let jet = w.rng.next_double01();
                    w.log_action(json!([ACTION_USE_CHIP, cs.template_id, cell, result]));
                    let cells = Self::area_target_cells(
                        &w,
                        cs.area,
                        me_cell,
                        cell,
                        cs.min_range,
                        cs.max_range,
                        cs.los,
                    );
                    apply_effects_on_cells(
                        &mut w,
                        fid,
                        &cells,
                        &cs.effects,
                        EffectContext {
                            critical,
                            result_code: result,
                            jet,
                            attack_type: 2,
                            item_id: cid,
                        },
                        cs.area,
                        cell,
                    );
                    if let Some(me) = w.entity_mut(fid) {
                        me.tp = (me.tp - cs.cost).max(0);
                    }
                    w.register_chip_use_after_success(fid, &cs);
                    result as i64
                };
                Ok(Some(Value::Integer(use_result)))
            }
            "useChip" => {
                // Official generator: `ChipClass.useChip(chip_id, leek_id?)` -> uses target's cell.
                let cid = arg0_as_i64_strict(args, "useChip")? as i32;
                let target_fid = match args.len() {
                    n if n >= 3 => value_as_i64(args.get(2).unwrap()).unwrap_or(fid as i64) as i32,
                    _ => fid,
                };
                let (me_cell, tp, has_chip, target_cell, target_dead, cs) = {
                    let w = self.world.borrow();
                    let Some(me) = w.entity(fid) else {
                        return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                    };
                    let Some(t) = w.entity(target_fid) else {
                        return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                    };
                    let has_chip = me.chips.contains(&cid);
                    let cs = w.chips_by_id.get(&cid).cloned();
                    (me.cell, me.tp, has_chip, t.cell, t.dead, cs)
                };
                let Some(cs) = cs else {
                    return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                };
                if !has_chip || target_dead {
                    return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                }
                if cs.cost > 0 && cs.cost > tp {
                    return Ok(Some(Value::Integer(USE_NOT_ENOUGH_TP as i64)));
                }
                {
                    let w = self.world.borrow();
                    if let Some(code) = Self::chip_use_blocked_by_cooldown_or_cap(&w, fid, &cs) {
                        return Ok(Some(Value::Integer(code)));
                    }
                }
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, target_cell)
                    || w.is_obstacle_cell(target_cell)
                {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                if !self.verify_java_range(
                    me_cell,
                    target_cell,
                    cs.launch_type,
                    cs.min_range,
                    cs.max_range,
                ) || (cs.los && !self.verify_java_los(me_cell, target_cell))
                {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                drop(w);
                let use_result = {
                    let mut w = self.world.borrow_mut();
                    if let Some(code) = self.use_chip_if_summon(&mut w, fid, target_cell, &cs, None)
                    {
                        return Ok(Some(Value::Integer(code)));
                    }
                    let critical = Self::generate_critical(&mut w, fid);
                    let result = if critical { USE_CRITICAL } else { USE_SUCCESS };
                    let jet = w.rng.next_double01();
                    // ActionUseChip: [USE_CHIP, chipTemplateId, cell, result]
                    w.log_action(json!([
                        ACTION_USE_CHIP,
                        cs.template_id,
                        target_cell,
                        result
                    ]));
                    let cells = Self::area_target_cells(
                        &w,
                        cs.area,
                        me_cell,
                        target_cell,
                        cs.min_range,
                        cs.max_range,
                        cs.los,
                    );
                    apply_effects_on_cells(
                        &mut w,
                        fid,
                        &cells,
                        &cs.effects,
                        EffectContext {
                            critical,
                            result_code: result,
                            jet,
                            attack_type: 2,
                            item_id: cid,
                        },
                        cs.area,
                        target_cell,
                    );
                    if let Some(me) = w.entity_mut(fid) {
                        me.tp = (me.tp - cs.cost).max(0);
                    }
                    w.register_chip_use_after_success(fid, &cs);
                    result as i64
                };
                Ok(Some(Value::Integer(use_result)))
            }
            "useChipOnCell" => {
                let (cid, cell) = pair_user_ints(args, "useChipOnCell")?;
                let cid = cid as i32;
                let cell = cell as i32;
                let (me_cell, tp, has_chip, cs) = {
                    let w = self.world.borrow();
                    let Some(me) = w.entity(fid) else {
                        return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                    };
                    let has_chip = me.chips.contains(&cid);
                    let cs = w.chips_by_id.get(&cid).cloned();
                    (me.cell, me.tp, has_chip, cs)
                };
                let Some(cs) = cs else {
                    return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                };
                if !has_chip {
                    return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                }
                if cs.cost > 0 && cs.cost > tp {
                    return Ok(Some(Value::Integer(USE_NOT_ENOUGH_TP as i64)));
                }
                {
                    let w = self.world.borrow();
                    if let Some(code) = Self::chip_use_blocked_by_cooldown_or_cap(&w, fid, &cs) {
                        return Ok(Some(Value::Integer(code)));
                    }
                }
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, cell) || w.is_obstacle_cell(cell) {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                if !self.verify_java_range(
                    me_cell,
                    cell,
                    cs.launch_type,
                    cs.min_range,
                    cs.max_range,
                ) || (cs.los && !self.verify_java_los(me_cell, cell))
                {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                drop(w);
                let use_result = {
                    let mut w = self.world.borrow_mut();
                    if let Some(code) = self.use_chip_if_summon(&mut w, fid, cell, &cs, None) {
                        return Ok(Some(Value::Integer(code)));
                    }
                    let critical = Self::generate_critical(&mut w, fid);
                    let result = if critical { USE_CRITICAL } else { USE_SUCCESS };
                    let jet = w.rng.next_double01();
                    w.log_action(json!([ACTION_USE_CHIP, cs.template_id, cell, result]));
                    let cells = Self::area_target_cells(
                        &w,
                        cs.area,
                        me_cell,
                        cell,
                        cs.min_range,
                        cs.max_range,
                        cs.los,
                    );
                    apply_effects_on_cells(
                        &mut w,
                        fid,
                        &cells,
                        &cs.effects,
                        EffectContext {
                            critical,
                            result_code: result,
                            jet,
                            attack_type: 2,
                            item_id: cid,
                        },
                        cs.area,
                        cell,
                    );
                    if let Some(me) = w.entity_mut(fid) {
                        me.tp = (me.tp - cs.cost).max(0);
                    }
                    w.register_chip_use_after_success(fid, &cs);
                    result as i64
                };
                Ok(Some(Value::Integer(use_result)))
            }
            "lineOfSight" => {
                // Official generator: `FieldClass.lineOfSight(start, end, ignore?)`.
                let (start, end) = pair_user_ints(args, "lineOfSight")?;
                let start = start as i32;
                let end = end as i32;

                // Build ignored cell list.
                let mut ignored: Vec<i32> = Vec::new();
                {
                    let w = self.world.borrow();
                    if let Some(me) = w.entity(fid) {
                        ignored.push(me.cell);
                    }
                }

                // Optional ignore arg: number (entity id) or array of entity ids.
                let opt_ignore = match args.len() {
                    n if n >= 4 => args.get(3),
                    3 => args.get(2),
                    _ => None,
                };
                if let Some(v) = opt_ignore {
                    if let Some(id) = value_as_i64(v) {
                        let w = self.world.borrow();
                        if let Some(e) = w.entity(id as i32) {
                            ignored.push(e.cell);
                        }
                    } else if let Value::Array(a) = v {
                        let ids: Vec<i32> = a
                            .borrow()
                            .iter()
                            .filter_map(|x| value_as_i64(x).map(|n| n as i32))
                            .collect();
                        let w = self.world.borrow();
                        for id in ids {
                            if let Some(e) = w.entity(id) {
                                ignored.push(e.cell);
                            }
                        }
                    }
                }

                // Official generator: returns null when either cell is invalid.
                let Some(ok) = self.verify_java_los_with_ignored(start, end, &ignored) else {
                    return Ok(Some(Value::Null));
                };
                Ok(Some(Value::Bool(ok)))
            }
            "resurrect" => {
                // Official generator: `ChipClass.resurrect` / `State.resurrectEntity`.
                let (target_fid, cell) = pair_user_ints(args, "resurrect")?;
                let target_fid = target_fid as i32;
                let cell = cell as i32;

                let (me_cell, tp, template_id, full_life, cs, target_summon, target_team) = {
                    let w = self.world.borrow();
                    if w.active_fid != fid {
                        return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                    }
                    let Some(me) = w.entity(fid) else {
                        return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                    };
                    if me.dead {
                        return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                    }
                    let has_84 = me.chips.contains(&CHIP_RESURRECTION)
                        && w.chips_by_id
                            .get(&CHIP_RESURRECTION)
                            .map_or(false, Self::chip_has_resurrect_effect);
                    let has_415 = me.chips.contains(&CHIP_AWEKENING)
                        && w.chips_by_id
                            .get(&CHIP_AWEKENING)
                            .map_or(false, Self::chip_has_resurrect_effect);
                    if !has_84 && !has_415 {
                        return Ok(Some(Value::Integer(-1)));
                    }
                    let template_id = if has_84 {
                        CHIP_RESURRECTION
                    } else {
                        CHIP_AWEKENING
                    };
                    let full_life = has_415;
                    let Some(cs) = w.chips_by_id.get(&template_id).cloned() else {
                        return Ok(Some(Value::Integer(-1)));
                    };
                    let Some(target) = w.entity(target_fid) else {
                        return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                    };
                    if !target.dead {
                        return Ok(Some(Value::Integer(USE_RESURRECT_INVALID_ENTITY as i64)));
                    }
                    (
                        me.cell,
                        me.tp,
                        template_id,
                        full_life,
                        cs,
                        target.is_summon,
                        target.team,
                    )
                };

                if target_summon
                    && self.world.borrow().team_summon_count(target_team)
                        >= FightWorld::SUMMON_LIMIT
                {
                    return Ok(Some(Value::Integer(USE_TOO_MANY_SUMMONS as i64)));
                }
                if cs.cost > 0 && cs.cost > tp {
                    return Ok(Some(Value::Integer(USE_NOT_ENOUGH_TP as i64)));
                }
                {
                    let w = self.world.borrow();
                    if let Some(code) = Self::chip_use_blocked_by_cooldown_or_cap(&w, fid, &cs) {
                        return Ok(Some(Value::Integer(code)));
                    }
                }
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, cell) || w.is_obstacle_cell(cell) {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                if w.living_entity_on_cell(cell, None).is_some() {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                if !self.verify_java_range(
                    me_cell,
                    cell,
                    cs.launch_type,
                    cs.min_range,
                    cs.max_range,
                ) || (cs.los && !self.verify_java_los(me_cell, cell))
                {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                drop(w);

                let use_result = {
                    let mut w = self.world.borrow_mut();
                    let critical = Self::generate_critical(&mut w, fid);
                    let result = if critical { USE_CRITICAL } else { USE_SUCCESS };
                    w.log_action(json!([ACTION_USE_CHIP, cs.template_id, cell, result]));
                    w.apply_resurrection(fid, target_fid, cell, critical, full_life);
                    w.register_chip_use_after_success(fid, &cs);
                    if result > 0 && template_id == CHIP_AWEKENING {
                        w.grant_awakening_invulnerability(fid, target_fid, critical);
                    }
                    if let Some(me) = w.entity_mut(fid) {
                        me.tp = (me.tp - cs.cost).max(0);
                    }
                    result as i64
                };
                Ok(Some(Value::Integer(use_result)))
            }
            "getLeekOnCell" | "getEntityOnCell" => {
                let c = int_user_arg(args, name)? as i32;
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, c) {
                    return Ok(Some(Value::Integer(-1)));
                }
                let v = w.living_entity_on_cell(c, None).unwrap_or(-1) as i64;
                Ok(Some(Value::Integer(v)))
            }
            "isEmptyCell" => {
                let c = int_user_arg(args, "isEmptyCell")? as i32;
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, c) {
                    return Ok(Some(Value::Bool(false)));
                }
                let empty = w.living_entity_on_cell(c, None).is_none();
                Ok(Some(Value::Bool(empty)))
            }
            "isEntity" | "isLeek" => {
                let c = int_user_arg(args, name)? as i32;
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, c) {
                    return Ok(Some(Value::Bool(false)));
                }
                Ok(Some(Value::Bool(
                    w.living_entity_on_cell(c, None).is_some(),
                )))
            }
            "isObstacle" => {
                let c = int_user_arg(args, "isObstacle")? as i32;
                let w = self.world.borrow();
                if !map::is_valid_cell(w.map_w, w.map_h, c) {
                    return Ok(Some(Value::Bool(true)));
                }
                Ok(Some(Value::Bool(w.is_obstacle_cell(c))))
            }
            "getWeapons" => {
                let w = {
                    let w = self.world.borrow();
                    let e = w.entity(fid).ok_or_else(|| InterpretError {
                        reference: "INTERNAL_ERROR",
                        message: "entity missing".into(),
                    })?;
                    e.weapons
                        .iter()
                        .map(|&i| Value::Integer(i as i64))
                        .collect::<Vec<_>>()
                };
                Ok(Some(Value::array_from(w)))
            }
            "getWeapon" => {
                // Official generator: `FightClass.getWeapon()`: equipped weapon item id, -1 when none.
                let w = self.world.borrow();
                let v = w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getWeaponName" => {
                let wid = arg0_as_i64_loose(args, "getWeaponName")? as i32;
                let w = self.world.borrow();
                let item_id = if wid == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    wid
                };
                let v = w
                    .weapons_by_item
                    .get(&item_id)
                    .map(|ws| ws.name.clone())
                    .unwrap_or_default();
                Ok(Some(Value::String(v.into())))
            }
            "getWeaponCost" => {
                let wid = arg0_as_i64_loose(args, "getWeaponCost")? as i32;
                let w = self.world.borrow();
                let item_id = if wid == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    wid
                };
                let v = w
                    .weapons_by_item
                    .get(&item_id)
                    .map(|ws| ws.cost)
                    .unwrap_or(-1);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getWeaponMaxUses" => {
                // Official generator: currently returns -1 (unlimited) for weapons.
                Ok(Some(Value::Integer(-1)))
            }
            "getWeaponMinRange" | "getWeaponMinScope" => {
                let wid = arg0_as_i64_loose(args, name)? as i32;
                let w = self.world.borrow();
                let item_id = if wid == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    wid
                };
                let v = w
                    .weapons_by_item
                    .get(&item_id)
                    .map(|ws| ws.min_range)
                    .unwrap_or(-1);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getWeaponMaxRange" | "getWeaponMaxScope" => {
                let wid = arg0_as_i64_loose(args, name)? as i32;
                let w = self.world.borrow();
                let item_id = if wid == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    wid
                };
                let v = w
                    .weapons_by_item
                    .get(&item_id)
                    .map(|ws| ws.max_range)
                    .unwrap_or(-1);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getWeaponLaunchType" => {
                let wid = arg0_as_i64_loose(args, "getWeaponLaunchType")? as i32;
                let w = self.world.borrow();
                let item_id = if wid == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    wid
                };
                let v = w
                    .weapons_by_item
                    .get(&item_id)
                    .map(|ws| ws.launch_type)
                    .unwrap_or(7);
                Ok(Some(Value::Integer(v as i64)))
            }
            "getWeaponArea" => {
                let wid = arg0_as_i64_loose(args, "getWeaponArea")? as i32;
                let w = self.world.borrow();
                let item_id = if wid == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    wid
                };
                let Some(ws) = w.weapons_by_item.get(&item_id) else {
                    return Ok(Some(Value::Null));
                };
                Ok(Some(Value::Integer(ws.area as i64)))
            }
            "getWeaponFailure" => Ok(Some(Value::Integer(0))),
            "getWeaponFailureRate" => Ok(Some(Value::Integer(0))),
            "getWeaponEffects" => {
                let wid = arg0_as_i64_loose(args, "getWeaponEffects")? as i32;
                let w = self.world.borrow();
                let item_id = if wid == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    wid
                };
                let Some(ws) = w.weapons_by_item.get(&item_id) else {
                    return Ok(Some(Value::Null));
                };
                let arr = ws
                    .effects
                    .iter()
                    .map(Self::feature_array_from_effect)
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(arr)))
            }
            "getWeaponPassiveEffects" => {
                let wid = arg0_as_i64_loose(args, "getWeaponPassiveEffects")? as i32;
                let w = self.world.borrow();
                let item_id = if wid == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    wid
                };
                let Some(ws) = w.weapons_by_item.get(&item_id) else {
                    return Ok(Some(Value::Null));
                };
                let arr = ws
                    .passive_effects
                    .iter()
                    .map(Self::feature_array_from_effect)
                    .collect::<Vec<_>>();
                Ok(Some(Value::array_from(arr)))
            }
            "isInlineWeapon" => {
                let wid = arg0_as_i64_loose(args, "isInlineWeapon")? as i32;
                let w = self.world.borrow();
                let item_id = if wid == -1 {
                    w.entity(fid).and_then(|e| e.equipped_weapon).unwrap_or(-1)
                } else {
                    wid
                };
                let launch = w
                    .weapons_by_item
                    .get(&item_id)
                    .map(|ws| ws.launch_type)
                    .unwrap_or(0);
                Ok(Some(Value::Bool(launch == 1)))
            }
            "getAllWeapons" => {
                let w = self.world.borrow();
                let mut ids: Vec<i32> = w.weapons_by_item.keys().copied().collect();
                ids.sort_unstable();
                Ok(Some(Value::array_from(
                    ids.into_iter()
                        .map(|id| Value::Integer(id as i64))
                        .collect(),
                )))
            }
            "canUseWeaponOnCell" => {
                // Range + LoS only (does NOT check TP or equipped, per signature docs).
                let (weapon, cell) = pair_user_ints(args, "canUseWeaponOnCell")?;
                let weapon = weapon as i32;
                let cell = cell as i32;
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Bool(false)));
                };
                let weapon_item = if weapon == -1 {
                    me.equipped_weapon.unwrap_or(-1)
                } else {
                    weapon
                };
                let Some(ws) = w.weapons_by_item.get(&weapon_item) else {
                    return Ok(Some(Value::Bool(false)));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, cell) || w.is_obstacle_cell(cell) {
                    return Ok(Some(Value::Bool(false)));
                }
                let ok = self.verify_java_range(
                    me.cell,
                    cell,
                    ws.launch_type,
                    ws.min_range,
                    ws.max_range,
                ) && (!ws.los || self.verify_java_los(me.cell, cell));
                Ok(Some(Value::Bool(ok)))
            }
            "canUseWeapon" => {
                let (weapon, target_fid) = pair_user_ints(args, "canUseWeapon")?;
                let weapon = weapon as i32;
                let target_fid = target_fid as i32;
                let w = self.world.borrow();
                let Some(me) = w.entity(fid) else {
                    return Ok(Some(Value::Bool(false)));
                };
                let Some(t) = w.entity(target_fid) else {
                    return Ok(Some(Value::Bool(false)));
                };
                if t.dead {
                    return Ok(Some(Value::Bool(false)));
                }
                let weapon_item = if weapon == -1 {
                    me.equipped_weapon.unwrap_or(-1)
                } else {
                    weapon
                };
                let Some(ws) = w.weapons_by_item.get(&weapon_item) else {
                    return Ok(Some(Value::Bool(false)));
                };
                let ok = self.verify_java_range(
                    me.cell,
                    t.cell,
                    ws.launch_type,
                    ws.min_range,
                    ws.max_range,
                ) && (!ws.los || self.verify_java_los(me.cell, t.cell));
                Ok(Some(Value::Bool(ok)))
            }
            "setWeapon" => {
                let wid = arg0_as_i64_strict(args, "setWeapon")? as i32;
                let ok = {
                    let mut w = self.world.borrow_mut();
                    let weapon_exists = w.weapons_by_item.contains_key(&wid);
                    let e = w.entity_mut(fid).ok_or_else(|| InterpretError {
                        reference: "INTERNAL_ERROR",
                        message: "entity missing".into(),
                    })?;
                    if e.tp <= 0 {
                        false
                    } else if !e.weapons.contains(&wid) {
                        false
                    } else if !weapon_exists {
                        // Official generator: `Weapons.getWeapon(wid)` must exist (scenario can contain stale ids).
                        false
                    } else {
                        e.equipped_weapon = Some(wid);
                        e.tp -= 1;
                        // Official generator: `ActionSetWeapon`: `[SET_WEAPON, weaponTemplate]`.
                        w.log_action(json!([ACTION_SET_WEAPON, wid]));
                        true
                    }
                };
                Ok(Some(Value::Bool(ok)))
            }
            "moveToward" => {
                let target = arg0_as_i64_loose(args, "moveToward")? as i32;
                let mut used = 0i32;
                let mut path: Vec<i32> = Vec::new();
                loop {
                    // Official generator: `Map.getPathBeetween(start, targetCell, null)` (A*), then move up to MP.
                    let planned = (|| -> Option<Vec<i32>> {
                        let w = self.world.borrow();
                        let (mw, mh) = (w.map_w, w.map_h);
                        let me = w.entity(fid)?;
                        if me.dead || me.mp <= 0 {
                            return None;
                        }
                        let cur = me.cell;
                        let target_cell = w.entity(target).filter(|t| !t.dead).map(|t| t.cell)?;
                        if !map::is_valid_cell(mw, mh, target_cell) {
                            return None;
                        }
                        let steps = pathfinding::get_path_between(&w, cur, target_cell, None)?;
                        if steps.is_empty() {
                            return None;
                        }
                        Some(steps)
                    })();

                    let Some(planned) = planned else { break };
                    // Walk up to remaining MP.
                    for next in planned {
                        let mut w = self.world.borrow_mut();
                        let blocked = w.living_entity_on_cell(next, Some(fid)).is_some();
                        let Some(me) = w.entity_mut(fid) else {
                            break;
                        };
                        if me.dead || me.mp <= 0 {
                            break;
                        }
                        if blocked {
                            break;
                        }
                        me.mp -= 1;
                        me.cell = next;
                        used += 1;
                        path.push(next);
                    }
                    break;
                }
                if used > 0 {
                    // Official generator: ActionMove: [MOVE_TO, leek, end, [path...]]
                    let end = *path.last().unwrap_or(&-1);
                    self.world
                        .borrow_mut()
                        .log_action(json!([ACTION_MOVE_TO, fid, end, path]));
                }
                Ok(Some(Value::Integer(used as i64)))
            }
            "moveTowardLine" => {
                // Official generator: `FightClass.moveTowardLine(cellA, cellB, pm?)` → `Map.getPathTowardLine` + `moveEntity`.
                let (cell1, cell2) = pair_user_ints(args, "moveTowardLine")?;
                let cell1 = cell1 as i32;
                let cell2 = cell2 as i32;
                let pm_to_use = if args.len() >= 4 {
                    value_as_i64(&args[3]).unwrap_or(-1) as i32
                } else if args.len() == 3 {
                    value_as_i64(&args[2]).unwrap_or(-1) as i32
                } else {
                    -1
                };
                let mut used = 0i32;
                let mut walked: Vec<i32> = Vec::new();
                let planned = (|| -> Option<Vec<i32>> {
                    let w = self.world.borrow();
                    let me = w.entity(fid)?;
                    if me.dead || me.mp <= 0 {
                        return None;
                    }
                    let pm = if pm_to_use == -1 {
                        me.mp
                    } else {
                        pm_to_use.min(me.mp)
                    };
                    if pm <= 0 {
                        return None;
                    }
                    let start = me.cell;
                    let goals = Self::line_cells_astar_goals(&w, cell1, cell2);
                    if goals.is_empty() {
                        return None;
                    }
                    let path = pathfinding::get_astar_path_to_any(&w, start, &goals, None)?;
                    // Official generator: `State.moveEntity`: if `path.len() > entity.getMP()` the move is rejected (0 PM used).
                    if path.len() > pm as usize {
                        return None;
                    }
                    Some(path)
                })();
                let Some(planned) = planned else {
                    return Ok(Some(Value::Integer(0)));
                };
                for next in planned {
                    let mut w = self.world.borrow_mut();
                    let blocked = w.is_obstacle_cell(next)
                        || w.living_entity_on_cell(next, Some(fid)).is_some();
                    let Some(me) = w.entity_mut(fid) else { break };
                    if me.dead || me.mp <= 0 || blocked {
                        break;
                    }
                    me.mp -= 1;
                    me.cell = next;
                    used += 1;
                    walked.push(next);
                }
                if used > 0 {
                    let end = *walked.last().unwrap_or(&-1);
                    self.world
                        .borrow_mut()
                        .log_action(json!([ACTION_MOVE_TO, fid, end, walked]));
                }
                Ok(Some(Value::Integer(used as i64)))
            }
            "moveTowardCell" => {
                // Official generator: `State.moveTowardCell(entity, cell_id, pm_to_use)`
                let cell_id = arg0_as_i64_strict(args, "moveTowardCell")? as i32;
                let pm_to_use = if args.len() >= 3 {
                    value_as_i64(&args[2]).unwrap_or(-1) as i32
                } else if args.len() == 2 {
                    value_as_i64(&args[1]).unwrap_or(-1) as i32
                } else {
                    -1
                };

                let mut used = 0i32;
                let mut walked: Vec<i32> = Vec::new();

                // Plan once, then walk like the official generator.
                let planned = (|| -> Option<Vec<i32>> {
                    let w = self.world.borrow();
                    let me = w.entity(fid)?;
                    if me.dead || me.mp <= 0 {
                        return None;
                    }
                    let pm = if pm_to_use == -1 {
                        me.mp
                    } else {
                        pm_to_use.min(me.mp)
                    };
                    if pm <= 0 {
                        return None;
                    }
                    let (mw, mh) = (w.map_w, w.map_h);
                    if !map::is_valid_cell(mw, mh, cell_id) {
                        return None;
                    }
                    let start = me.cell;

                    // If the target cell is blocked, aim for an adjacent walkable/empty cell (simplified vs the official generator `getValidCellsAroundObstacle`).
                    let mut goals: Vec<i32> = Vec::new();
                    let target_blocked = w.is_obstacle_cell(cell_id)
                        || w.living_entity_on_cell(cell_id, Some(fid)).is_some();
                    if target_blocked {
                        for g in [
                            cell_id + mw - 1,
                            cell_id - mw,
                            cell_id - mw + 1,
                            cell_id + mw,
                        ] {
                            if !map::is_valid_cell(mw, mh, g) {
                                continue;
                            }
                            if w.is_obstacle_cell(g) {
                                continue;
                            }
                            if w.living_entity_on_cell(g, None).is_some() {
                                continue;
                            }
                            goals.push(g);
                        }
                        if goals.is_empty() {
                            return None;
                        }
                        goals.sort();
                        pathfinding::astar_path(&w, start, &goals, None)
                    } else {
                        pathfinding::get_path_between(&w, start, cell_id, None)
                    }
                })();

                let Some(planned) = planned else {
                    return Ok(Some(Value::Integer(0)));
                };

                for next in planned {
                    let mut w = self.world.borrow_mut();
                    let blocked = w.is_obstacle_cell(next)
                        || w.living_entity_on_cell(next, Some(fid)).is_some();
                    let Some(me) = w.entity_mut(fid) else { break };
                    if me.dead || me.mp <= 0 {
                        break;
                    }
                    if blocked {
                        break;
                    }
                    me.mp -= 1;
                    me.cell = next;
                    used += 1;
                    walked.push(next);
                }

                if used > 0 {
                    let end = *walked.last().unwrap_or(&-1);
                    self.world
                        .borrow_mut()
                        .log_action(json!([ACTION_MOVE_TO, fid, end, walked]));
                }
                Ok(Some(Value::Integer(used as i64)))
            }
            "moveTowardEntities" | "moveTowardLeeks" => {
                // Official generator: `FightClass.moveTowardEntities(leeks, pm_to_use?)`: A* toward any alive target cell.
                // Signature in LeekScript passes an array of entity ids.
                let leeks_val = if args.len() >= 2 { &args[1] } else { &args[0] };
                let pm_to_use = if args.len() >= 3 {
                    value_as_i64(&args[2]).unwrap_or(-1) as i32
                } else {
                    -1
                };
                let mut used = 0i32;
                let mut walked: Vec<i32> = Vec::new();
                let planned = (|| -> Option<Vec<i32>> {
                    let w = self.world.borrow();
                    let me = w.entity(fid)?;
                    if me.dead || me.mp <= 0 {
                        return None;
                    }
                    let pm = if pm_to_use == -1 {
                        me.mp
                    } else {
                        pm_to_use.min(me.mp)
                    };
                    if pm <= 0 {
                        return None;
                    }
                    let start = me.cell;
                    let targets: Vec<i32> = value_i32_vec(leeks_val)
                        .into_iter()
                        .filter_map(|eid| w.entity(eid).filter(|e| !e.dead).map(|e| e.cell))
                        .filter(|&c| map::is_valid_cell(w.map_w, w.map_h, c))
                        .collect();
                    if targets.is_empty() {
                        return None;
                    }
                    pathfinding::get_astar_path_to_any(&w, start, &targets, None)
                })();
                let Some(planned) = planned else {
                    return Ok(Some(Value::Integer(0)));
                };
                for next in planned {
                    let mut w = self.world.borrow_mut();
                    let blocked = w.is_obstacle_cell(next)
                        || w.living_entity_on_cell(next, Some(fid)).is_some();
                    let Some(me) = w.entity_mut(fid) else { break };
                    if me.dead || me.mp <= 0 || blocked {
                        break;
                    }
                    me.mp -= 1;
                    me.cell = next;
                    used += 1;
                    walked.push(next);
                }
                if used > 0 {
                    let end = *walked.last().unwrap_or(&-1);
                    self.world
                        .borrow_mut()
                        .log_action(json!([ACTION_MOVE_TO, fid, end, walked]));
                }
                Ok(Some(Value::Integer(used as i64)))
            }
            "moveTowardCells" => {
                // Official generator: `FightClass.moveTowardCells(cells, pm_to_use?)`: A* toward any target cell.
                let cells_val = if args.len() >= 2 { &args[1] } else { &args[0] };
                let pm_to_use = if args.len() >= 3 {
                    value_as_i64(&args[2]).unwrap_or(-1) as i32
                } else {
                    -1
                };
                let mut used = 0i32;
                let mut walked: Vec<i32> = Vec::new();
                let planned = (|| -> Option<Vec<i32>> {
                    let w = self.world.borrow();
                    let me = w.entity(fid)?;
                    if me.dead || me.mp <= 0 {
                        return None;
                    }
                    let pm = if pm_to_use == -1 {
                        me.mp
                    } else {
                        pm_to_use.min(me.mp)
                    };
                    if pm <= 0 {
                        return None;
                    }
                    let start = me.cell;
                    let targets: Vec<i32> = value_i32_vec(cells_val)
                        .into_iter()
                        .map(|c| c as i32)
                        .filter(|&c| map::is_valid_cell(w.map_w, w.map_h, c))
                        .collect();
                    if targets.is_empty() {
                        return None;
                    }
                    pathfinding::get_astar_path_to_any(&w, start, &targets, None)
                })();
                let Some(planned) = planned else {
                    return Ok(Some(Value::Integer(0)));
                };
                for next in planned {
                    let mut w = self.world.borrow_mut();
                    let blocked = w.is_obstacle_cell(next)
                        || w.living_entity_on_cell(next, Some(fid)).is_some();
                    let Some(me) = w.entity_mut(fid) else { break };
                    if me.dead || me.mp <= 0 || blocked {
                        break;
                    }
                    me.mp -= 1;
                    me.cell = next;
                    used += 1;
                    walked.push(next);
                }
                if used > 0 {
                    let end = *walked.last().unwrap_or(&-1);
                    self.world
                        .borrow_mut()
                        .log_action(json!([ACTION_MOVE_TO, fid, end, walked]));
                }
                Ok(Some(Value::Integer(used as i64)))
            }
            "moveAwayFrom" => {
                // Official generator: `FightClass.moveAwayFrom(entity, mp?)`
                let target = arg0_as_i64_strict(args, "moveAwayFrom")? as i32;
                let pm_to_use = if args.len() >= 3 {
                    value_as_i64(&args[2]).unwrap_or(-1) as i32
                } else if args.len() == 2 {
                    value_as_i64(&args[1]).unwrap_or(-1) as i32
                } else {
                    -1
                };
                let (start, pm, bad_cells) = {
                    let w = self.world.borrow();
                    let Some(me) = w.entity(fid) else {
                        return Ok(Some(Value::Integer(0)));
                    };
                    if me.dead || me.mp <= 0 {
                        return Ok(Some(Value::Integer(0)));
                    }
                    let pm = if pm_to_use == -1 {
                        me.mp
                    } else {
                        pm_to_use.min(me.mp)
                    };
                    let bad = w
                        .entity(target)
                        .filter(|e| !e.dead)
                        .map(|e| vec![e.cell])
                        .unwrap_or_default();
                    (me.cell, pm, bad)
                };
                if pm <= 0 || bad_cells.is_empty() {
                    return Ok(Some(Value::Integer(0)));
                }
                let planned = {
                    let w = self.world.borrow();
                    Self::path_away_from_cells(&w, start, &bad_cells, pm)
                };
                let Some(planned) = planned else {
                    return Ok(Some(Value::Integer(0)));
                };
                let mut used = 0i32;
                let mut walked: Vec<i32> = Vec::new();
                for next in planned {
                    let mut w = self.world.borrow_mut();
                    let blocked = w.is_obstacle_cell(next)
                        || w.living_entity_on_cell(next, Some(fid)).is_some();
                    let Some(me) = w.entity_mut(fid) else { break };
                    if me.dead || me.mp <= 0 || blocked {
                        break;
                    }
                    me.mp -= 1;
                    me.cell = next;
                    used += 1;
                    walked.push(next);
                }
                if used > 0 {
                    let end = *walked.last().unwrap_or(&-1);
                    self.world
                        .borrow_mut()
                        .log_action(json!([ACTION_MOVE_TO, fid, end, walked]));
                }
                Ok(Some(Value::Integer(used as i64)))
            }
            "moveAwayFromCell" => {
                let cell_id = arg0_as_i64_strict(args, "moveAwayFromCell")? as i32;
                let pm_to_use = if args.len() >= 3 {
                    value_as_i64(&args[2]).unwrap_or(-1) as i32
                } else if args.len() == 2 {
                    value_as_i64(&args[1]).unwrap_or(-1) as i32
                } else {
                    -1
                };
                let (start, pm, bad_cells) = {
                    let w = self.world.borrow();
                    let Some(me) = w.entity(fid) else {
                        return Ok(Some(Value::Integer(0)));
                    };
                    if me.dead || me.mp <= 0 {
                        return Ok(Some(Value::Integer(0)));
                    }
                    let pm = if pm_to_use == -1 {
                        me.mp
                    } else {
                        pm_to_use.min(me.mp)
                    };
                    let bad = if map::is_valid_cell(w.map_w, w.map_h, cell_id) {
                        vec![cell_id]
                    } else {
                        Vec::new()
                    };
                    (me.cell, pm, bad)
                };
                if pm <= 0 || bad_cells.is_empty() {
                    return Ok(Some(Value::Integer(0)));
                }
                let planned = {
                    let w = self.world.borrow();
                    Self::path_away_from_cells(&w, start, &bad_cells, pm)
                };
                let Some(planned) = planned else {
                    return Ok(Some(Value::Integer(0)));
                };
                let mut used = 0i32;
                let mut walked: Vec<i32> = Vec::new();
                for next in planned {
                    let mut w = self.world.borrow_mut();
                    let blocked = w.is_obstacle_cell(next)
                        || w.living_entity_on_cell(next, Some(fid)).is_some();
                    let Some(me) = w.entity_mut(fid) else { break };
                    if me.dead || me.mp <= 0 || blocked {
                        break;
                    }
                    me.mp -= 1;
                    me.cell = next;
                    used += 1;
                    walked.push(next);
                }
                if used > 0 {
                    let end = *walked.last().unwrap_or(&-1);
                    self.world
                        .borrow_mut()
                        .log_action(json!([ACTION_MOVE_TO, fid, end, walked]));
                }
                Ok(Some(Value::Integer(used as i64)))
            }
            "moveAwayFromCells" => {
                let cells_val = if args.len() >= 2 { &args[1] } else { &args[0] };
                let pm_to_use = if args.len() >= 3 {
                    value_as_i64(&args[2]).unwrap_or(-1) as i32
                } else {
                    -1
                };
                let (start, pm, bad_cells) = {
                    let w = self.world.borrow();
                    let Some(me) = w.entity(fid) else {
                        return Ok(Some(Value::Integer(0)));
                    };
                    if me.dead || me.mp <= 0 {
                        return Ok(Some(Value::Integer(0)));
                    }
                    let pm = if pm_to_use == -1 {
                        me.mp
                    } else {
                        pm_to_use.min(me.mp)
                    };
                    let bad = value_i32_vec(cells_val)
                        .into_iter()
                        .map(|c| c as i32)
                        .filter(|&c| map::is_valid_cell(w.map_w, w.map_h, c))
                        .collect::<Vec<_>>();
                    (me.cell, pm, bad)
                };
                if pm <= 0 || bad_cells.is_empty() {
                    return Ok(Some(Value::Integer(0)));
                }
                let planned = {
                    let w = self.world.borrow();
                    Self::path_away_from_cells(&w, start, &bad_cells, pm)
                };
                let Some(planned) = planned else {
                    return Ok(Some(Value::Integer(0)));
                };
                let mut used = 0i32;
                let mut walked: Vec<i32> = Vec::new();
                for next in planned {
                    let mut w = self.world.borrow_mut();
                    let blocked = w.is_obstacle_cell(next)
                        || w.living_entity_on_cell(next, Some(fid)).is_some();
                    let Some(me) = w.entity_mut(fid) else { break };
                    if me.dead || me.mp <= 0 || blocked {
                        break;
                    }
                    me.mp -= 1;
                    me.cell = next;
                    used += 1;
                    walked.push(next);
                }
                if used > 0 {
                    let end = *walked.last().unwrap_or(&-1);
                    self.world
                        .borrow_mut()
                        .log_action(json!([ACTION_MOVE_TO, fid, end, walked]));
                }
                Ok(Some(Value::Integer(used as i64)))
            }
            "moveAwayFromEntities" | "moveAwayFromLeeks" => {
                let leeks_val = if args.len() >= 2 { &args[1] } else { &args[0] };
                let pm_to_use = if args.len() >= 3 {
                    value_as_i64(&args[2]).unwrap_or(-1) as i32
                } else {
                    -1
                };
                let (start, pm, bad_cells) = {
                    let w = self.world.borrow();
                    let Some(me) = w.entity(fid) else {
                        return Ok(Some(Value::Integer(0)));
                    };
                    if me.dead || me.mp <= 0 {
                        return Ok(Some(Value::Integer(0)));
                    }
                    let pm = if pm_to_use == -1 {
                        me.mp
                    } else {
                        pm_to_use.min(me.mp)
                    };
                    let bad = value_i32_vec(leeks_val)
                        .into_iter()
                        .filter_map(|eid| w.entity(eid).filter(|e| !e.dead).map(|e| e.cell))
                        .filter(|&c| map::is_valid_cell(w.map_w, w.map_h, c))
                        .collect::<Vec<_>>();
                    (me.cell, pm, bad)
                };
                if pm <= 0 || bad_cells.is_empty() {
                    return Ok(Some(Value::Integer(0)));
                }
                let planned = {
                    let w = self.world.borrow();
                    Self::path_away_from_cells(&w, start, &bad_cells, pm)
                };
                let Some(planned) = planned else {
                    return Ok(Some(Value::Integer(0)));
                };
                let mut used = 0i32;
                let mut walked: Vec<i32> = Vec::new();
                for next in planned {
                    let mut w = self.world.borrow_mut();
                    let blocked = w.is_obstacle_cell(next)
                        || w.living_entity_on_cell(next, Some(fid)).is_some();
                    let Some(me) = w.entity_mut(fid) else { break };
                    if me.dead || me.mp <= 0 || blocked {
                        break;
                    }
                    me.mp -= 1;
                    me.cell = next;
                    used += 1;
                    walked.push(next);
                }
                if used > 0 {
                    let end = *walked.last().unwrap_or(&-1);
                    self.world
                        .borrow_mut()
                        .log_action(json!([ACTION_MOVE_TO, fid, end, walked]));
                }
                Ok(Some(Value::Integer(used as i64)))
            }
            "moveAwayFromLine" => {
                let (cell1, cell2) = pair_user_ints(args, "moveAwayFromLine")?;
                let cell1 = cell1 as i32;
                let cell2 = cell2 as i32;
                let pm_to_use = if args.len() >= 4 {
                    value_as_i64(&args[3]).unwrap_or(-1) as i32
                } else if args.len() == 3 {
                    value_as_i64(&args[2]).unwrap_or(-1) as i32
                } else {
                    -1
                };
                let (start, pm, bad_cells) = {
                    let w = self.world.borrow();
                    let Some(me) = w.entity(fid) else {
                        return Ok(Some(Value::Integer(0)));
                    };
                    if me.dead || me.mp <= 0 {
                        return Ok(Some(Value::Integer(0)));
                    }
                    let pm = if pm_to_use == -1 {
                        me.mp
                    } else {
                        pm_to_use.min(me.mp)
                    };
                    let bad = Self::line_cells_extended(&w, cell1, cell2);
                    (me.cell, pm, bad)
                };
                if pm <= 0 || bad_cells.is_empty() {
                    return Ok(Some(Value::Integer(0)));
                }
                let planned = {
                    let w = self.world.borrow();
                    Self::path_away_from_cells(&w, start, &bad_cells, pm)
                };
                let Some(planned) = planned else {
                    return Ok(Some(Value::Integer(0)));
                };
                let mut used = 0i32;
                let mut walked: Vec<i32> = Vec::new();
                for next in planned {
                    let mut w = self.world.borrow_mut();
                    let blocked = w.is_obstacle_cell(next)
                        || w.living_entity_on_cell(next, Some(fid)).is_some();
                    let Some(me) = w.entity_mut(fid) else { break };
                    if me.dead || me.mp <= 0 || blocked {
                        break;
                    }
                    me.mp -= 1;
                    me.cell = next;
                    used += 1;
                    walked.push(next);
                }
                if used > 0 {
                    let end = *walked.last().unwrap_or(&-1);
                    self.world
                        .borrow_mut()
                        .log_action(json!([ACTION_MOVE_TO, fid, end, walked]));
                }
                Ok(Some(Value::Integer(used as i64)))
            }
            "useWeapon" => {
                let target_fid = arg0_as_i64_loose(args, "useWeapon")? as i32;
                let w = self.world.borrow();
                let (striker_cell, item_id) = match w.entity(fid) {
                    Some(e) if !e.dead => (e.cell, e.equipped_weapon),
                    _ => return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64))),
                };
                let Some(item_id) = item_id else {
                    let log_no_weapon = w
                        .entity(target_fid)
                        .is_some_and(|v| !v.dead && v.fid != fid);
                    drop(w);
                    if log_no_weapon {
                        self.charge_no_weapon_equipped_system_log(fid, trace);
                    }
                    return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                };
                if target_fid < 0 {
                    return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                }
                let victim_cell = match w.entity(target_fid) {
                    Some(v) if !v.dead && v.fid != fid => v.cell,
                    _ => return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64))),
                };
                let (cost, area, effects, weapon_item_id) =
                    if let Some(wstat) = w.weapons_by_item.get(&item_id) {
                        // Official generator: `Map.canUseAttack` = verifyRange + verifyLoS
                        if !self.verify_java_range(
                            striker_cell,
                            victim_cell,
                            wstat.launch_type,
                            wstat.min_range,
                            wstat.max_range,
                        ) {
                            return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                        }
                        if wstat.los && !self.verify_java_los(striker_cell, victim_cell) {
                            return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                        }
                        let striker_tp = w.entity(fid).map(|e| e.tp).unwrap_or(0);
                        if striker_tp < wstat.cost {
                            return Ok(Some(Value::Integer(USE_NOT_ENOUGH_TP as i64)));
                        }
                        (wstat.cost, wstat.area, wstat.effects.clone(), item_id)
                    } else {
                        return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                    };
                drop(w);
                let mut w = self.world.borrow_mut();
                let critical = Self::generate_critical(&mut w, fid);
                let result = if critical { USE_CRITICAL } else { USE_SUCCESS };
                let jet = w.rng.next_double01();
                let cells =
                    Self::area_target_cells(&w, area, striker_cell, victim_cell, 1, 50, false);
                apply_effects_on_cells(
                    &mut w,
                    fid,
                    &cells,
                    &effects,
                    EffectContext {
                        critical,
                        result_code: result,
                        jet,
                        attack_type: 1,
                        item_id: weapon_item_id,
                    },
                    area,
                    victim_cell,
                );
                if let Some(me) = w.entity_mut(fid) {
                    me.tp = (me.tp - cost).max(0);
                }
                // Official generator: ActionUseWeapon logs at use-time regardless of damage details.
                w.log_action(json!([ACTION_USE_WEAPON, victim_cell, result]));
                Ok(Some(Value::Integer(result as i64)))
            }
            "useWeaponOnCell" => {
                let target_cell = arg0_as_i64_strict(args, "useWeaponOnCell")? as i32;
                let w = self.world.borrow();
                let (striker_cell, item_id) = match w.entity(fid) {
                    Some(e) if !e.dead => (e.cell, e.equipped_weapon),
                    _ => return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64))),
                };
                let Some(item_id) = item_id else {
                    let striker_cell = match w.entity(fid) {
                        Some(e) if !e.dead => e.cell,
                        _ => return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64))),
                    };
                    let target_ok = map::is_valid_cell(w.map_w, w.map_h, target_cell)
                        && !w.is_obstacle_cell(target_cell)
                        && target_cell != striker_cell;
                    drop(w);
                    if target_ok {
                        self.charge_no_weapon_equipped_system_log(fid, trace);
                    }
                    return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                };
                if !map::is_valid_cell(w.map_w, w.map_h, target_cell)
                    || w.is_obstacle_cell(target_cell)
                {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                let Some(wstat) = w.weapons_by_item.get(&item_id) else {
                    return Ok(Some(Value::Integer(USE_INVALID_TARGET as i64)));
                };
                if !self.verify_java_range(
                    striker_cell,
                    target_cell,
                    wstat.launch_type,
                    wstat.min_range,
                    wstat.max_range,
                ) || (wstat.los && !self.verify_java_los(striker_cell, target_cell))
                {
                    return Ok(Some(Value::Integer(USE_INVALID_POSITION as i64)));
                }
                let striker_tp = w.entity(fid).map(|e| e.tp).unwrap_or(0);
                if striker_tp < wstat.cost {
                    return Ok(Some(Value::Integer(USE_NOT_ENOUGH_TP as i64)));
                }
                let (cost, area, effects, weapon_item_id) =
                    (wstat.cost, wstat.area, wstat.effects.clone(), item_id);
                drop(w);
                let mut w = self.world.borrow_mut();
                let critical = Self::generate_critical(&mut w, fid);
                let result = if critical { USE_CRITICAL } else { USE_SUCCESS };
                let jet = w.rng.next_double01();
                let cells =
                    Self::area_target_cells(&w, area, striker_cell, target_cell, 1, 50, false);
                apply_effects_on_cells(
                    &mut w,
                    fid,
                    &cells,
                    &effects,
                    EffectContext {
                        critical,
                        result_code: result,
                        jet,
                        attack_type: 1,
                        item_id: weapon_item_id,
                    },
                    area,
                    target_cell,
                );
                if let Some(me) = w.entity_mut(fid) {
                    me.tp = (me.tp - cost).max(0);
                }
                w.log_action(json!([ACTION_USE_WEAPON, target_cell, result]));
                Ok(Some(Value::Integer(result as i64)))
            }
            _ => Ok(None),
        }
    }

    fn leek_fight_registry_ops(&self, name: &str) -> u64 {
        super::registry_ops::fight_functions_registry_ops(name)
    }

    fn java_native_wrapper_ops(&self, _name: &str) -> u64 {
        // Official generator: `FightClass.moveToward` runtime `ai.ops(2000)` is folded into `fight_functions_registry_ops`
        // (`leekscript_run` / `leek_wars_gen` build.rs adds +2000 to the `FightFunctions` table cost).
        // Do not charge again here — `java_native_wrapper_ops` would double-count (see leekgen-compare ops).
        0
    }

    fn take_native_dispatch_extra_ops(&mut self) -> u64 {
        let v = self.native_dispatch_extra_ops.get();
        self.native_dispatch_extra_ops.set(0);
        v
    }

    fn emit_debug_log(
        &mut self,
        kind: DebugLogKind,
        message: &str,
        color_rgb24: Option<u32>,
        position: Option<(i32, i32)>,
    ) -> Result<DebugLogHandled, InterpretError> {
        let fid = self.current_fid();
        let log_owner = self
            .world
            .borrow()
            .entity(fid)
            .map(|e| e.log_bucket_owner)
            .unwrap_or(0);
        // Official generator: `SystemClass.debug*` → `AILog.STANDARD/WARNING/ERROR` + optional color (`debugC`).
        const STANDARD: i32 = 1;
        const WARNING: i32 = 2;
        const ERROR: i32 = 3;
        let (log_type, color) = match kind {
            DebugLogKind::Info => (STANDARD, None),
            DebugLogKind::Warning => (WARNING, None),
            DebugLogKind::Error => (ERROR, None),
            DebugLogKind::Colored => {
                let c = color_rgb24.unwrap_or(0) as i32 & 0x00FF_FFFF;
                (STANDARD, Some(c))
            }
        };
        self.world
            .borrow_mut()
            .push_ai_debug_log(log_owner, fid, log_type, message, color, position);
        Ok(DebugLogHandled::Handled)
    }
}

/// Greedy grid step toward `to` using official-generator-style `(x,y)` neighbors.
// (deprecated) `step_toward_cell`: replaced with shortest-path planning in `moveToward`.

fn int_user_arg(args: &[Value], ctx: &str) -> Result<i64, InterpretError> {
    let v = if args.len() >= 2 {
        args.get(1)
    } else {
        args.get(0)
    };
    let v = v.ok_or_else(|| InterpretError::invalid_parameter_count(1, args.len()))?;
    value_as_i64(v).ok_or_else(|| InterpretError {
        reference: "WRONG_ARGUMENT_TYPE",
        message: format!("{ctx}: expected integer"),
    })
}

fn pair_user_ints(args: &[Value], ctx: &str) -> Result<(i64, i64), InterpretError> {
    let (a, b) = match args.len() {
        n if n >= 3 => {
            let a = args
                .get(1)
                .ok_or_else(|| InterpretError::invalid_parameter_count(2, n))?;
            let b = args
                .get(2)
                .ok_or_else(|| InterpretError::invalid_parameter_count(2, n))?;
            (a, b)
        }
        2 => {
            let a = args
                .get(0)
                .ok_or_else(|| InterpretError::invalid_parameter_count(2, 2))?;
            let b = args
                .get(1)
                .ok_or_else(|| InterpretError::invalid_parameter_count(2, 2))?;
            (a, b)
        }
        n => {
            return Err(InterpretError::invalid_parameter_count(2, n));
        }
    };
    let ai = value_as_i64(a).ok_or_else(|| InterpretError {
        reference: "WRONG_ARGUMENT_TYPE",
        message: format!("{ctx}: expected integer (arg 1)"),
    })?;
    let bi = value_as_i64(b).ok_or_else(|| InterpretError {
        reference: "WRONG_ARGUMENT_TYPE",
        message: format!("{ctx}: expected integer (arg 2)"),
    })?;
    Ok((ai, bi))
}

fn entity_arg_or_current(args: &[Value], current: i32) -> Result<i32, InterpretError> {
    if args.is_empty() {
        return Ok(current);
    }
    let v = if args.len() >= 2 {
        args.get(1)
    } else {
        args.get(0)
    };
    let v = v.ok_or_else(|| InterpretError::invalid_parameter_count(1, args.len()))?;
    match v {
        Value::Null => Ok(current),
        _ => value_as_i64(v)
            .map(|x| x as i32)
            .ok_or_else(|| InterpretError {
                reference: "WRONG_ARGUMENT_TYPE",
                message: "expected integer entity id".into(),
            }),
    }
}

fn value_as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        Value::Real(x) => Some(*x as i64),
        Value::RealDotZero(x) => Some(*x as i64),
        _ => None,
    }
}

fn value_cells_vec(v: &Value) -> Vec<i32> {
    match v {
        Value::Array(a) => a
            .borrow()
            .iter()
            .filter_map(|x| value_as_i64(x).map(|n| n as i32))
            .collect(),
        Value::Null => Vec::new(),
        // Deprecated official-generator overload: ignore entity id -> ignore its cell. We don't have access to world here,
        // so treat it as empty.
        _ => Vec::new(),
    }
}

fn value_i32_vec(v: &Value) -> Vec<i32> {
    match v {
        Value::Array(a) => a
            .borrow()
            .iter()
            .filter_map(|x| value_as_i64(x).map(|n| n as i32))
            .collect(),
        Value::Integer(i) => vec![*i as i32],
        Value::Real(x) => vec![*x as i32],
        Value::RealDotZero(x) => vec![*x as i32],
        Value::Null => Vec::new(),
        _ => Vec::new(),
    }
}

fn pick_user_arg<'a>(args: &'a [Value]) -> Option<&'a Value> {
    if args.len() >= 2 {
        args.get(1)
    } else {
        args.get(0)
    }
}

fn arg0_as_i64_strict(args: &[Value], ctx: &str) -> Result<i64, InterpretError> {
    let v = pick_user_arg(args)
        .ok_or_else(|| InterpretError::invalid_parameter_count(1, args.len()))?;
    value_as_i64(v).ok_or_else(|| InterpretError {
        reference: "WRONG_ARGUMENT_TYPE",
        message: format!("{ctx}: expected integer (args={args:?})"),
    })
}

fn arg0_as_i64_loose(args: &[Value], ctx: &str) -> Result<i64, InterpretError> {
    let v = pick_user_arg(args)
        .ok_or_else(|| InterpretError::invalid_parameter_count(1, args.len()))?;
    match v {
        Value::Null => Ok(-1),
        _ => value_as_i64(v).ok_or_else(|| InterpretError {
            reference: "WRONG_ARGUMENT_TYPE",
            message: format!("{ctx}: expected integer (args={args:?})"),
        }),
    }
}

fn value_debug_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Real(x) => x.to_string(),
        Value::RealDotZero(x) => x.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        _ => "<value>".into(),
    }
}
