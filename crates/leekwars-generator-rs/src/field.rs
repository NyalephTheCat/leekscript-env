use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct Field {
    pub tiles_x: i32,
    id_to_xy: HashMap<i64, (i32, i32)>,
    xy_to_id: HashMap<(i32, i32), i64>,
    obstacles: HashSet<i64>,
}

impl Field {
    #[must_use]
    pub fn new(tiles_x: i32, obstacles: impl IntoIterator<Item = i64>) -> Self {
        let mut id_to_xy = HashMap::new();
        let mut xy_to_id = HashMap::new();
        let tx = tiles_x;
        let base: i64 = (tx * (tx - 1)) as i64;

        for x in (-tx + 1)..tx {
            for y in (-tx + 1)..tx {
                if x.abs() + y.abs() >= tx {
                    continue;
                }
                let id = base + (tx as i64) * (x as i64) + ((tx - 1) as i64) * (y as i64);
                id_to_xy.insert(id, (x, y));
                xy_to_id.insert((x, y), id);
            }
        }

        let obstacles: HashSet<i64> = obstacles.into_iter().collect();
        Self {
            tiles_x,
            id_to_xy,
            xy_to_id,
            obstacles,
        }
    }

    #[must_use]
    pub fn cell_exists(&self, id: i64) -> bool {
        self.id_to_xy.contains_key(&id)
    }

    #[must_use]
    pub fn is_obstacle(&self, id: i64) -> bool {
        self.obstacles.contains(&id)
    }

    #[must_use]
    pub fn neighbors4(&self, id: i64) -> Vec<i64> {
        let Some(&(x, y)) = self.id_to_xy.get(&id) else {
            return vec![];
        };
        const DIRS: &[(i32, i32)] = &[(1, 0), (0, 1), (-1, 0), (0, -1)];
        let mut out = Vec::with_capacity(4);
        for (dx, dy) in DIRS {
            if let Some(&nid) = self.xy_to_id.get(&(x + dx, y + dy)) {
                out.push(nid);
            }
        }
        out
    }

    #[must_use]
    pub fn cell_xy(&self, id: i64) -> Option<(i32, i32)> {
        self.id_to_xy.get(&id).copied()
    }

    #[must_use]
    pub fn xy_cell(&self, x: i32, y: i32) -> Option<i64> {
        self.xy_to_id.get(&(x, y)).copied()
    }

    #[must_use]
    pub fn all_cells(&self) -> Vec<i64> {
        self.id_to_xy.keys().copied().collect()
    }

    /// Shortest path over 4-neighbor grid. Returned path includes `start` and `goal`.
    #[must_use]
    pub fn shortest_path(
        &self,
        start: i64,
        goal: i64,
        blocked: &HashSet<i64>,
    ) -> Option<Vec<i64>> {
        if start == goal {
            return Some(vec![start]);
        }
        if !self.cell_exists(start) || !self.cell_exists(goal) {
            return None;
        }
        if self.is_obstacle(goal) {
            // Goal is not walkable; no path.
            return None;
        }

        let mut q = VecDeque::new();
        let mut prev: HashMap<i64, i64> = HashMap::new();
        let mut seen: HashSet<i64> = HashSet::new();
        q.push_back(start);
        seen.insert(start);

        while let Some(cur) = q.pop_front() {
            for n in self.neighbors4(cur) {
                if seen.contains(&n) {
                    continue;
                }
                if self.is_obstacle(n) {
                    continue;
                }
                if blocked.contains(&n) && n != goal {
                    continue;
                }
                prev.insert(n, cur);
                if n == goal {
                    // Reconstruct.
                    let mut path = vec![goal];
                    let mut p = goal;
                    while let Some(&pp) = prev.get(&p) {
                        path.push(pp);
                        if pp == start {
                            break;
                        }
                        p = pp;
                    }
                    path.reverse();
                    return Some(path);
                }
                seen.insert(n);
                q.push_back(n);
            }
        }

        None
    }

