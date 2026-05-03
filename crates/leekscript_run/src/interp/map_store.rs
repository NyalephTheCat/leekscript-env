//! Insertion-ordered map/object storage with average O(1) key lookup (Java `HashMap`-style buckets).
//! The previous `Vec` + linear scan made stress tests O(n²) and effectively hang on large loops.

use super::util::values_equal_for_compare;
use super::value::Value;
use std::collections::HashMap;
use std::ops::{Index, IndexMut};
use std::rc::Rc;

#[derive(Clone, Debug, Default)]
pub struct MapStore {
    entries: Vec<(Value, Value)>,
    buckets: HashMap<u64, Vec<usize>>,
}

fn mix(tag: u8, payload: u64) -> u64 {
    0x9e37_79b9_7f4a_7c15u64
        .wrapping_mul(u64::from(tag) + 1)
        .wrapping_add(payload)
}

/// Hash partition for bucket lookup. Must match for keys that [`values_equal_for_compare`] treats as equal
/// for scalar numerics (integer / whole real / bool / null coercions).
pub fn map_bucket_hash(k: &Value) -> u64 {
    use Value::{
        Array, Bool, Function, Instance, Integer, Interval, Map, Native, Null, Object, Real,
        RealDotZero, Set, String, Super, UserClass,
    };
    match k {
        Integer(i) => mix(1, *i as u64),
        Bool(b) => mix(1, u64::from(*b)),
        Null => mix(1, 0),
        Real(r) | RealDotZero(r) => {
            if r.is_finite() && r.fract() == 0.0 && *r >= i64::MIN as f64 && *r <= i64::MAX as f64 {
                mix(1, *r as i64 as u64)
            } else {
                mix(2, r.to_bits())
            }
        }
        String(s) => {
            let mut h = 0u64;
            for b in s.as_bytes().chunks(8) {
                let mut x = 0u64;
                for (i, &bb) in b.iter().enumerate() {
                    x |= u64::from(bb) << (i * 8);
                }
                h = h.wrapping_mul(0x100000001b3).wrapping_add(x);
            }
            mix(3, h.wrapping_add(s.len() as u64))
        }
        Array(a) => mix(4, Rc::as_ptr(a) as usize as u64),
        Map(m) | Object(m) => mix(5, Rc::as_ptr(m) as usize as u64),
        Set(s) => mix(6, Rc::as_ptr(s) as usize as u64),
        Interval(iv) => mix(
            7,
            u64::from(u8::from(iv.min_closed))
                .wrapping_add(u64::from(u8::from(iv.max_closed)) << 8)
                .wrapping_add(iv.min.to_bits())
                .wrapping_add(iv.max.to_bits().rotate_left(17)),
        ),
        Function(f) => mix(8, Rc::as_ptr(f) as usize as u64),
        Native(n) => mix(9, n.as_ptr() as usize as u64),
        Instance(i) => mix(10, Rc::as_ptr(i) as usize as u64),
        UserClass(c) => {
            let mut h = 0u64;
            for b in c.as_bytes() {
                h = h.wrapping_mul(31).wrapping_add(u64::from(*b));
            }
            mix(11, h.wrapping_add(c.len() as u64))
        }
        Super => mix(12, 0),
    }
}

impl MapStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_pairs(pairs: Vec<(Value, Value)>) -> Self {
        let mut s = Self {
            entries: Vec::new(),
            buckets: HashMap::new(),
        };
        s.replace_all(pairs);
        s
    }

    pub fn replace_all(&mut self, pairs: Vec<(Value, Value)>) {
        self.clear();
        self.entries = pairs;
        self.rebuild_buckets();
    }

    fn rebuild_buckets(&mut self) {
        self.buckets.clear();
        for (i, (k, _)) in self.entries.iter().enumerate() {
            let h = map_bucket_hash(k);
            self.buckets.entry(h).or_default().push(i);
        }
    }

    fn bucket_indices(&self, k: &Value) -> &[usize] {
        let h = map_bucket_hash(k);
        self.buckets
            .get(&h)
            .map(std::vec::Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn find_key(&self, k: &Value) -> Option<usize> {
        self.bucket_indices(k)
            .iter()
            .find(|&&i| values_equal_for_compare(&self.entries[i].0, k))
            .copied()
    }

    pub fn find_key_legacy(&self, k: &Value) -> Option<usize> {
        if let Some(p) = self.find_key(k) {
            return Some(p);
        }
        self.entries
            .iter()
            .position(|(mk, _)| super::util::map_stored_key_matches_legacy_query(mk, k))
    }

    pub fn as_slice(&self) -> &[(Value, Value)] {
        &self.entries
    }

    pub fn to_vec(&self) -> Vec<(Value, Value)> {
        self.entries.clone()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn last_pair(&self) -> Option<&(Value, Value)> {
        self.entries.last()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.buckets.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &(Value, Value)> {
        self.entries.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (Value, Value)> {
        self.entries.iter_mut()
    }

    pub fn reverse_in_place(&mut self) {
        self.entries.reverse();
        self.rebuild_buckets();
    }

    fn add_index(&mut self, idx: usize, k: &Value) {
        let h = map_bucket_hash(k);
        self.buckets.entry(h).or_default().push(idx);
    }

    /// Append a new key (caller must ensure key is absent).
    pub fn push_kv(&mut self, k: Value, v: Value) {
        let idx = self.entries.len();
        self.add_index(idx, &k);
        self.entries.push((k, v));
    }

    /// `Vec::push`-compatible alias (insertion-ordered map).
    pub fn push(&mut self, pair: (Value, Value)) {
        self.push_kv(pair.0, pair.1);
    }

    /// Remove the entry at `p` while preserving insertion order of remaining pairs (Java `Map` / `LinkedHashMap`).
    pub fn remove_ordered(&mut self, p: usize) -> (Value, Value) {
        let kv = self.entries.remove(p);
        self.rebuild_buckets();
        kv
    }

    pub fn retain<F: FnMut(&(Value, Value)) -> bool>(&mut self, mut pred: F) {
        self.entries.retain(|e| pred(e));
        self.rebuild_buckets();
    }
}

impl IntoIterator for MapStore {
    type Item = (Value, Value);
    type IntoIter = std::vec::IntoIter<(Value, Value)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl PartialEq for MapStore {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl Index<usize> for MapStore {
    type Output = (Value, Value);

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl IndexMut<usize> for MapStore {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}
