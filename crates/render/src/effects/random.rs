//! Cosmetic randomness, the former `Math.random`. It is deliberately separate from
//! the seeded simulation stream (`simulation.rng`): effects may draw from it in any
//! order without changing bot decisions, trajectories or outcomes.

/// SplitMix64 in [0, 1). Tests can pin it to a constant, like mocking `Math.random`.
#[derive(Clone, Debug)]
pub struct CosmeticRandom {
    state: u64,
    fixed: Option<f64>,
}

impl Default for CosmeticRandom {
    fn default() -> Self {
        Self::seeded(0x5eed_7a4c_u64)
    }
}

impl CosmeticRandom {
    pub fn seeded(seed: u64) -> Self {
        Self {
            state: seed,
            fixed: None,
        }
    }

    /// Always returns `value` (the tests' `Math.random = () => 0.5`).
    pub fn constant(value: f64) -> Self {
        Self {
            state: 0,
            fixed: Some(value),
        }
    }

    pub fn next(&mut self) -> f64 {
        if let Some(value) = self.fixed {
            return value;
        }
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_stay_in_the_unit_interval() {
        let mut random = CosmeticRandom::seeded(7);
        let mut sum = 0.0;
        for _ in 0..10_000 {
            let value = random.next();
            assert!((0.0..1.0).contains(&value));
            sum += value;
        }
        assert!((sum / 10_000.0 - 0.5).abs() < 0.02);
        assert_eq!(CosmeticRandom::constant(0.5).next(), 0.5);
    }
}
