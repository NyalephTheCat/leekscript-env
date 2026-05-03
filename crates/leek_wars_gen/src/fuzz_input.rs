//! Byte-decodable fuzz input model (“possibility fields”).
//!
//! This is used by two drivers:
//! - `leekgen-compare --fuzz`: long-running campaign driver (RNG-based), which generates a fresh
//!   [`FuzzInput`] per iteration and records it in artifacts for deterministic replay.
//! - `cargo fuzz`: libFuzzer targets can take the raw bytes as corpus input and minimize them.
//!
//! The encoding is intentionally simple and stable: a fixed-size header with a seed, followed by
//! single-byte weights/magnitudes.

use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

/// Fixed-size on-disk / on-wire encoding for [`FuzzInput`].
///
/// Keep this stable so existing corpora remain usable.
/// `8` (seed) + `17` scenario/AI weight bytes (see [`FuzzInput`] fields after `seed`).
pub const FUZZ_INPUT_BYTES: usize = 8 + 17;

/// Probability weights are in `0..=255` (0 = never, 255 = always).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzInput {
    /// Per-iteration deterministic seed (recorded in artifacts).
    pub seed: u64,

    // --- scenario knobs ----------------------------------------------------
    pub p_fuzz_random_seed: u8,
    pub p_shuffle_ais: u8,
    pub p_allow_external_ais: u8,

    pub p_jitter_entity_stats: u8,
    pub mag_entity_stats: u8,

    pub p_jitter_max_turns: u8,
    pub mag_max_turns: u8,

    pub p_randomize_draw_rule: u8,
    pub p_jitter_map_obstacles: u8,
    pub p_jitter_entity_cells: u8,
    pub p_jitter_max_operations: u8,
    pub p_jitter_entity_loadouts: u8,

    // --- AI mutation knobs -------------------------------------------------
    /// 0..=4 (0 = off). Higher = more edits.
    pub mutate_ai_level: u8,
    pub mutate_ai_require_parseable: u8,   // probability
    pub mutate_ai_inject_complexity: u8,   // magnitude-ish
    pub mutate_ai_inject_wrap_percent: u8, // 0..=100
    pub mutate_ai_inject_max_stmts: u8,    // 1..=16
}

impl Default for FuzzInput {
    fn default() -> Self {
        Self {
            seed: 0,
            p_fuzz_random_seed: 255,
            p_shuffle_ais: 255,
            p_allow_external_ais: 0,
            p_jitter_entity_stats: 0,
            mag_entity_stats: 64,
            p_jitter_max_turns: 0,
            mag_max_turns: 64,
            p_randomize_draw_rule: 0,
            p_jitter_map_obstacles: 0,
            p_jitter_entity_cells: 0,
            p_jitter_max_operations: 0,
            p_jitter_entity_loadouts: 0,
            mutate_ai_level: 1,
            mutate_ai_require_parseable: 0,
            mutate_ai_inject_complexity: 2,
            mutate_ai_inject_wrap_percent: 55,
            mutate_ai_inject_max_stmts: 3,
        }
    }
}

impl FuzzInput {
    /// Deterministic decode from bytes (libFuzzer entrypoint).
    ///
    /// - Empty/short inputs are padded with zeros.
    /// - Some fields are clamped to safe ranges.
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut buf = [0u8; FUZZ_INPUT_BYTES];
        let n = data.len().min(FUZZ_INPUT_BYTES);
        buf[..n].copy_from_slice(&data[..n]);

        let seed = u64::from_le_bytes(buf[0..8].try_into().expect("fixed size"));
        let mut i = 8;
        let mut next = || {
            let b = buf[i];
            i += 1;
            b
        };

