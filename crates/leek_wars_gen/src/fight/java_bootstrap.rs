//! Pure-Rust replay of the official generator `State.init` map + RNG setup (procedural `Map.generateMap`, then
//! `StartOrder.compute`), matching `com.leekwars.DumpStateRng` / `generator.jar`.
//!
//! Mirrors `Scenario.fromFile` + `DumpStateRng`: **no** embedded JSON `map` object (official generator `Scenario`
//! does not load it). Uses [`FightWorld::fight_type`] / [`FightWorld::fight_context`] (default `0`).

use crate::engine::JavaFightBootstrap;
use crate::fight::map as lwmap;
use crate::fight::rng::JavaCompatRng;
use crate::fight::start_order::compute_turn_order;
use crate::fight::world::FightWorld;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};

const TYPE_BATTLE_ROYALE: i32 = 3;
const TYPE_CHEST_HUNT: i32 = 6;
const TYPE_CHEST: i32 = 3;
const TYPE_TURRET: i32 = 2;

const CONTEXT_TEST: i32 = 0;
const CONTEXT_TOURNAMENT: i32 = 3;

const DIR_NORTH: i32 = 0;
const DIR_EAST: i32 = 1;
const DIR_SOUTH: i32 = 2;
const DIR_WEST: i32 = 3;

#[derive(Clone)]
struct CellData {
    walkable: bool,
    obstacle: i32,
    size: i32,
    x: i32,
    y: i32,
    composante: i32,
    north: bool,
    west: bool,
    east: bool,
    south: bool,
}

struct GenMap {
    w: i32,
    h: i32,
    map_id: i32,
    custom: bool,
    cells: Vec<CellData>,
    nb_cells: i32,
    min_x: i32,
    #[allow(dead_code)]
    max_x: i32,
    min_y: i32,
    #[allow(dead_code)]
    max_y: i32,
    coord: Vec<Vec<Option<i32>>>,
    occupant: HashMap<i32, i32>,
    map_type: i32,
}

fn edge_flags(width: i32, height: i32, cell_id: i32) -> (bool, bool, bool, bool) {
    let x_raw = cell_id % (width * 2 - 1);
    let y_raw = cell_id / (width * 2 - 1);
    let mut north = true;
    let mut west = true;
    let mut east = true;
    let mut south = true;
    if y_raw == 0 && x_raw < width {
        north = false;
        west = false;
    } else if y_raw + 1 == height && x_raw >= width {
        east = false;
        south = false;
    }
    if x_raw == 0 {
        south = false;
        west = false;
    } else if x_raw + 1 == width {
        north = false;
        east = false;
    }
    (north, west, east, south)
}

impl GenMap {
    fn new(width: i32, height: i32, map_id: i32, custom: bool) -> Self {
        let nb = lwmap::nb_cells(width, height);
        let mut cells = Vec::with_capacity(nb as usize);
        let mut min_x = i32::MAX;
        let mut max_x = i32::MIN;
        let mut min_y = i32::MAX;
        let mut max_y = i32::MIN;
        for id in 0..nb {
            let (x, y) = lwmap::cell_xy(width, id);
            let (n, we, ea, s) = edge_flags(width, height, id);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
            cells.push(CellData {
                walkable: true,
                obstacle: 0,
                size: 0,
                x,
                y,
                composante: 0,
                north: n,
                west: we,
                east: ea,
                south: s,
            });
        }
        let sx = (max_x - min_x + 1) as usize;
        let sy = (max_y - min_y + 1) as usize;
        let mut coord = vec![vec![None; sy]; sx];
        for id in 0..nb {
            let c = &cells[id as usize];
            coord[(c.x - min_x) as usize][(c.y - min_y) as usize] = Some(id);
        }
        Self {
            w: width,
            h: height,
            map_id,
            custom,
            cells,
            nb_cells: nb,
            min_x,
            max_x,
            min_y,
            max_y,
            coord,
            occupant: HashMap::new(),
            map_type: 0,
        }
    }

    fn get_cell_xy(&self, x: i32, y: i32) -> Option<i32> {
        let ix = x - self.min_x;
        let iy = y - self.min_y;
        if ix < 0 || iy < 0 {
            return None;
        }
        self.coord
            .get(ix as usize)
            .and_then(|row| row.get(iy as usize))
            .copied()
            .flatten()
    }

