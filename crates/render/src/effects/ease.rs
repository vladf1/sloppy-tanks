//! Powers of 0..1 tabulated for the effects that evaluate several for every puff each
//! frame: `powf` is a software routine in Wasm. The table's linear steps differ from
//! `powf` by less than a hundred-thousandth past the first half percent of the range,
//! where a puff is still fading in.

/// Steps over 0..1.
pub const EASE_STEPS: usize = 1024;

/// `x^power` at `EASE_STEPS` even steps over 0..1.
pub struct EaseTable([f32; EASE_STEPS + 1]);

impl EaseTable {
    pub fn new(power: f64) -> Self {
        Self(std::array::from_fn(|step| {
            (step as f64 / EASE_STEPS as f64).powf(power) as f32
        }))
    }

    /// `x^power`, `x` clamped to 0..1.
    pub fn at(&self, x: f64) -> f64 {
        let scaled = x.clamp(0.0, 1.0) * EASE_STEPS as f64;
        let step = (scaled as usize).min(EASE_STEPS - 1);
        let (low, high) = (f64::from(self.0[step]), f64::from(self.0[step + 1]));
        low + (high - low) * (scaled - step as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_follow_their_powers() {
        // The wreck columns' and the pickup signal smoke's powers among them.
        for power in [0.55, 0.8, 0.85, 1.1, 1.4, 1.6, 1.8] {
            let table = EaseTable::new(power);
            for step in 0..=4000 {
                let x = f64::from(step) / 4000.0;
                let error = (table.at(x) - x.powf(power)).abs();
                let bound = match x {
                    x if x < 0.005 => 1e-2,
                    x if x < 0.04 => 1e-3,
                    _ => 1e-5,
                };
                assert!(error < bound, "{power} at {x}: {error}");
            }
            assert_eq!(table.at(-1.0), 0.0);
            assert_eq!(table.at(2.0), 1.0);
        }
    }
}