        let mut out = Self {
            seed,
            p_fuzz_random_seed: next(),
            p_shuffle_ais: next(),
            p_allow_external_ais: next(),
            p_jitter_entity_stats: next(),
            mag_entity_stats: next(),
            p_jitter_max_turns: next(),
            mag_max_turns: next(),
            p_randomize_draw_rule: next(),
            p_jitter_map_obstacles: next(),
            p_jitter_entity_cells: next(),
            p_jitter_max_operations: next(),
            p_jitter_entity_loadouts: next(),
            mutate_ai_level: next(),
            mutate_ai_require_parseable: next(),
            mutate_ai_inject_complexity: next(),
            mutate_ai_inject_wrap_percent: next(),
            mutate_ai_inject_max_stmts: next(),
        };

        out.mutate_ai_level = out.mutate_ai_level.min(4);
        out.mutate_ai_inject_wrap_percent = out.mutate_ai_inject_wrap_percent.min(100);
        out.mutate_ai_inject_max_stmts = out.mutate_ai_inject_max_stmts.clamp(1, 16);

        out
    }

    /// Stable, fixed-size encoding for artifacts / corpora.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; FUZZ_INPUT_BYTES] {
        let mut out = [0u8; FUZZ_INPUT_BYTES];
        out[0..8].copy_from_slice(&self.seed.to_le_bytes());
        let mut i = 8usize;
        let mut put = |b: u8| {
            out[i] = b;
            i += 1;
        };
        put(self.p_fuzz_random_seed);
        put(self.p_shuffle_ais);
        put(self.p_allow_external_ais);
        put(self.p_jitter_entity_stats);
        put(self.mag_entity_stats);
        put(self.p_jitter_max_turns);
        put(self.mag_max_turns);
        put(self.p_randomize_draw_rule);
        put(self.p_jitter_map_obstacles);
        put(self.p_jitter_entity_cells);
        put(self.p_jitter_max_operations);
        put(self.p_jitter_entity_loadouts);
        put(self.mutate_ai_level);
        put(self.mutate_ai_require_parseable);
        put(self.mutate_ai_inject_complexity);
        put(self.mutate_ai_inject_wrap_percent);
        put(self.mutate_ai_inject_max_stmts);
        out
    }

    /// Convenience: render bytes as lowercase hex for `meta.json`.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let b = self.to_bytes();
        let mut s = String::with_capacity(b.len() * 2);
        for x in b {
            use std::fmt::Write;
            write!(&mut s, "{x:02x}").expect("fmt");
        }
        s
    }

    /// Decode from lowercase/uppercase hex produced by [`Self::to_hex`].
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        let s = hex.trim();
        if !s.len().is_multiple_of(2) {
            return None;
        }
        let mut bytes = Vec::with_capacity(s.len() / 2);
        let mut it = s.as_bytes().chunks_exact(2);
        for pair in &mut it {
            let h = (pair[0] as char).to_digit(16)? as u8;
            let l = (pair[1] as char).to_digit(16)? as u8;
            bytes.push((h << 4) | l);
        }
        Some(Self::from_bytes(&bytes))
    }

    /// Construct a fresh [`FuzzInput`] by expanding a seed into bytes.
    ///
    /// This is used by the RNG-based campaign driver so each iteration can be recorded and replayed
    /// via the same libFuzzer-style input.
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut buf = [0u8; FUZZ_INPUT_BYTES];
        rng.fill_bytes(&mut buf);
        buf[0..8].copy_from_slice(&seed.to_le_bytes());
        Self::from_bytes(&buf)
    }

    /// `true` with probability `p/255` (cheap, deterministic for a given RNG stream).
    pub fn roll(&self, rng: &mut StdRng, p: u8) -> bool {
        if p == 0 {
            return false;
        }
        if p == 255 {
            return true;
        }
        rng.next_u32() as u8 <= p
    }

    /// Convert a `0..=255` magnitude into a small integer scale in `1..=4`.
    #[must_use]
    pub fn mag_scale(m: u8) -> i64 {
        1 + (i64::from(m) / 85) // 0..84 => 1, 85..169 => 2, 170..254 => 3, 255 => 4
    }
}
