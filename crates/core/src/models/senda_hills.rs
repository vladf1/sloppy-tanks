//! The land round Senda past its walls: foothills to the north and east, ridges a few
//! dozen metres high whose spurs reach toward the tract between folds, lower rolling
//! ground to the south and west, and a valley where Senda Parkway climbs away
//! north-eastward. Dry tan grass, olive scrub and dark chaparral by slope and patch, and
//! oaks in the folds. No homes stand on them.
//!
//! Everything here lies outside the playfield: the hills rise only past the walls and the
//! streets' ends, and the overhead camera, which looks north from the tank's south, sees
//! them only beyond the level's north edge.

use glam::DVec2;

use crate::sim::senda_layout::{Line, Street};

/// How round the hills' footing is: a rounded square, between the arena's square and a
/// circle.
const FOOT_ROUNDNESS: f64 = 5.0;
/// The foothills to the north and east: where they begin rising (rounded-square radius,
/// m), how far their spurs reach in and their folds draw back (m), the horizontal run
/// from their foot to the crest (m), and the crests' height range (m).
const FOOTHILL_FOOT: f64 = 90.0;
const FOOTHILL_SPUR: f64 = 7.0;
const FOOTHILL_RUN: f64 = 82.0;
const FOOTHILL_HEIGHT: (f64, f64) = (24.0, 40.0);
/// The rolling ground to the south and west: its foot, spurs, run and heights.
const ROLLING_FOOT: f64 = 92.0;
const ROLLING_SPUR: f64 = 6.0;
const ROLLING_RUN: f64 = 55.0;
const ROLLING_HEIGHT: (f64, f64) = (3.0, 7.5);
/// The ridges dip into a saddle toward the east-north-east (the heading from the arena's
/// middle, world x and z), so the far range's twin summits show over it from the
/// streets: its share of their height kept at its lowest, and its half width (radians).
const SADDLE_TOWARD: (f64, f64) = (0.9, -0.44);
const SADDLE_DEPTH: f64 = 0.82;
const SADDLE_WIDTH: f64 = 0.3;
/// Senda Parkway's valley: from the paving's edge out, the floor stays level this far
/// (m), then its sides rise over this run (m) to the hills' own height.
const VALLEY_FLOOR: f64 = 4.0;
const VALLEY_SIDES: f64 = 26.0;

/// The land past the walls, from Senda Parkway's line.
pub(super) struct Hills {
    way: Line,
    /// Where the Way's trace ends and its heading there: its valley runs on past it.
    way_end: DVec2,
    way_out: DVec2,
}

impl Hills {
    pub fn new() -> Hills {
        let way = Street::SendaParkway.line();
        let n = way.0.len();
        let way_end = way.0[n - 1];
        let way_out = (way_end - way.0[n - 2]).normalize();
        Hills {
            way,
            way_end,
            way_out,
        }
    }

    /// How much of the ground at `(x, z)` is the hills' own (0 on the level's ground and
    /// the valley floor, 1 up their slopes).
    pub fn share(&self, x: f64, z: f64) -> f64 {
        smooth((self.height(x, z) - 0.3) / 2.5)
    }

