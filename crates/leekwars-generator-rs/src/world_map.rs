use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct Rng {
    pub n: i64,
}

impl Rng {
    #[must_use]
    pub fn new(seed: i64) -> Self {
        Self { n: seed }
    }

    #[must_use]
    pub fn next_double_0_1(&mut self) -> f64 {
        // Match Java `long` overflow + signed division/mod semantics.
        self.n = self.n.wrapping_mul(1103515245).wrapping_add(12345);
        let r = ((self.n / 65536) % 32768) + 32768;
        (r as f64) / 65536.0
    }

    #[must_use]
    pub fn int_inclusive(&mut self, min: i32, max: i32) -> i32 {
        if (max as i64) - (min as i64) + 1 <= 0 {
            return 0;
        }
        let span = (max - min + 1) as f64;
        min + (self.next_double_0_1() * span) as i32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    North = 0,
    East = 1,
    South = 2,
    West = 3,
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub id: i32,
    pub x: i32,
    pub y: i32,

    pub walkable: bool,
    pub obstacle_id: i32,
    pub obstacle_size: i32,

    pub north: bool,
    pub south: bool,
    pub west: bool,
    pub east: bool,
}

impl Cell {
    pub fn available(&self, occupied: &HashSet<i32>) -> bool {
        self.walkable && !occupied.contains(&self.id)
    }
}

#[derive(Debug, Clone)]
pub struct WorldMap {
    pub width: i32,
    pub height: i32,
    pub nb_cells: i32,
    pub map_type: i32,

    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,

    /// Index by cell id.
    pub cells: Vec<Cell>,
    /// Coordinate lookup (x,y) -> cell id.
    coord: HashMap<(i32, i32), i32>,
}

impl WorldMap {
    #[must_use]
    pub fn new(width: i32, height: i32) -> Self {
        // nb_cells = (width * 2 - 1) * height - (width - 1)
        let nb_cells = (width * 2 - 1) * height - (width - 1);
        let mut cells = Vec::with_capacity(nb_cells as usize);
        let mut coord: HashMap<(i32, i32), i32> = HashMap::new();

        let mut min_x: i32 = i32::MAX;
        let mut max_x: i32 = i32::MIN;
        let mut min_y: i32 = i32::MAX;
        let mut max_y: i32 = i32::MIN;

        for id in 0..nb_cells {
            let c = Self::cell_from_id(width, height, id);
            min_x = min_x.min(c.x);
            max_x = max_x.max(c.x);
            min_y = min_y.min(c.y);
            max_y = max_y.max(c.y);
            coord.insert((c.x, c.y), id);
            cells.push(c);
        }

        Self {
            width,
            height,
            nb_cells,
            map_type: 0,
            min_x,
            max_x,
            min_y,
            max_y,
            cells,
            coord,
        }
    }

    fn cell_from_id(width: i32, height: i32, id: i32) -> Cell {
        let mut north = true;
        let mut west = true;
        let mut east = true;
        let mut south = true;

        let w2m1 = width * 2 - 1;
        let x0 = id % w2m1;
        let y0 = id / w2m1;

        if y0 == 0 && x0 < width {
            north = false;
            west = false;
        } else if y0 + 1 == height && x0 >= width {
            east = false;
            south = false;
        }
        if x0 == 0 {
            south = false;
            west = false;
        } else if x0 + 1 == width {
            north = false;
            east = false;
        }

        let y = y0 - (x0 % width);
        let x = (id - (width - 1) * y) / width;

        let _ = height;
        Cell {
            id,
            x,
            y,
            walkable: true,
            obstacle_id: 0,
            obstacle_size: 0,
            north,
            south,
            west,
            east,
        }
    }

    #[must_use]
    pub fn get_cell(&self, id: i32) -> Option<&Cell> {
        self.cells.get(id as usize)
    }

    #[must_use]
    pub fn get_cell_mut(&mut self, id: i32) -> Option<&mut Cell> {
        self.cells.get_mut(id as usize)
    }

    #[must_use]
    pub fn get_cell_xy(&self, x: i32, y: i32) -> Option<i32> {
        self.coord.get(&(x, y)).copied()
    }

    #[must_use]
    pub fn get_cell_by_dir(&self, id: i32, dir: Dir) -> Option<i32> {
        let c = self.get_cell(id)?;
        match dir {
            Dir::North if c.north => Some(id - self.width + 1),
            Dir::West if c.west => Some(id - self.width),
            Dir::East if c.east => Some(id + self.width),
            Dir::South if c.south => Some(id + self.width - 1),
            _ => None,
        }
        .filter(|nid| *nid >= 0 && *nid < self.nb_cells)
    }