    /// Shortest path to any goal in `goals`. Returned path includes `start` and chosen goal.
    #[must_use]
    pub fn shortest_path_to_any(
        &self,
        start: i64,
        goals: &HashSet<i64>,
        blocked: &HashSet<i64>,
    ) -> Option<Vec<i64>> {
        if goals.contains(&start) {
            return Some(vec![start]);
        }
        if !self.cell_exists(start) {
            return None;
        }

        let mut q = VecDeque::new();
        let mut prev: HashMap<i64, i64> = HashMap::new();
        let mut seen: HashSet<i64> = HashSet::new();
        q.push_back(start);
        seen.insert(start);

        while let Some(cur) = q.pop_front() {
            for n in self.neighbors4(cur) {
                if seen.contains(&n) {
                    continue;
                }
                if self.is_obstacle(n) {
                    continue;
                }
                if blocked.contains(&n) {
                    continue;
                }
                prev.insert(n, cur);
                if goals.contains(&n) {
                    let mut path = vec![n];
                    let mut p = n;
                    while let Some(&pp) = prev.get(&p) {
                        path.push(pp);
                        if pp == start {
                            break;
                        }
                        p = pp;
                    }
                    path.reverse();
                    return Some(path);
                }
                seen.insert(n);
                q.push_back(n);
            }
        }
        None
    }

    #[must_use]
    pub fn path_distance(&self, start: i64, goal: i64, blocked: &HashSet<i64>) -> Option<i64> {
        let p = self.shortest_path(start, goal, blocked)?;
        Some(p.len().saturating_sub(1) as i64)
    }

    #[must_use]
    pub fn cell_distance(&self, a: i64, b: i64) -> Option<i64> {
        let (ax, ay) = self.cell_xy(a)?;
        let (bx, by) = self.cell_xy(b)?;
        // Hex-ish grid distance (axial-like): max(|dx|, |dy|, |dx+dy|)
        let dx = (ax - bx).abs() as i64;
        let dy = (ay - by).abs() as i64;
        let dz = ((ax + ay) - (bx + by)).abs() as i64;
        Some(dx.max(dy).max(dz))
    }

    /// Cells strictly between `a` and `b` if they are aligned on a straight axis.
    ///
    /// Uses Field axial-ish coordinates (x,y). A line exists if x matches, y matches,
    /// or (x+y) matches.
    #[must_use]
    pub fn line_between_exclusive(&self, a: i64, b: i64) -> Option<Vec<i64>> {
        let (ax, ay) = self.cell_xy(a)?;
        let (bx, by) = self.cell_xy(b)?;
        if ax == bx {
            let step = (by - ay).signum();
            let mut out = Vec::new();
            let mut y = ay + step;
            while y != by {
                out.push(self.xy_cell(ax, y)?);
                y += step;
            }
            return Some(out);
        }
        if ay == by {
            let step = (bx - ax).signum();
            let mut out = Vec::new();
            let mut x = ax + step;
            while x != bx {
                out.push(self.xy_cell(x, ay)?);
                x += step;
            }
            return Some(out);
        }
        if ax + ay == bx + by {
            let stepx = (bx - ax).signum();
            let stepy = (by - ay).signum();
            let mut out = Vec::new();
            let mut x = ax + stepx;
            let mut y = ay + stepy;
            while x != bx || y != by {
                out.push(self.xy_cell(x, y)?);
                x += stepx;
                y += stepy;
            }
            // we included b; remove it
            out.pop();
            return Some(out);
        }
        None
    }

    #[must_use]
    pub fn area_cells(&self, area: i64, target: i64) -> Vec<i64> {
        // Mirrors Java Area ids loosely; implement only the common ones for now.
        match area {
            1 => vec![target], // single cell
            3 => {
                // circle1 / plus1
                let mut v = vec![target];
                v.extend(self.neighbors4(target));
                v
            }
            _ => vec![target],
        }
    }
}