    fn cell_available(&self, cell_id: i32) -> bool {
        if cell_id < 0 || cell_id >= self.nb_cells {
            return false;
        }
        self.cells[cell_id as usize].walkable && !self.occupant.contains_key(&cell_id)
    }

    fn set_obstacle_raw(&mut self, cell_id: i32, obs: i32, sz: i32) {
        let c = &mut self.cells[cell_id as usize];
        c.walkable = false;
        c.obstacle = obs;
        c.size = sz;
    }

    fn clear_obstacle_cell(&mut self, cell_id: i32) {
        let c = &mut self.cells[cell_id as usize];
        c.obstacle = 0;
        c.size = 0;
        c.walkable = true;
    }
}

fn get_cell_by_dir(g: &GenMap, cell_id: i32, dir: i32) -> Option<i32> {
    if cell_id < 0 || cell_id >= g.nb_cells {
        return None;
    }
    let c = &g.cells[cell_id as usize];
    let nid = match dir {
        DIR_NORTH if c.north => cell_id - g.w + 1,
        DIR_WEST if c.west => cell_id - g.w,
        DIR_EAST if c.east => cell_id + g.w,
        DIR_SOUTH if c.south => cell_id + g.w - 1,
        _ => return None,
    };
    if nid >= 0 && nid < g.nb_cells {
        Some(nid)
    } else {
        None
    }
}

fn case_distance(w: i32, a: i32, b: i32) -> i32 {
    let (x1, y1) = lwmap::cell_xy(w, a);
    let (x2, y2) = lwmap::cell_xy(w, b);
    (x1 - x2).abs() + (y1 - y2).abs()
}

fn min_dist_team(
    world: &FightWorld,
    team: usize,
    cell: i32,
    placement: &HashMap<i32, i32>,
) -> i32 {
    let mut min = i32::MAX;
    let Some(fids) = world.team_fids.get(team) else {
        return min;
    };
    for &fid in fids {
        let Some(e) = world.entity(fid) else {
            continue;
        };
        if e.dead {
            continue;
        }
        let Some(&ec) = placement.get(&fid) else {
            continue;
        };
        let d = case_distance(world.map_w, ec, cell);
        min = min.min(d);
    }
    min
}

