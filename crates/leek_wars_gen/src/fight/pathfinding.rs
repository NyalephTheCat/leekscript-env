//! Official-generator-style pathfinding helpers (`Map.getAStarPath` subset).

use super::java_weight_open::{JavaWeightTree, OpenKey};
use super::map;
use super::world::FightWorld;
use std::collections::{HashMap, HashSet};

fn heuristic_f32(world: &FightWorld, a: i32, b: i32) -> f32 {
    // Official generator: getDistance = sqrt(distance2) (double), then cast to float.
    let d2 = f64::from(map::distance2(world.map_w, a, b));
    d2.sqrt() as f32
}

/// Official generator: `Map.getAStarPath`: start is `c1.weight = 0` (not `cost + getDistance`). Every other cell in
/// the open set uses `weight = cost + (float) getDistance(cell, goal)`.
fn open_weight(
    cell: i32,
    start: i32,
    best_g: &HashMap<i32, i32>,
    world: &FightWorld,
    goal0: i32,
) -> f32 {
    if cell == start {
        return 0.0;
    }
    best_g.get(&cell).map_or(f32::INFINITY, |g| {
        *g as f32 + heuristic_f32(world, cell, goal0)
    })
}

fn neighbors(world: &FightWorld, u: i32) -> Vec<i32> {
    let w = world.map_w;
    let h = world.map_h;

    // Match `com.leekwars.generator.maps.Cell` edge flags + `Map.getCellsAround` ordering.
    // Note: validity is *not* just `0..nb_cells`: some ids inside the range have missing directions.
    let x_raw = u.rem_euclid(w * 2 - 1);
    let y_raw = u.div_euclid(w * 2 - 1);

    let mut north = true;
    let mut west = true;
    let mut east = true;
    let mut south = true;

    if y_raw == 0 && x_raw < w {
        north = false;
        west = false;
    } else if y_raw + 1 == h && x_raw >= w {
        east = false;
        south = false;
    }
    if x_raw == 0 {
        south = false;
        west = false;
    } else if x_raw + 1 == w {
        north = false;
        east = false;
    }

    let mut out = Vec::with_capacity(4);
    // SOUTH, WEST, NORTH, EAST
    if south {
        out.push(u + w - 1);
    }
    if west {
        out.push(u - w);
    }
    if north {
        out.push(u - w + 1);
    }
    if east {
        out.push(u + w);
    }
    out
}

/// A* path from `start` to any `goals` (cell ids). Returns the official-generator-style path: excludes `start`,
/// may include a goal cell, but if the last cell is occupied (and not ignored) it is removed.
///
/// Open set matches official generator `TreeSet` backed by `TreeMap` with `Map$2` comparator
/// `(w_a > w_b) ? 1 : -1` on `weight = cost + heuristic` (f32).
pub fn astar_path(
    world: &FightWorld,
    start: i32,
    goals: &[i32],
    ignore_cells: Option<&[i32]>,
) -> Option<Vec<i32>> {
    astar_path_inner(world, start, goals, ignore_cells, None)
}