    #[must_use]
    pub fn neighbors4(&self, id: i32) -> Vec<i32> {
        let mut out = Vec::with_capacity(4);
        for d in [Dir::South, Dir::West, Dir::North, Dir::East] {
            if let Some(n) = self.get_cell_by_dir(id, d) {
                out.push(n);
            }
        }
        out
    }

    #[must_use]
    pub fn case_distance(&self, a: i32, b: i32) -> Option<i32> {
        let ca = self.get_cell(a)?;
        let cb = self.get_cell(b)?;
        Some((ca.x - cb.x).abs() + (ca.y - cb.y).abs())
    }

    #[must_use]
    pub fn verify_los(
        &self,
        start: i32,
        end: i32,
        need_los: bool,
        occupied: &HashSet<i32>,
        ignored: &HashSet<i32>,
    ) -> bool {
        if !need_los {
            return true;
        }
        let Some(s) = self.get_cell(start) else { return false };
        let Some(e) = self.get_cell(end) else { return false };

        let a = (s.y - e.y).abs();
        let b = (s.x - e.x).abs();
        let dx = if s.x > e.x { -1 } else { 1 };
        let dy = if s.y < e.y { 1 } else { -1 };
        let mut path: Vec<i32> = Vec::with_capacity(((b + 1) * 2) as usize);

        if b == 0 {
            path.push(0);
            path.push(a + 1);
        } else {
            let d = (a as f64) / (b as f64) / 2.0;
            let mut h: i32 = 0;
            for i in 0..b {
                let y = 0.5 + ((i * 2 + 1) as f64) * d;
                path.push(h);
                path.push((y - 0.00001).ceil() as i32 - h);
                h = (y + 0.00001).floor() as i32;
            }
            path.push(h);
            path.push(a + 1 - h);
        }

        for p in (0..path.len()).step_by(2) {
            let seg_len = path[p + 1];
            for i in 0..seg_len {
                let cx = s.x + ((p as i32) / 2) * dx;
                let cy = s.y + (path[p] + i) * dy;
                let Some(cid) = self.get_cell_xy(cx, cy) else { return false };
                let Some(cell) = self.get_cell(cid) else { return false };
                if !cell.walkable {
                    return false;
                }
                if occupied.contains(&cid) {
                    if cid == start {
                        continue;
                    }
                    if cid == end {
                        return true;
                    }
                    if !ignored.contains(&cid) {
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn set_obstacle(&mut self, id: i32, obstacle_id: i32, size: i32) {
        if let Some(c) = self.get_cell_mut(id) {
            c.walkable = false;
            c.obstacle_id = obstacle_id;
            c.obstacle_size = size;
        }
    }

    pub fn clear_obstacles(&mut self) {
        for c in &mut self.cells {
            c.walkable = true;
            c.obstacle_id = 0;
            c.obstacle_size = 0;
        }
    }

    #[must_use]
    pub fn is_obstacle(&self, id: i32) -> bool {
        self.get_cell(id).map(|c| !c.walkable).unwrap_or(true)
    }

    #[must_use]
    pub fn area_cells(&self, area: i64, target: i32) -> Vec<i32> {
        match area {
            1 => vec![target],
            3 => {
                let mut v = vec![target];
                v.extend(self.neighbors4(target));
                v
            }
            _ => vec![target],
        }
    }

    #[must_use]
    pub fn compute_components(&self) -> Vec<i32> {
        let mut comp = vec![-1; self.nb_cells as usize];
        let mut next_id: i32 = 1;
        for id in 0..self.nb_cells {
            let idx = id as usize;
            if comp[idx] != -1 {
                continue;
            }
            let Some(c0) = self.get_cell(id) else { continue };
            let walkable = c0.walkable;
            let mut q = VecDeque::new();
            q.push_back(id);
            comp[idx] = next_id;
            while let Some(cur) = q.pop_front() {
                for n in self.neighbors4(cur) {
                    let nidx = n as usize;
                    if comp[nidx] != -1 {
                        continue;
                    }
                    let Some(nc) = self.get_cell(n) else { continue };
                    if nc.walkable != walkable {
                        continue;
                    }
                    comp[nidx] = next_id;
                    q.push_back(n);
                }
            }
            next_id += 1;
        }
        comp
    }

    #[must_use]
    pub fn random_cell(&self, rng: &mut Rng, occupied: &HashSet<i32>) -> Option<i32> {
        let mut out: Option<i32> = None;
        let mut nb = 0;
        loop {
            let ok = match out.and_then(|id| self.get_cell(id)) {
                Some(c) => c.available(occupied),
                None => false,
            };
            if ok {
                break;
            }
            out = Some(rng.int_inclusive(0, self.nb_cells));
            if nb > 64 {
                break;
            }
            nb += 1;
        }
        out.filter(|id| self.get_cell(*id).map(|c| c.available(occupied)).unwrap_or(false))
    }

    #[must_use]
    pub fn random_cell_part(&self, rng: &mut Rng, part: i32, occupied: &HashSet<i32>) -> Option<i32> {
        let mut out: Option<i32> = None;
        let mut nb = 0;
        loop {
            let ok = match out.and_then(|id| self.get_cell(id)) {
                Some(c) => c.available(occupied),
                None => false,
            };
            if ok {
                break;
            }
            let y = rng.int_inclusive(0, self.height - 1);
            let x = rng.int_inclusive(0, self.width / 4);
            let mut cellid = y * (self.width * 2 - 1);
            cellid += (part - 1) * self.width / 4 + x;
            out = Some(cellid);
            if nb > 64 {
                break;
            }
            nb += 1;
        }
        out.filter(|id| self.get_cell(*id).map(|c| c.available(occupied)).unwrap_or(false))
    }

    #[must_use]
    pub fn a_star_path(
        &self,
        start: i32,
        end_cells: &[i32],
        occupied: &HashSet<i32>,
        cells_to_ignore: Option<&HashSet<i32>>,
    ) -> Option<Vec<i32>> {
        if end_cells.is_empty() || end_cells.contains(&start) {
            return None;
        }
        let h_goal = *end_cells.first().unwrap();

        let n = self.nb_cells as usize;
        let mut visited = vec![false; n];
        let mut closed = vec![false; n];
        let mut cost: Vec<i32> = vec![i32::MAX; n];
        let mut weight: Vec<f32> = vec![0.0; n];
        let mut parent: Vec<Option<i32>> = vec![None; n];

        cost[start as usize] = 0;
        weight[start as usize] = 0.0;
        visited[start as usize] = true;
        let mut open: Vec<i32> = vec![start];

        while !open.is_empty() {
            let mut best_idx = 0usize;
            let mut best_w = weight[open[0] as usize];
            for (i, &cid) in open.iter().enumerate().skip(1) {
                let w = weight[cid as usize];
                // Tie-break like Java's TreeSet comparator (which never returns 0):
                // for equal weights, the most recently inserted node tends to win.
                if w < best_w || (w == best_w && i > best_idx) {
                    best_w = w;
                    best_idx = i;
                }
            }
            let u = open.swap_remove(best_idx);
            closed[u as usize] = true;

            if end_cells.contains(&u) {
                let mut result: Vec<i32> = Vec::new();
                let mut cur = u;
                let mut s = cost[u as usize];
                while s >= 1 {
                    result.push(cur);
                    if let Some(p) = parent[cur as usize] {
                        cur = p;
                    } else {
                        break;
                    }
                    s -= 1;
                }
                result.reverse();
                if let Some(&last) = result.last() {
                    let ignored = cells_to_ignore.map(|s| s.contains(&last)).unwrap_or(false);
                    if occupied.contains(&last) && !ignored {
                        result.pop();
                    }
                }
                return Some(result);
            }

            for c in self.neighbors4(u) {
                let idx = c as usize;
                if closed[idx] {
                    continue;
                }
                let cc = self.get_cell(c)?;
                if !cc.walkable {
                    continue;
                }
                let ignored = cells_to_ignore.map(|s| s.contains(&c)).unwrap_or(false);
                if occupied.contains(&c) && !ignored && !end_cells.contains(&c) {
                    continue;
                }
                let new_cost = cost[u as usize].saturating_add(1);
                if !visited[idx] || new_cost < cost[idx] {
                    cost[idx] = new_cost;
                    let ca = self.get_cell(c)?;
                    let cb = self.get_cell(h_goal)?;
                    let dx = (ca.x - cb.x) as f32;
                    let dy = (ca.y - cb.y) as f32;
                    let h = (dx * dx + dy * dy).sqrt(); // Java: sqrt(distance2)
                    weight[idx] = (new_cost as f32) + h;
                    parent[idx] = Some(u);
                    if !visited[idx] {
                        open.push(c);
                        visited[idx] = true;
                    }
                }
            }
        }
        None
    }

    /// Random world generation + spawn placement with connectivity retries.
    pub fn generate_random(
        mut rng: Rng,
        context: i32,
        width: i32,
        height: i32,
        obstacle_count: i32,
        team_sizes: &[usize],
    ) -> (WorldMap, Vec<i32>, Rng) {
        let mut valid = false;
        let mut nb = 0;
        let mut map = WorldMap::new(width, height);
        let mut placements: Vec<i32> = Vec::new();

        while !valid && nb < 63 {
            nb += 1;
            map = WorldMap::new(width, height);
            placements.clear();
            let occupied: HashSet<i32> = HashSet::new();

            for _ in 0..obstacle_count {
                // Same out-of-range behavior as above.
                let cid = rng.int_inclusive(0, map.nb_cells);
                let Some(c) = map.get_cell(cid) else { continue };
                if !c.available(&occupied) {
                    continue;
                }
                let mut size = rng.int_inclusive(1, 2);
                let typ = rng.int_inclusive(0, 2);
                if size == 2 {
                    let c2 = map.get_cell_by_dir(cid, Dir::East);
                    let c3 = map.get_cell_by_dir(cid, Dir::South);
                    let c4 = c3.and_then(|cc3| map.get_cell_by_dir(cc3, Dir::East));
                    let ok = c2
                        .and_then(|x| map.get_cell(x))
                        .map(|x| x.available(&occupied))
                        .unwrap_or(false)
                        && c3
                            .and_then(|x| map.get_cell(x))
                            .map(|x| x.available(&occupied))
                            .unwrap_or(false)
                        && c4
                            .and_then(|x| map.get_cell(x))
                            .map(|x| x.available(&occupied))
                            .unwrap_or(false);
                    if !ok {
                        size = 1;
                    } else {
                        if let Some(id2) = c2 {
                            map.set_obstacle(id2, 0, -1);
                        }
                        if let Some(id3) = c3 {
                            map.set_obstacle(id3, 0, -2);
                        }
                        if let Some(id4) = c4 {
                            map.set_obstacle(id4, 0, -3);
                        }
                    }
                }
                map.set_obstacle(cid, typ, size);
            }

            // Place entities (classic: two sides if exactly 2 teams, else random).
            let mut occ: HashSet<i32> = HashSet::new();
            for (t, &sz) in team_sizes.iter().enumerate() {
                for _ in 0..sz {
                    let cell = if team_sizes.len() == 2 {
                        map.random_cell_part(&mut rng, if t == 0 { 1 } else { 4 }, &occ)
                    } else {
                        map.random_cell(&mut rng, &occ)
                    };
                    let Some(cell) = cell else { continue };
                    occ.insert(cell);
                    placements.push(cell);
                }
            }

            // Connectivity check: all placements must be in same walkable component.
            valid = true;
            if !placements.is_empty() {
                let comps = map.compute_components();
                let first = comps[placements[0] as usize];
                for &p in &placements[1..] {
                    if comps[p as usize] != first {
                        valid = false;
                        break;
                    }
                }
            }
        }

        // Generate type like the reference implementation, then override based on context.
        map.map_type = rng.int_inclusive(0, 4);
        if context == 0 {
            map.map_type = -1;
        } else if context == 3 {
            map.map_type = 5;
        }

        (map, placements, rng)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coords_and_neighbors_smoke() {
        let m = WorldMap::new(18, 18);
        assert_eq!(m.nb_cells, (18 * 2 - 1) * 18 - (18 - 1));
        let id = 100;
        let c = m.get_cell(id).unwrap();
        if c.north {
            assert_eq!(m.get_cell_by_dir(id, Dir::North), Some(id - 18 + 1));
        }
        if c.west {
            assert_eq!(m.get_cell_by_dir(id, Dir::West), Some(id - 18));
        }
        if c.east {
            assert_eq!(m.get_cell_by_dir(id, Dir::East), Some(id + 18));
        }
        if c.south {
            assert_eq!(m.get_cell_by_dir(id, Dir::South), Some(id + 18 - 1));
        }
    }

    #[test]
    fn los_no_obstacles_is_true() {
        let m = WorldMap::new(18, 18);
        let occupied = HashSet::new();
        let ignored = HashSet::new();
        assert!(m.verify_los(0, 100, true, &occupied, &ignored));
    }

    #[test]
    fn astar_basic_path_exists() {
        let m = WorldMap::new(18, 18);
        let occupied = HashSet::new();
        let path = m.a_star_path(0, &[100], &occupied, None);
        assert!(path.is_some());
    }

    #[test]
    fn mapgen_smoke() {
        let rng = Rng::new(1234567);
        let (m, placements, _rng2) = WorldMap::generate_random(rng, 0, 18, 18, 50, &[2, 2]);
        assert_eq!(m.width, 18);
        assert_eq!(placements.len(), 4);
    }
}