fn team_has_placed_cell(world: &FightWorld, team: usize, placement: &HashMap<i32, i32>) -> bool {
    world
        .team_fids
        .get(team)
        .map(|fids| {
            fids.iter().any(|&fid| {
                world
                    .entity(fid)
                    .map(|e| !e.dead && placement.contains_key(&fid))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn get_random_cell(g: &mut GenMap, rng: &mut JavaCompatRng) -> Option<i32> {
    let mut retour: Option<i32> = None;
    let mut nb = 0u32;
    loop {
        let bad = match retour {
            None => true,
            Some(id) => !g.cell_available(id),
        };
        if !bad {
            break;
        }
        let idx = rng.next_int_inclusive(0, g.nb_cells);
        retour = if idx >= 0 && idx < g.nb_cells {
            Some(idx)
        } else {
            None
        };
        // Official generator: `if (nb++ > 64) break;` (post-increment: compare old `nb`, then increment).
        let old = nb;
        nb += 1;
        if old > 64 {
            break;
        }
    }
    retour
}

fn get_random_cell_part(g: &mut GenMap, rng: &mut JavaCompatRng, part: i32) -> Option<i32> {
    let mut retour: Option<i32> = None;
    let mut nb = 0u32;
    loop {
        let bad = match retour {
            None => true,
            Some(id) => !g.cell_available(id),
        };
        if !bad {
            break;
        }
        let y = rng.next_int_inclusive(0, g.h - 1);
        let x = rng.next_int_inclusive(0, g.w / 4);
        let mut cellid = y * (g.w * 2 - 1);
        // Match Java `cellid += (part - 1) * width / 4 + x` (`*`/`/` left-associative: `((part-1)*width)/4 + x`).
        cellid += (part - 1) * g.w / 4 + x;
        retour = if cellid >= 0 && cellid < g.nb_cells {
            Some(cellid)
        } else {
            None
        };
        let old = nb;
        nb += 1;
        if old > 64 {
            break;
        }
    }
    retour
}

fn get_random_cell_near_center(
    g: &mut GenMap,
    rng: &mut JavaCompatRng,
    max_distance: i32,
) -> Option<i32> {
    let center = g.nb_cells / 2;
    let mut possible: Vec<i32> = Vec::new();
    for id in 0..g.nb_cells {
        if g.cell_available(id) && case_distance(g.w, id, center) <= max_distance {
            possible.push(id);
        }
    }
    if !possible.is_empty() {
        let i = rng.next_int_inclusive(0, possible.len() as i32 - 1);
        return Some(possible[i as usize]);
    }
    get_random_cell(g, rng)
}

fn get_random_cell_away_from_center(
    g: &mut GenMap,
    rng: &mut JavaCompatRng,
    min_distance: i32,
) -> Option<i32> {
    let center = g.nb_cells / 2;
    let mut possible: Vec<i32> = Vec::new();
    for id in 0..g.nb_cells {
        if g.cell_available(id) && case_distance(g.w, id, center) >= min_distance {
            possible.push(id);
        }
    }
    if !possible.is_empty() {
        let i = rng.next_int_inclusive(0, possible.len() as i32 - 1);
        return Some(possible[i as usize]);
    }
    get_random_cell(g, rng)
}

fn get_cell_equal_distance(
    world: &FightWorld,
    g: &mut GenMap,
    rng: &mut JavaCompatRng,
    placement: &HashMap<i32, i32>,
) -> Option<i32> {
    let mut possible: Vec<i32> = Vec::new();
    for id in 0..g.nb_cells {
        if !g.cell_available(id) {
            continue;
        }
        let d0 = min_dist_team(world, 0, id, placement);
        let d1 = min_dist_team(world, 1, id, placement);
        if (d0 - d1).abs() < 2 {
            possible.push(id);
        }
    }
    if !possible.is_empty() {
        let i = rng.next_int_inclusive(0, possible.len() as i32 - 1);
        return Some(possible[i as usize]);
    }
    get_random_cell(g, rng)
}

fn remove_obstacle(g: &mut GenMap, cell_id: i32) {
    if cell_id < 0 || cell_id >= g.nb_cells {
        return;
    }
    let sz = g.cells[cell_id as usize].size;
    if sz > 0 {
        if sz == 2 {
            if let Some(c2) = get_cell_by_dir(g, cell_id, DIR_EAST) {
                if let Some(c3) = get_cell_by_dir(g, cell_id, DIR_SOUTH) {
                    if let Some(c4) = get_cell_by_dir(g, c3, DIR_EAST) {
                        g.clear_obstacle_cell(c2);
                        g.clear_obstacle_cell(c3);
                        g.clear_obstacle_cell(c4);
                    }
                }
            }
        }
        g.clear_obstacle_cell(cell_id);
    }
}

fn get_cells_in_circle(g: &GenMap, center_id: i32, radius: i32) -> Vec<i32> {
    let (cx, cy) = lwmap::cell_xy(g.w, center_id);
    let mut out = Vec::new();
    for x in (cx - radius)..=(cx + radius) {
        for y in (cy - radius)..=(cy + radius) {
            if let Some(id) = g.get_cell_xy(x, y) {
                out.push(id);
            }
        }
    }
    out
}

fn compute_composantes(g: &mut GenMap) {
    let sx = g.coord.len();
    let sy = if sx > 0 { g.coord[0].len() } else { 0 };
    let mut connexe = vec![vec![-1i32; sy]; sx];
    let mut ni = 1i32;

    for x in 0..sx {
        for y in 0..sy {
            let Some(cid) = g.coord[x][y] else {
                continue;
            };
            let c = &g.cells[cid as usize];
            let mut cur_number = 0i32;

            if x > 0 {
                if let Some(left_id) = g.coord[x - 1][y] {
                    let left = &g.cells[left_id as usize];
                    if left.walkable == c.walkable {
                        cur_number = connexe[x - 1][y];
                    }
                }
            }

            if y > 0 {
                if let Some(up_id) = g.coord[x][y - 1] {
                    let up = &g.cells[up_id as usize];
                    if up.walkable == c.walkable {
                        if cur_number == 0 {
                            cur_number = connexe[x][y - 1];
                        } else if cur_number != connexe[x][y - 1] {
                            let target_number = connexe[x][y - 1];
                            for x2 in 0..sx {
                                for y2 in 0..=y {
                                    if connexe[x2][y2] == target_number {
                                        connexe[x2][y2] = cur_number;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if cur_number == 0 {
                connexe[x][y] = ni;
                ni += 1;
            } else {
                connexe[x][y] = cur_number;
            }
        }
    }

    for id in 0..g.nb_cells {
        let c = &g.cells[id as usize];
        let ix = (c.x - g.min_x) as usize;
        let iy = (c.y - g.min_y) as usize;
        g.cells[id as usize].composante = connexe[ix][iy];
    }
}

fn try_procedural_attempt(
    world: &FightWorld,
    state_type: i32,
    obstacle_count: i32,
    rng: &mut JavaCompatRng,
    width: i32,
    height: i32,
) -> (GenMap, HashMap<i32, i32>, Vec<i32>) {
    let mut g = GenMap::new(width, height, 0, false);
    let mut placement: HashMap<i32, i32> = HashMap::new();

    for _ in 0..obstacle_count {
        let idx = rng.next_int_inclusive(0, g.nb_cells);
        let c = if idx >= 0 && idx < g.nb_cells {
            Some(idx)
        } else {
            None
        };
        if let Some(cid) = c {
            if g.cell_available(cid) {
                let mut size = rng.next_int_inclusive(1, 2);
                let obs_type = rng.next_int_inclusive(0, 2);
                if size == 2 {
                    if let Some(c2) = get_cell_by_dir(&g, cid, DIR_EAST) {
                        if let Some(c3) = get_cell_by_dir(&g, cid, DIR_SOUTH) {
                            if let Some(c4) = get_cell_by_dir(&g, c3, DIR_EAST) {
                                let ok = g.cell_available(c2)
                                    && g.cell_available(c3)
                                    && g.cell_available(c4);
                                if !ok {
                                    size = 1;
                                } else {
                                    g.set_obstacle_raw(c2, 0, -1);
                                    g.set_obstacle_raw(c3, 0, -2);
                                    g.set_obstacle_raw(c4, 0, -3);
                                }
                            } else {
                                size = 1;
                            }
                        } else {
                            size = 1;
                        }
                    } else {
                        size = 1;
                    }
                }
                g.set_obstacle_raw(cid, obs_type, size);
            }
        }
    }

    compute_composantes(&mut g);

    let mut leeks_order: Vec<i32> = Vec::new();

    for t in 0..world.team_fids.len() {
        for &fid in &world.team_fids[t] {
            let Some(ent) = world.entity(fid) else {
                continue;
            };
            let c: Option<i32> = if state_type == TYPE_BATTLE_ROYALE {
                get_random_cell(&mut g, rng)
            } else if state_type == TYPE_CHEST_HUNT {
                if ent.entity_type == TYPE_CHEST {
                    get_random_cell_near_center(&mut g, rng, 3)
                } else {
                    get_random_cell_away_from_center(&mut g, rng, 12)
                }
            } else if ent.entity_type == TYPE_CHEST {
                let t0 = team_has_placed_cell(world, 0, &placement);
                let t1 = world.team_fids.len() > 1 && team_has_placed_cell(world, 1, &placement);
                if t0 && t1 {
                    get_cell_equal_distance(world, &mut g, rng, &placement)
                } else {
                    get_random_cell(&mut g, rng)
                }
            } else {
                let part = if t == 0 { 1 } else { 4 };
                get_random_cell_part(&mut g, rng, part)
            };

            if let Some(cell_id) = c {
                g.occupant.insert(cell_id, fid);
                placement.insert(fid, cell_id);
                leeks_order.push(fid);

                if ent.entity_type == TYPE_TURRET {
                    for cell in get_cells_in_circle(&g, cell_id, 5) {
                        remove_obstacle(&mut g, cell);
                    }
                }
            }
        }
    }

    compute_composantes(&mut g);

    (g, placement, leeks_order)
}

fn generate_procedural_map(
    world: &FightWorld,
    width: i32,
    height: i32,
    state_type: i32,
    obstacle_count: i32,
    rng: &mut JavaCompatRng,
) -> (GenMap, HashMap<i32, i32>) {
    let mut valid = false;
    let mut nb = 0;
    let mut last_g = GenMap::new(width, height, 0, false);
    let mut last_p = HashMap::new();

    while !valid && nb < 63 {
        let (g, p, leeks) = try_procedural_attempt(
            world,
            state_type,
            obstacle_count,
            rng,
            width,
            height,
        );
        last_g = g;
        last_p = p;
        valid = true;
        if !leeks.is_empty() {
            if let Some(&first) = leeks.first() {
                if let Some(&fc) = last_p.get(&first) {
                    let comp0 = last_g.cells[fc as usize].composante;
                    for fid in leeks.iter().skip(1) {
                        let Some(&cid) = last_p.get(fid) else {
                            valid = false;
                            break;
                        };
                        if last_g.cells[cid as usize].composante != comp0 {
                            valid = false;
                            break;
                        }
                    }
                } else {
                    valid = false;
                }
            } else {
                valid = false;
            }
        }
        nb += 1;
    }

    let mut map_type = rng.next_int_inclusive(0, 4);
    if world.fight_context == CONTEXT_TEST {
        map_type = -1;
    } else if world.fight_context == CONTEXT_TOURNAMENT {
        map_type = 5;
    }
    last_g.map_type = map_type;

    (last_g, last_p)
}

fn bootstrap_obstacle_stored_value(v: &Value) -> Option<i32> {
    if let Some(i) = v.as_i64() {
        return Some(i as i32);
    }
    if let Some(arr) = v.as_array() {
        if let Some(s) = arr.get(1).and_then(|x| x.as_i64()) {
            return Some(s as i32);
        }
        if let Some(s) = arr.get(0).and_then(|x| x.as_i64()) {
            return Some(s as i32);
        }
    }
    None
}

fn build_obstacle_exports(g: &GenMap) -> (BTreeMap<i32, i32>, Value) {
    let iter_len = (g.w * 2 - 1) * g.h;
    let mut obstacles = BTreeMap::new();
    let mut outcome_map = Map::new();

    for i in 0..iter_len {
        let c = if i >= 0 && i < g.nb_cells {
            &g.cells[i as usize]
        } else {
            continue;
        };
        if c.walkable {
            continue;
        }
        let sz = c.size;
        let id = i;

        let obs_val = if sz <= 0 {
            if g.custom {
                json!([c.obstacle, 1])
            } else {
                json!(1)
            }
        } else if g.map_id != 0 {
            json!(c.obstacle)
        } else if g.custom {
            json!([c.obstacle, c.size])
        } else {
            json!(c.size)
        };

        if let Some(stored) = bootstrap_obstacle_stored_value(&obs_val) {
            obstacles.insert(id, stored);
        }

        if sz > 0 {
            let out_val = if g.map_id != 0 {
                json!(c.obstacle)
            } else if g.custom {
                json!([c.obstacle, c.size])
            } else {
                json!(c.size)
            };
            outcome_map.insert(id.to_string(), out_val);
        }
    }

    (obstacles, Value::Object(outcome_map))
}

/// Same post-`State.init` snapshot as [`crate::engine::dump_java_fight_bootstrap`], without spawning `DumpStateRng`.
pub fn compute_java_fight_bootstrap(world: &FightWorld) -> JavaFightBootstrap {
    let mut rng = JavaCompatRng::new(world.seed);
    let obstacle_count = rng.next_int_inclusive(30, 80);
    let width = world.map_w;
    let height = world.map_h;
    let state_type = world.fight_type;

    let (genmap, entity_cells) =
        generate_procedural_map(world, width, height, state_type, obstacle_count, &mut rng);

    let initial_fids = compute_turn_order(world, &mut rng);
    let rng_internal_n = rng.internal_n();

    let (obstacles, outcome_obstacles) = build_obstacle_exports(&genmap);

    JavaFightBootstrap {
        rng_internal_n,
        initial_fids,
        entity_cells,
        obstacles,
        outcome_obstacles: Some(outcome_obstacles),
        map_w: width,
        map_h: height,
        map_type: genmap.map_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::Scenario;
    use std::path::Path;

    /// Java: `cellid += (part - 1) * width / 4 + x` is `((part-1)*width)/4 + x`, not `(part-1)*(width/4) + x`.
    #[test]
    fn get_random_cell_part_formula_matches_java_precedence() {
        let w = 18i32;
        let part = 4i32;
        let y = 14i32;
        let x = 4i32;
        let row = y * (w * 2 - 1);
        let wrong = row + (part - 1) * (w / 4) + x;
        let java = row + (part - 1) * w / 4 + x;
        assert_eq!(wrong, 506);
        assert_eq!(java, 507);
    }

    #[test]
    fn scenario1_obstacles_match_java_fixture() {
        let java_raw = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/scenario1_java_obstacles.json"
        ));
        let java_val: serde_json::Value = serde_json::from_str(java_raw).expect("parse fixture");
        let java_obj = java_val.as_object().expect("object");

        let j = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../leek-wars-generator/test/scenario/scenario1.json"
        ));
        let sc: Scenario = serde_json::from_str(j).expect("parse");
        let weapons = crate::fight::load_weapons_json(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator/data/weapons.json"),
        )
        .expect("weapons");
        let chips = crate::fight::load_chips_json(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator/data/chips.json"),
        )
        .expect("chips");
        let summons = crate::fight::load_summons_json(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator/data/summons.json"),
        )
        .expect("summons");
        let world = FightWorld::from_scenario(&sc, weapons, chips, summons);
        let boot = compute_java_fight_bootstrap(&world);

        assert_eq!(boot.obstacles.len(), java_obj.len(), "obstacle key count");
        for (k, v) in java_obj {
            let id: i32 = k.parse().expect("key");
            let exp = v.as_i64().expect("int val") as i32;
            assert_eq!(
                boot.obstacles.get(&id).copied(),
                Some(exp),
                "obstacle cell {id}"
            );
        }
    }

    #[test]
    #[ignore = "debug helper: run with --ignored --nocapture to print bootstrap"]
    fn debug_print_scenario1_bootstrap() {
        let j = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../leek-wars-generator/test/scenario/scenario1.json"
        ));
        let sc: Scenario = serde_json::from_str(j).expect("parse");
        let weapons = crate::fight::load_weapons_json(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator/data/weapons.json"),
        )
        .expect("weapons");
        let chips = crate::fight::load_chips_json(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator/data/chips.json"),
        )
        .expect("chips");
        let summons = crate::fight::load_summons_json(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator/data/summons.json"),
        )
        .expect("summons");
        let world = FightWorld::from_scenario(&sc, weapons, chips, summons);
        let boot = compute_java_fight_bootstrap(&world);
        eprintln!(
            "rng_n={}\ninitial_fids={:?}\nentity_cells={:?}",
            boot.rng_internal_n, boot.initial_fids, boot.entity_cells
        );
    }

    #[test]
    fn scenario1_bootstrap_matches_known_generator_rng_n() {
        let j = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../leek-wars-generator/test/scenario/scenario1.json"
        ));
        let sc: Scenario = serde_json::from_str(j).expect("parse");
        let weapons = crate::fight::load_weapons_json(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator/data/weapons.json"),
        )
        .expect("weapons");
        let chips = crate::fight::load_chips_json(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator/data/chips.json"),
        )
        .expect("chips");
        let summons = crate::fight::load_summons_json(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator/data/summons.json"),
        )
        .expect("summons");
        let world = FightWorld::from_scenario(&sc, weapons, chips, summons);
        let boot = compute_java_fight_bootstrap(&world);
        assert_eq!(
            boot.rng_internal_n,
            -5_933_333_234_847_835_179_i64,
            "Rust bootstrap RNG state must match DumpStateRng from the official generator line 1"
        );
        assert_eq!(boot.initial_fids, vec![0, 3, 1, 2]);
        assert_eq!(boot.entity_cells.get(&0), Some(&1));
        assert_eq!(boot.entity_cells.get(&1), Some(&492));
        assert_eq!(boot.entity_cells.get(&2), Some(&332));
        assert_eq!(boot.entity_cells.get(&3), Some(&225));
        assert_eq!(boot.map_type, -1);
    }
}
