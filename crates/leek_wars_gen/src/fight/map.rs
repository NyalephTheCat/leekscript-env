//! Cell indexing matching `com.leekwars.generator.maps.Cell` (constructor math).

/// Grid coordinates used by `Map.getDistance2(Cell, Cell)`.
#[inline]
#[must_use]
pub fn cell_xy(map_width: i32, cell_id: i32) -> (i32, i32) {
    let w = map_width;
    let x_raw = cell_id % (w * 2 - 1);
    let y_raw = cell_id / (w * 2 - 1);
    let y = y_raw - x_raw % w;
    let x = (cell_id - (w - 1) * y) / w;
    (x, y)
}

/// Inverse of [`cell_xy`] (`id = w * x + (w - 1) * y`).
#[inline]
#[must_use]
pub fn cell_id_from_xy(map_width: i32, x: i32, y: i32) -> i32 {
    map_width * x + (map_width - 1) * y
}

/// Squared distance between two cells (Java `Map.getDistance2`).
#[inline]
#[must_use]
pub fn distance2(map_width: i32, a: i32, b: i32) -> i32 {
    let (x1, y1) = cell_xy(map_width, a);
    let (x2, y2) = cell_xy(map_width, b);
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}

/// Cell count for a rhombus map (`Map` constructor in the Java generator).
#[inline]
#[must_use]
pub fn nb_cells(map_width: i32, map_height: i32) -> i32 {
    (map_width * 2 - 1) * map_height - (map_width - 1)
}

#[inline]
#[must_use]
pub fn is_valid_cell(map_width: i32, map_height: i32, cell_id: i32) -> bool {
    cell_id >= 0 && cell_id < nb_cells(map_width, map_height)
}

/// Manhattan distance on internal `(x, y)` (`Pathfinding.getCaseDistance`).
#[inline]
#[must_use]
pub fn case_distance(map_width: i32, a: i32, b: i32) -> i32 {
    let (x1, y1) = cell_xy(map_width, a);
    let (x2, y2) = cell_xy(map_width, b);
    (x1 - x2).abs() + (y1 - y2).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference: `/tmp/CellXY.java` with `w = 17`.
    #[test]
    fn cell_xy_matches_java() {
        let w = 17;
        assert_eq!(cell_xy(w, 123), (11, -4));
        assert_eq!(cell_xy(w, 566), (22, 12));
        assert_eq!(cell_xy(w, 301), (13, 5));
    }

    #[test]
    fn distance2_sample() {
        let w = 17;
        let d = distance2(w, 123, 566);
        assert_eq!(d, (11 - 22) * (11 - 22) + (-4 - 12) * (-4 - 12));
    }

    #[test]
    fn cell_id_roundtrip() {
        let w = 17;
        for id in [123, 301, 566, 0, 100] {
            let (x, y) = cell_xy(w, id);
            assert_eq!(cell_id_from_xy(w, x, y), id);
        }
    }

    #[test]
    fn nb_cells_matches_java_constructor() {
        assert_eq!(nb_cells(18, 18), 613);
        assert!(is_valid_cell(18, 18, 566));
        assert!(!is_valid_cell(18, 18, 613));
    }
}
