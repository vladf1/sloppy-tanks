//! Barrel (drum) collision shape.

use std::f64::consts::PI;

use rapier3d::prelude::ColliderBuilder;

use super::physics::convex_hull;

/// Match the twelve-sided barrel: it rolls, then rests on a facet instead of creeping forever.
pub fn barrel_collider(w: f64, h: f64, d: f64) -> ColliderBuilder {
    let mut points = Vec::with_capacity(24 * 3);
    for y in [-h / 2.0, h / 2.0] {
        for i in 0..12 {
            let angle = (i as f64 * PI) / 6.0;
            points.push(((angle.sin() * w) / 2.0) as f32);
            points.push(y as f32);
            points.push(((angle.cos() * d) / 2.0) as f32);
        }
    }
    convex_hull(&points)
}