    /// The ground's height at `(x, z)`: level over the neighbourhood, rising past the
    /// walls into the hills.
    pub fn height(&self, x: f64, z: f64) -> f64 {
        let r = (x.abs().powf(FOOT_ROUNDNESS) + z.abs().powf(FOOT_ROUNDNESS))
            .powf(1.0 / FOOT_ROUNDNESS);
        if r < ROLLING_FOOT.min(FOOTHILL_FOOT) - FOOTHILL_SPUR - 1.0 {
            return 0.0;
        }
        let length = x.hypot(z).max(1e-6);
        let toward = DVec2::new(x, z) / length;
        // North and east the foothills, south and west the rolling ground, blending
        // round the north-west and south-east.
        let north_east = toward.dot(DVec2::new(1.0, -1.0) / 2f64.sqrt());
        let w = smooth((north_east + 0.25) / 0.7);
        let angle = z.atan2(x);
        // Waves round the ring in whole turns, so they close on themselves.
        let wave = |turns: f64, phase: f64| (angle * turns + phase).sin();
        let spur = 0.75 * wave(10.0, 1.3) + 0.25 * wave(17.0, 0.2);
        let swell = 0.5 + 0.5 * (0.65 * wave(3.0, 0.8) + 0.35 * wave(5.0, 2.1));
        let saddle_angle = SADDLE_TOWARD.1.atan2(SADDLE_TOWARD.0);
        let off = wrap(angle - saddle_angle);
        let saddle = 1.0 - SADDLE_DEPTH * (-(off / SADDLE_WIDTH).powi(2)).exp();
        let lerp = |a: f64, b: f64| a + (b - a) * w;
        let foot = lerp(
            ROLLING_FOOT - ROLLING_SPUR * spur,
            FOOTHILL_FOOT - FOOTHILL_SPUR * spur,
        );
        let run = lerp(ROLLING_RUN, FOOTHILL_RUN);
        let crest = lerp(
            ROLLING_HEIGHT.0 + (ROLLING_HEIGHT.1 - ROLLING_HEIGHT.0) * swell,
            (FOOTHILL_HEIGHT.0 + (FOOTHILL_HEIGHT.1 - FOOTHILL_HEIGHT.0) * swell) * saddle,
        );
        let d = r - foot;
        if d <= 0.0 {
            return 0.0;
        }
        let t = d / run;
        // Up to the crest, then on over rolling ground behind it.
        // A long concave foot, a broad rounded shoulder, and on over rolling ground.
        let shoulder = smooth(t).powf(0.85);
        let profile = shoulder * (1.0 + 0.12 * (d * 0.045 + angle * 4.0).sin() * smooth(t - 0.6));
        // Gullies down the slopes between the spurs, and a few metres of lumpy relief.
        let gully =
            (wave(29.0, 0.7) * 0.5 + 0.5).powi(4) * smooth(t * 2.0) * (1.0 - smooth(t - 0.8));
        let relief = 0.7 * (x * 0.31 + z * 0.17).sin() * (z * 0.27 - x * 0.13).cos();
        let h = crest * profile * (1.0 - 0.08 * gully) + relief * smooth(t * 3.0);
        h.max(0.0) * self.valley(DVec2::new(x, z))
    }

    /// Senda Parkway's valley: 0 on its floor, 1 where the hills keep their height.
    fn valley(&self, p: DVec2) -> f64 {
        let ahead = (p - self.way_end).dot(self.way_out);
        let beyond = if ahead > 0.0 {
            p.distance(self.way_end + self.way_out * ahead)
        } else {
            f64::INFINITY
        };
        let distance = self.way.distance(p).min(beyond) - Street::SendaParkway.half();
        smooth((distance - VALLEY_FLOOR) / VALLEY_SIDES)
    }

    /// Oaks in the hills' folds: at the foot of each fold between two spurs on the
    /// foothills, a little way up, and a few on the rolling ground; each a spot and a
    /// seed. The folds are where the spurs' wave bottoms out.
    pub fn fold_oaks(&self) -> Vec<(DVec2, f64)> {
        let mut oaks = Vec::new();
        let steps = 720;
        let spur_at =
            |angle: f64| 0.75 * (angle * 10.0 + 1.3).sin() + 0.25 * (angle * 17.0 + 0.2).sin();
        for k in 0..steps {
            let angle = k as f64 / steps as f64 * std::f64::consts::TAU - std::f64::consts::PI;
            let step = std::f64::consts::TAU / steps as f64;
            let (before, here, after) =
                (spur_at(angle - step), spur_at(angle), spur_at(angle + step));
            if !(here < before && here <= after) {
                continue;
            }
            let toward = DVec2::new(angle.cos(), angle.sin());
            for n in 0..4 {
                let seed = k as f64 * 3.1 + n as f64 * 1.7;
                let reach = 104.0 + 20.0 * hash(seed) + n as f64 * 3.0;
                let across = DVec2::new(-toward.y, toward.x) * (hash(seed + 0.5) - 0.5) * 8.0;
                let at = toward * reach + across;
                if self.share(at.x, at.y) > 0.5 && at.x.abs().max(at.y.abs()) < 146.0 {
                    oaks.push((at, seed));
                }
            }
        }
        oaks
    }
}

/// An angle wrapped into -pi..pi.
fn wrap(angle: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    (angle + std::f64::consts::PI).rem_euclid(tau) - std::f64::consts::PI
}

/// Smoothstep from 0 to 1.
fn smooth(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A fraction from 0 to 1 that follows from `a` alone.
fn hash(a: f64) -> f64 {
    ((a * 12.9898).sin() * 43_758.545).fract().abs()
}
