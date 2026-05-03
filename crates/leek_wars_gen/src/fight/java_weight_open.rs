//! OpenJDK-style `TreeMap` used as the A* open set in Leek Wars Java pathfinding.
//!
//! The game comparator is effectively `(w_a > w_b) ? 1 : -1` — it never returns `Equal`, so
//! equal-weight keys are ordered by RB-tree shape (insert path), not by a total order.
//! We mirror that by comparing **current** `weight(cell)` values at insertion time only; the tree
//! is not rebalanced when weights later change (matching mutable `Cell.weight` in Java).

#[derive(Clone, Copy)]
pub(crate) struct OpenKey {
    pub cell: i32,
    #[allow(dead_code)]
    pub seq: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Black,
}

struct Node {
    k: OpenKey,
    parent: Option<usize>,
    l: Option<usize>,
    r: Option<usize>,
    color: Color,
}

pub(crate) struct JavaWeightTree {
    n: Vec<Node>,
    root: Option<usize>,
}

impl JavaWeightTree {
    pub fn new() -> Self {
        Self {
            n: Vec::new(),
            root: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    fn color_of(&self, x: Option<usize>) -> Color {
        x.map_or(Color::Black, |i| self.n[i].color)
    }

    fn set_color(&mut self, x: Option<usize>, c: Color) {
        if let Some(i) = x {
            self.n[i].color = c;
        }
    }

    fn parent_of(&self, x: Option<usize>) -> Option<usize> {
        x.and_then(|i| self.n[i].parent)
    }

    fn left_of(&self, x: Option<usize>) -> Option<usize> {
        x.and_then(|i| self.n[i].l)
    }

    fn right_of(&self, x: Option<usize>) -> Option<usize> {
        x.and_then(|i| self.n[i].r)
    }

    fn rotate_left(&mut self, p: usize) {
        let r = self.n[p].r.expect("rotate_left requires right child");
        let rl = self.n[r].l;
        self.n[p].r = rl;
        if let Some(rl) = rl {
            self.n[rl].parent = Some(p);
        }
        let pp = self.n[p].parent;
        self.n[r].parent = pp;
        match pp {
            None => self.root = Some(r),
            Some(ppi) => {
                if self.n[ppi].l == Some(p) {
                    self.n[ppi].l = Some(r);
                } else {
                    self.n[ppi].r = Some(r);
                }
            }
        }
        self.n[r].l = Some(p);
        self.n[p].parent = Some(r);
    }

    fn rotate_right(&mut self, p: usize) {
        let l = self.n[p].l.expect("rotate_right requires left child");
        let lr = self.n[l].r;
        self.n[p].l = lr;
        if let Some(lr) = lr {
            self.n[lr].parent = Some(p);
        }
        let pp = self.n[p].parent;
        self.n[l].parent = pp;
        match pp {
            None => self.root = Some(l),
            Some(ppi) => {
                if self.n[ppi].r == Some(p) {
                    self.n[ppi].r = Some(l);
                } else {
                    self.n[ppi].l = Some(l);
                }
            }
        }
        self.n[l].r = Some(p);
        self.n[p].parent = Some(l);
    }

    /// `java.util.TreeMap.fixAfterInsertion` — grandparent may be absent (`null`); never unwrap it.
    fn fix_after_insertion(&mut self, mut x: usize) {
        self.n[x].color = Color::Red;
        while let Some(root_idx) = self.root {
            if x == root_idx {
                break;
            }
            let Some(xp) = self.parent_of(Some(x)) else {
                break;
            };
            if self.n[xp].color != Color::Red {
                break;
            }
            let xpp = self.n[xp].parent;
            // `parentOf(x) == leftOf(parentOf(parentOf(x)))`
            let xp_is_left = match xpp {
                Some(gp) => self.n[gp].l == Some(xp),
                None => false,
            };
            if xp_is_left {
                let y = xpp.and_then(|gp| self.n[gp].r);
                if self.color_of(y) == Color::Red {
                    self.n[xp].color = Color::Black;
                    self.set_color(y, Color::Black);
                    self.set_color(xpp, Color::Red);
                    match xpp {
                        Some(gp) => x = gp,
                        None => break,
                    }
                } else {
                    if self.n[xp].r == Some(x) {
                        x = xp;
                        self.rotate_left(x);
                    }
                    let xp2 = self
                        .parent_of(Some(x))
                        .expect("non-root after rotate in fixAfterInsertion");
                    let xpp2 = self.n[xp2].parent;
                    self.n[xp2].color = Color::Black;
                    self.set_color(xpp2, Color::Red);
                    if let Some(g) = xpp2 {
                        self.rotate_right(g);
                    }
                }
            } else {
                let y = xpp.and_then(|gp| self.n[gp].l);
                if self.color_of(y) == Color::Red {
                    self.n[xp].color = Color::Black;
                    self.set_color(y, Color::Black);
                    self.set_color(xpp, Color::Red);
                    match xpp {
                        Some(gp) => x = gp,
                        None => break,
                    }
                } else {
                    if self.n[xp].l == Some(x) {
                        x = xp;
                        self.rotate_right(x);
                    }
                    let xp2 = self
                        .parent_of(Some(x))
                        .expect("non-root after rotate in fixAfterInsertion");
                    let xpp2 = self.n[xp2].parent;
                    self.n[xp2].color = Color::Black;
                    self.set_color(xpp2, Color::Red);
                    if let Some(g) = xpp2 {
                        self.rotate_left(g);
                    }
                }
            }
        }
        if let Some(r) = self.root {
            self.n[r].color = Color::Black;
        }
    }

    fn successor(&self, mut t: usize) -> Option<usize> {
        if let Some(r) = self.n[t].r {
            t = r;
            while let Some(l) = self.n[t].l {
                t = l;
            }
            return Some(t);
        }
        let mut p = self.n[t].parent;
        let mut ch = Some(t);
        while let Some(pp) = p {
            if self.n[pp].l == ch {
                return Some(pp);
            }
            ch = Some(pp);
            p = self.n[pp].parent;
        }
        None
    }

    fn fix_after_deletion(&mut self, mut x: Option<usize>) {
        while x != self.root && self.color_of(x) == Color::Black {
            let xp = self.parent_of(x);
            let Some(xp) = xp else {
                break;
            };
            if x == self.n[xp].l {
                let mut sib = self.n[xp].r;
                if self.color_of(sib) == Color::Red {
                    self.set_color(sib, Color::Black);
                    self.n[xp].color = Color::Red;
                    self.rotate_left(xp);
                    sib = self.n[xp].r;
                }
                if self.color_of(self.left_of(sib)) == Color::Black
                    && self.color_of(self.right_of(sib)) == Color::Black
                {
                    self.set_color(sib, Color::Red);
                    x = Some(xp);
                } else {
                    if self.color_of(self.right_of(sib)) == Color::Black {
                        self.set_color(self.left_of(sib), Color::Black);
                        self.set_color(sib, Color::Red);
                        if let Some(s) = sib {
                            self.rotate_right(s);
                        }
                        sib = self.n[xp].r;
                    }
                    self.set_color(sib, self.n[xp].color);
                    self.n[xp].color = Color::Black;
                    self.set_color(self.right_of(sib), Color::Black);
                    self.rotate_left(xp);
                    x = self.root;
                }
            } else {
                let mut sib = self.n[xp].l;
                if self.color_of(sib) == Color::Red {
                    self.set_color(sib, Color::Black);
                    self.n[xp].color = Color::Red;
                    self.rotate_right(xp);
                    sib = self.n[xp].l;
                }
                if self.color_of(self.right_of(sib)) == Color::Black
                    && self.color_of(self.left_of(sib)) == Color::Black
                {
                    self.set_color(sib, Color::Red);
                    x = Some(xp);
                } else {
                    if self.color_of(self.left_of(sib)) == Color::Black {
                        self.set_color(self.right_of(sib), Color::Black);
                        self.set_color(sib, Color::Red);
                        if let Some(s) = sib {
                            self.rotate_left(s);
                        }
                        sib = self.n[xp].l;
                    }
                    self.set_color(sib, self.n[xp].color);
                    self.n[xp].color = Color::Black;
                    self.set_color(self.left_of(sib), Color::Black);
                    self.rotate_right(xp);
                    x = self.root;
                }
            }
        }
        self.set_color(x, Color::Black);
    }

    fn delete_entry(&mut self, p: usize) {
        let mut p = p;
        if self.n[p].l.is_some() && self.n[p].r.is_some() {
            let s = self.successor(p).unwrap();
            self.n[p].k = self.n[s].k;
            p = s;
        }
        let replacement = if self.n[p].l.is_some() {
            self.n[p].l
        } else {
            self.n[p].r
        };
        if let Some(r) = replacement {
            let p_parent = self.n[p].parent;
            self.n[r].parent = p_parent;
            match p_parent {
                None => self.root = Some(r),
                Some(pp) if self.n[pp].l == Some(p) => self.n[pp].l = Some(r),
                Some(pp) => self.n[pp].r = Some(r),
            }
            self.n[p].l = None;
            self.n[p].r = None;
            self.n[p].parent = None;
            if self.n[p].color == Color::Black {
                self.fix_after_deletion(Some(r));
            }
        } else if self.n[p].parent.is_none() {
            self.root = None;
        } else {
            if self.n[p].color == Color::Black {
                self.fix_after_deletion(Some(p));
            }
            let pp = self.n[p].parent.unwrap();
            if self.n[pp].l == Some(p) {
                self.n[pp].l = None;
            } else if self.n[pp].r == Some(p) {
                self.n[pp].r = None;
            }
            self.n[p].parent = None;
        }
    }

    /// Insert using Java `Map$2`-style compare: `(w(new) > w(old)) ? go_right : go_left`.
    pub fn insert(&mut self, k: OpenKey, w: &impl Fn(i32) -> f32) {
        let mut parent: Option<usize> = None;
        let mut t = self.root;
        let mut cmp_right = false;
        while let Some(cur) = t {
            parent = Some(cur);
            cmp_right = w(k.cell) > w(self.n[cur].k.cell);
            t = if cmp_right {
                self.n[cur].r
            } else {
                self.n[cur].l
            };
        }
        let e = self.n.len();
        self.n.push(Node {
            k,
            parent,
            l: None,
            r: None,
            color: Color::Red,
        });
        if let Some(p) = parent {
            if cmp_right {
                self.n[p].r = Some(e);
            } else {
                self.n[p].l = Some(e);
            }
        } else {
            self.root = Some(e);
        }
        self.fix_after_insertion(e);
    }

    /// `TreeSet.pollFirst`: remove and return the leftmost entry.
    pub fn poll_first(&mut self) -> Option<OpenKey> {
        let mut p = self.root?;
        while let Some(l) = self.n[p].l {
            p = l;
        }
        let out = self.n[p].k;
        self.delete_entry(p);
        Some(out)
    }
}

/// Replay a `TreeSetWeightProbe.java` script using this crate's [`JavaWeightTree`]. Each line is one
/// `pollFirst` result (`"null"` if empty).
#[doc(hidden)]
#[must_use]
pub fn replay_treeset_weight_probe_polls(script: &str) -> Vec<String> {
    use std::collections::HashMap;

    let mut weights: HashMap<i32, f32> = HashMap::new();
    let mut tree = JavaWeightTree::new();
    let mut seq: u32 = 0;
    let mut lines = Vec::new();
    for raw in script.lines() {
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        let mut it = raw.split_whitespace();
        let Some(op) = it.next() else {
            continue;
        };
        if op == "u" {
            let id: i32 = it.next().unwrap().parse().unwrap();
            let bits: u32 = it.next().unwrap().parse::<i32>().unwrap() as u32;
            weights.insert(id, f32::from_bits(bits));
        } else if op == "i" {
            let id: i32 = it.next().unwrap().parse().unwrap();
            let w = |c: i32| *weights.get(&c).expect("weight set with u before i");
            tree.insert(OpenKey { cell: id, seq }, &w);
            seq = seq.wrapping_add(1);
        } else if op == "p" {
            let s = if tree.is_empty() {
                "null".to_string()
            } else {
                tree.poll_first().unwrap().cell.to_string()
            };
            lines.push(s);
        } else {
            panic!("unknown op {op:?} in line {raw:?}");
        }
    }
    lines
}

/// Output directory for compiled `TreeSetWeightProbe` (shared with unit crosscheck tests).
#[doc(hidden)]
pub fn treeset_weight_probe_classes_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/tree_set_weight_probe_classes")
}

/// Compile `leek-wars-generator/tools/TreeSetWeightProbe.java`; returns the classpath directory.
#[doc(hidden)]
pub fn compile_treeset_weight_probe_java() -> Result<std::path::PathBuf, String> {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../leek-wars-generator/tools/TreeSetWeightProbe.java");
    if !src.is_file() {
        return Err(format!(
            "TreeSetWeightProbe.java not found at {}",
            src.display()
        ));
    }
    let out = treeset_weight_probe_classes_dir();
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let ok = Command::new("javac")
        .arg("-d")
        .arg(&out)
        .arg(&src)
        .status()
        .map_err(|e| format!("javac: {e}"))?;
    if !ok.success() {
        return Err("javac failed (is a JDK on PATH?)".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Matches `tools/treeset_probe` / `OpenJDK` `TreeSet` with `Comparator` `(a > b) ? 1 : -1`.
    #[test]
    fn equal_weights_poll_order_lifo() {
        let mut weights: HashMap<i32, f32> = HashMap::new();
        let mut t = JavaWeightTree::new();
        for i in 1..=4 {
            weights.insert(i, 10.0);
            let w = |c: i32| *weights.get(&c).unwrap();
            t.insert(
                OpenKey {
                    cell: i,
                    seq: i as u32,
                },
                &w,
            );
        }
        let mut out = Vec::new();
        while !t.is_empty() {
            out.push(t.poll_first().unwrap().cell);
        }
        assert_eq!(out, vec![4, 3, 2, 1]);
    }

    /// `OpenJDK` `TreeSet` with `(a.w > b.w) ? 1 : -1`, insert order 1,3,2,4 — see `/tmp/T.java`.
    #[test]
    fn equal_weights_insert_order_1324_matches_java() {
        let mut weights: HashMap<i32, f32> = HashMap::new();
        let mut t = JavaWeightTree::new();
        for i in [1, 3, 2, 4] {
            weights.insert(i, 1.0);
            let w = |c: i32| *weights.get(&c).unwrap();
            t.insert(
                OpenKey {
                    cell: i,
                    seq: i as u32,
                },
                &w,
            );
        }
        let mut out = Vec::new();
        while !t.is_empty() {
            out.push(t.poll_first().unwrap().cell);
        }
        assert_eq!(out, vec![4, 2, 3, 1]);
    }

    #[test]
    fn lower_weight_polls_first() {
        let mut weights: HashMap<i32, f32> = [(1, 5.0), (2, 3.0), (3, 4.0)].into_iter().collect();
        let mut t = JavaWeightTree::new();
        for i in 1..=3 {
            let w = |c: i32| *weights.get(&c).unwrap();
            t.insert(
                OpenKey {
                    cell: i,
                    seq: i as u32,
                },
                &w,
            );
        }
        assert_eq!(t.poll_first().unwrap().cell, 2);
        weights.insert(2, 100.0);
        // tree shape unchanged; next leftmost by structure, not by updated weight
        let _ = weights;
        let k = t.poll_first().unwrap().cell;
        assert!(k == 1 || k == 3);
    }
}

/// Random insert / `pollFirst` streams vs `OpenJDK` `TreeSet` (`tools/TreeSetWeightProbe.java`).
#[cfg(all(test, unix))]
mod java_treeset_crosscheck {
    use super::{compile_treeset_weight_probe_java, replay_treeset_weight_probe_polls};
    use std::io::Write;
    use std::path::Path;
    use std::process::{Command, Stdio};

    fn java_poll_lines(script: &str, cp: &Path) -> Result<Vec<String>, String> {
        let mut child = Command::new("java")
            .arg("-cp")
            .arg(cp)
            .arg("TreeSetWeightProbe")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("java: {e}"))?;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(script.as_bytes())
            .map_err(|e| format!("stdin: {e}"))?;
        let out = child.wait_with_output().map_err(|e| format!("wait: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "java stderr: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    struct Lcg(u64);
    impl Lcg {
        fn new(seed: u64) -> Self {
            Self(seed)
        }
        fn next_u64(&mut self) -> u64 {
            // Numerical Recipes linear congruential generator (64-bit style).
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn next_finite_f32_bits(&mut self) -> u32 {
            loop {
                let b = (self.next_u64() >> 32) as u32;
                let f = f32::from_bits(b);
                if f.is_finite() {
                    return b;
                }
            }
        }
    }

    /// Random stream with **mutable** weights (`u`) like Java `Cell.weight` + `TreeSet` comparator.
    fn random_mutable_script(seed: u64, ops: usize) -> String {
        let mut rng = Lcg::new(seed);
        let mut next_id: i32 = 0;
        let mut s = String::new();
        for _ in 0..ops {
            let r = (rng.next_u64() % 100) as u8;
            if r < 45 && next_id > 0 {
                let id = (rng.next_u64() % next_id as u64) as i32;
                let bits = rng.next_finite_f32_bits();
                s.push_str(&format!("u {id} {}\n", bits as i32));
            } else if r < 78 && next_id < 50_000 {
                let bits = rng.next_finite_f32_bits();
                s.push_str(&format!("u {next_id} {}\n", bits as i32));
                s.push_str(&format!("i {next_id}\n"));
                next_id += 1;
            } else {
                s.push_str("p\n");
            }
        }
        s
    }

    #[test]
    fn handcrafted_equal_weight_chain_matches_java_treeset() {
        let Ok(cp) = compile_treeset_weight_probe_java() else {
            return;
        };
        let ten = 10.0f32.to_bits() as i32;
        let script = format!(
            "u 1 {ten}\ni 1\nu 2 {ten}\ni 2\nu 3 {ten}\ni 3\nu 4 {ten}\ni 4\n\
             p\np\np\np\n"
        );
        let j = java_poll_lines(&script, &cp).expect("java");
        let r = replay_treeset_weight_probe_polls(&script);
        assert_eq!(j, r);
        assert_eq!(r, vec!["4", "3", "2", "1"]);
    }

    /// Fuzz `insert` + `pollFirst` against `OpenJDK`. On failure, panic message includes `seed` — save
    /// the script from `random_script(seed, ops)` to reproduce.
    /// Live-weight `u` lines mirror Java `Cell.weight` updates while keys stay in the `TreeSet`.
    #[test]
    fn random_streams_match_java_treeset() {
        let Ok(cp) = compile_treeset_weight_probe_java() else {
            return;
        };
        const SEEDS: u64 = 256;
        const OPS: usize = 320;
        for seed in 0..SEEDS {
            let script = random_mutable_script(seed, OPS);
            let j = java_poll_lines(&script, &cp).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            let r = replay_treeset_weight_probe_polls(&script);
            assert_eq!(
                j, r,
                "poll mismatch seed={seed} (replay with random_mutable_script({seed}, {OPS}))"
            );
        }
    }
}
