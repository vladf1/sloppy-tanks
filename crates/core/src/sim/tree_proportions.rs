//! Shared deterministic proportions for the standing tree, fallen trunk and rooted stump.

use std::f64::consts::PI;

use super::math::{Random, js_round, to_int32};

#[derive(Clone, Debug, PartialEq)]
pub struct TreeProportions {
    pub seed: u32,
    /// The stream after the proportion draws; models continue drawing from it.
    pub rng: Random,
    pub family: u32,
    pub twist: f64,
    pub height: f64,
    pub radius: f64,
    pub stump_height: f64,
    pub stump_radius: f64,
}

/// Proportions for a tree cover footprint at (x, z) with size w/d/h.
pub fn tree_proportions(x: f64, z: f64, w: f64, d: f64, h: f64) -> TreeProportions {
    let seed = (to_int32(js_round(x * 100.0) * 73_856_093.0) ^ to_int32(js_round(z * 100.0) * 19_349_663.0)) as u32;
    let mut rng = Random::new(seed as f64);
    let family = (rng.next() * 6.0).floor() as u32;
    let twist = rng.range(0.0, PI * 2.0);
    let height = h * rng.range(0.9, 1.07);
    let radius = w.min(d)
        * if family == 3 {
            0.14
        } else if family >= 4 {
            0.1
        } else {
            0.12
        };
    let stump_height = radius * rng.range(1.5, 1.9);
    // Block the solid flared trunk, not the thin roots extending along the ground.
    TreeProportions {
        seed,
        rng,
        family,
        twist,
        height,
        radius,
        stump_height,
        stump_radius: radius * 1.25,
    }
}