fn astar_path_inner(
    world: &FightWorld,
    start: i32,
    goals: &[i32],
    ignore_cells: Option<&[i32]>,
    mut probe: Option<&mut String>,
) -> Option<Vec<i32>> {
    let mut push_probe = |line: &str| {
        if let Some(s) = probe.as_mut() {
            s.push_str(line);
        }
    };
    if goals.is_empty() || goals.contains(&start) {
        return None;
    }

    let goal0 = goals[0];
    let ignore: HashSet<i32> = ignore_cells
        .map(|v| v.iter().copied().collect())
        .unwrap_or_default();

    let mut open = JavaWeightTree::new();
    let mut came_from: HashMap<i32, i32> = HashMap::new();
    let mut best_g: HashMap<i32, i32> = HashMap::new();
    // Official generator: marks cells when first discovered for open; better costs update `best_g` / parent but do
    // not add another open entry or rebalance the tree.
    let mut discovered: HashSet<i32> = HashSet::new();
    let mut closed: HashSet<i32> = HashSet::new();

    best_g.insert(start, 0);
    came_from.insert(start, start);
    let mut seq: u32 = 0;
    {
        let w0 = open_weight(start, start, &best_g, world, goal0);
        push_probe(&format!("u {} {}\n", start, w0.to_bits() as i32));
        push_probe(&format!("i {start}\n"));
        let w = |cell: i32| open_weight(cell, start, &best_g, world, goal0);
        open.insert(OpenKey { cell: start, seq }, &w);
    }
    discovered.insert(start);
    seq = seq.wrapping_add(1);

    let debug_polls = std::env::var_os("LEEK_ASTAR_DEBUG").is_some();
    while !open.is_empty() {
        push_probe("p\n");
        let k = open.poll_first().expect("open non-empty");
        let cur_cell = k.cell;
        if debug_polls {
            eprintln!("{cur_cell}");
        }
        let cur_g = *best_g
            .get(&cur_cell)
            .expect("cell popped from open must have best_g");
        if closed.contains(&cur_cell) {
            continue;
        }
        closed.insert(cur_cell);

        if goals.contains(&cur_cell) {
            let mut out = Vec::new();
            let mut at = cur_cell;
            while at != start {
                out.push(at);
                at = *came_from.get(&at)?;
            }
            out.reverse();

            if let Some(&last) = out.last() {
                let occupied = world.living_entity_on_cell(last, None).is_some();
                if occupied && !ignore.contains(&last) {
                    out.pop();
                }
            }
            return Some(out);
        }

        for v in neighbors(world, cur_cell) {
            if !map::is_valid_cell(world.map_w, world.map_h, v) {
                continue;
            }
            if world.is_obstacle_cell(v) {
                continue;
            }
            if world.living_entity_on_cell(v, None).is_some()
                && !ignore.contains(&v)
                && !goals.contains(&v)
            {
                continue;
            }
            if closed.contains(&v) {
                continue;
            }

            let tentative_g = cur_g + 1;
            let prev_best = best_g.get(&v).copied();
            let improve = match prev_best {
                None => true,
                Some(pb) => tentative_g < pb,
            };
            if improve {
                best_g.insert(v, tentative_g);
                came_from.insert(v, cur_cell);
                let wv = open_weight(v, start, &best_g, world, goal0);
                push_probe(&format!("u {} {}\n", v, wv.to_bits() as i32));
                if !discovered.contains(&v) {
                    push_probe(&format!("i {v}\n"));
                    {
                        let w = |cell: i32| open_weight(cell, start, &best_g, world, goal0);
                        open.insert(OpenKey { cell: v, seq }, &w);
                    }
                    discovered.insert(v);
                    seq = seq.wrapping_add(1);
                }
            }
        }
    }

    None
}

/// [`astar_path`] with a probe script for `tools/TreeSetWeightProbe.java` (`u` / `i` / `p`, live weights).
#[must_use]
pub fn astar_path_probe_script(
    world: &FightWorld,
    start: i32,
    goals: &[i32],
    ignore_cells: Option<&[i32]>,
) -> (Option<Vec<i32>>, String) {
    let mut script = String::new();
    let path = astar_path_inner(world, start, goals, ignore_cells, Some(&mut script));
    (path, script)
}

/// Official generator: `Map.getPathBeetween(start, end, cells_to_ignore)` wrapper.
#[must_use]
pub fn get_path_between(
    world: &FightWorld,
    start: i32,
    end: i32,
    ignore_cells: Option<&[i32]>,
) -> Option<Vec<i32>> {
    astar_path(world, start, &[end], ignore_cells)
}

/// Official generator: `Map.getAStarPath(start, endCells, cells_to_ignore)` wrapper.
///
/// Picks the first reached goal during A* search, using `goals[0]` for heuristic like the official generator does.
pub fn get_astar_path_to_any(
    world: &FightWorld,
    start: i32,
    goals: &[i32],
    ignore_cells: Option<&[i32]>,
) -> Option<Vec<i32>> {
    astar_path(world, start, goals, ignore_cells)
}
