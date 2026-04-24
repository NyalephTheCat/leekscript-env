//! Official generator `State` anonymous [`RandomGenerator`](https://github.com/leek-wars/leek-wars-generator) (LCG + `getDouble`).

/// RNG used for `StartOrder` team shuffling (`state.getRandom().getDouble()`).
pub trait TurnOrderRng {
    fn next_double01(&mut self) -> f64;
}

/// Matches `com.leekwars.generator.state.State` field `randomGenerator` (`n = n*1103515245+12345`, etc.).
#[derive(Debug, Clone)]
pub struct JavaCompatRng {
    n: i64,
}

impl JavaCompatRng {
    pub fn new(seed: i64) -> Self {
        Self { n: seed }
    }

    /// Internal LCG state after the last [`Self::step_double`] (matches Official generator `State` anonymous `RandomGenerator.n`).
    pub fn internal_n(&self) -> i64 {
        self.n
    }

    /// Resume the generator-compatible stream from a captured post-`State.init()` value (see `com.leekwars.DumpStateRng`).
    pub fn from_internal_n(n: i64) -> Self {
        Self { n }
    }

    fn step_double(&mut self) -> f64 {
        self.n = self.n.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let r = (self.n / 65_536) % 32_768 + 32_768;
        r as f64 / 65_536.0
    }

    /// Same formula as the official generator `getDouble()`.
    pub fn next_double01(&mut self) -> f64 {
        self.step_double()
    }

    /// Official generator: `getInt(min, max)` inclusive.
    pub fn next_int_inclusive(&mut self, min: i32, max: i32) -> i32 {
        if max - min + 1 <= 0 {
            return 0;
        }
        min + (self.step_double() * (max - min + 1) as f64) as i32
    }
}

impl TurnOrderRng for JavaCompatRng {
    fn next_double01(&mut self) -> f64 {
        self.step_double()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference: official generator RNG 1234567 (see workspace history).
    const EXPECTED: &[f64] = &[
        0.97186279296875,
        0.250274658203125,
        0.7882537841796875,
        0.72357177734375,
        0.2124786376953125,
        0.8607940673828125,
        0.50604248046875,
        0.0779876708984375,
        0.7020111083984375,
        0.0838775634765625,
    ];

    #[test]
    fn java_reference_doubles() {
        let mut g = JavaCompatRng::new(1_234_567);
        for (i, exp) in EXPECTED.iter().enumerate() {
            let v = g.next_double01();
            assert!((v - *exp).abs() < 1e-15, "i={} got {} want {}", i, v, exp);
        }
    }

    /// `com.leekwars.DumpStateRng` on `test/scenario/scenario1.json` after `Fight.initFight` (generator.jar).
    #[test]
    fn java_scenario1_internal_n_after_state_init() {
        let target = -5_933_333_234_847_835_179_i64;
        let mut g = JavaCompatRng::new(1_234_567);
        let mut draws = 0u32;
        loop {
            g.next_double01();
            draws += 1;
            if g.internal_n() == target {
                assert!(
                    draws > 7,
                    "expected many map draws before StartOrder; got {draws}"
                );
                return;
            }
            assert!(draws < 500_000, "no match after {draws} draws");
        }
    }
}
