//! The land round Senda past its walls: foothills to the north and east, ridges a few
//! dozen metres high whose spurs reach toward the tract between folds, lower rolling
//! ground to the south and west behind the next rows of homes, and a valley where Senda
//! Parkway climbs away north-eastward. Dry tan grass, olive scrub and dark chaparral by
//! slope and patch, oaks in the folds, and on the nearest north and east slopes the
//! tract stepping on uphill: rows of homes on pads cut into the hillside, the cuts behind
//! them planted in terraces.
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
/// A pad cut into the hillside for a row of homes: how far (m) it reaches behind their
/// backs and before their fronts, and the run (m) over which the hillside meets it.
const PAD_BEHIND: f64 = 3.0;
const PAD_BEFORE: f64 = 12.5;
const PAD_ENDS: f64 = 3.0;
const PAD_BLEND: f64 = 9.0;
/// The cut slopes behind the pads are planted in terraces: this many metres of rise to a
/// terrace, and how far (m) up the slope behind a pad they reach.
const TERRACE_RISE: f64 = 2.4;
const TERRACE_REACH: f64 = 14.0;
const TERRACE_BENCHES: usize = 4;
const TERRACE_SPACING: f64 = 3.4;

/// One row of the tract on the hills: houses from `from` to `to`, their fronts toward
/// `out` (downhill), their backs on the line, on a pad cut level at the hillside's height
/// at the row's middle.
#[derive(Clone, Copy, Debug)]
pub(super) struct HillRow {
    pub from: DVec2,
    pub to: DVec2,
    pub out: DVec2,
    pub level: f64,
}

/// The rows of homes stepping up the nearest north and east slopes, before their pads'
/// levels are taken from the hillside.
const HILL_ROWS: [(DVec2, DVec2, DVec2); 3] = [
    (
        DVec2::new(-54.0, -101.0),
        DVec2::new(-14.0, -101.0),
        DVec2::new(0.0, 1.0),
    ),
    (
        DVec2::new(-46.0, -118.0),
        DVec2::new(-6.0, -118.0),
        DVec2::new(0.0, 1.0),
    ),
    (
        DVec2::new(104.0, 6.0),
        DVec2::new(104.0, 46.0),
        DVec2::new(-1.0, 0.0),
    ),
];

/// The land past the walls, from Senda Parkway's line and the rows on the hills.
pub(super) struct Hills {
    way: Line,
    /// Where the Way's trace ends and its heading there: its valley runs on past it.
    way_end: DVec2,
    way_out: DVec2,
    pub rows: Vec<HillRow>,
}

impl Hills {
    pub fn new() -> Hills {
        let way = Street::SendaParkway.line();
        let n = way.0.len();
        let way_end = way.0[n - 1];
        let way_out = (way_end - way.0[n - 2]).normalize();
        let mut hills = Hills {
            way,
            way_end,
            way_out,
            rows: Vec::new(),
        };
        hills.rows = HILL_ROWS
            .iter()
            .map(|&(from, to, out)| {
                let middle = (from + to) / 2.0 + out * (PAD_BEFORE - PAD_BEHIND) / 2.0;
                HillRow {
                    from,
                    to,
                    out,
                    level: hills.natural(middle.x, middle.y),
                }
            })
            .collect();
        hills
    }

    /// The ground's height at `(x, z)`: level over the neighbourhood, rising past the
    /// walls into the hills, cut level under the rows' pads.
    pub fn height(&self, x: f64, z: f64) -> f64 {
        let natural = self.natural(x, z);
        let p = DVec2::new(x, z);
        self.rows.iter().fold(natural, |h, row| {
            let pad = pad_share(row, p);
            h + (row.level - h) * pad
        })
    }

    /// How much of the ground at `(x, z)` is the hills' own (0 on the level's ground and
    /// the valley floor, 1 up their slopes).
    pub fn share(&self, x: f64, z: f64) -> f64 {
        smooth((self.natural(x, z) - 0.3) / 2.5)
    }

    /// Whether `p` lies on a row's pad.
    pub fn on_pad(&self, p: DVec2) -> bool {
        self.rows.iter().any(|row| pad_share(row, p) > 0.5)
    }

    /// How far into the terraced cut behind a row's pad `p` lies, 0 to 1.
    pub fn terraced(&self, p: DVec2) -> f64 {
        self.rows
            .iter()
            .map(|row| {
                let (u, v, length) = row_frame(row, p);
                let ends = smooth((u + PAD_ENDS) / PAD_BLEND)
                    * smooth((length + PAD_ENDS - u) / PAD_BLEND);
                let behind = -PAD_BEHIND - v;
                ends * smooth(behind / 2.0) * smooth((TERRACE_REACH - behind) / 4.0)
            })
            .fold(0.0, f64::max)
    }

    /// The height of the planted terrace benches up a cut at `p`, 0 to 1 across each
    /// bench's rise: the lower half of each bench is its planted row.
    pub fn terrace_band(&self, x: f64, z: f64) -> f64 {
        (self.natural(x, z) / TERRACE_RISE).fract()
    }

    /// The hills without the pads.
    fn natural(&self, x: f64, z: f64) -> f64 {
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

    /// The planted rows along the terraces up the cut behind each row's pad: a spot every
    /// [`TERRACE_SPACING`] along each bench, each bench [`TERRACE_RISE`] above the last,
    /// up to [`TERRACE_REACH`] behind the pad; with each a seed.
    pub fn terrace_plants(&self) -> Vec<(DVec2, f64)> {
        let mut plants = Vec::new();
        for (r, row) in self.rows.iter().enumerate() {
            let length = row.from.distance(row.to);
            let along = (row.to - row.from) / length;
            let mut u = -PAD_ENDS;
            while u < length + PAD_ENDS {
                for bench in 1..=TERRACE_BENCHES {
                    let wanted = row.level + TERRACE_RISE * bench as f64;
                    let mut v = -PAD_BEHIND;
                    while v > -PAD_BEHIND - TERRACE_REACH {
                        let at = row.from + along * u + row.out * v;
                        if self.natural(at.x, at.y) >= wanted {
                            plants.push((at, r as f64 * 101.0 + u * 3.7 + bench as f64 * 1.3));
                            break;
                        }
                        v -= 0.5;
                    }
                }
                u += TERRACE_SPACING;
            }
        }
        plants
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
                if self.share(at.x, at.y) > 0.5
                    && !self.on_pad(at)
                    && at.x.abs().max(at.y.abs()) < 146.0
                {
                    oaks.push((at, seed));
                }
            }
        }
        oaks
    }
}

/// Where `p` lies in `row`'s frame: along it from `from`, out toward its fronts from its
/// backs' line, and the row's length.
fn row_frame(row: &HillRow, p: DVec2) -> (f64, f64, f64) {
    let length = row.from.distance(row.to);
    let along = (row.to - row.from) / length;
    let d = p - row.from;
    (d.dot(along), d.dot(row.out), length)
}

/// How much of `p` lies on `row`'s pad, 0 to 1 across the hillside's run onto it.
fn pad_share(row: &HillRow, p: DVec2) -> f64 {
    let (u, v, length) = row_frame(row, p);
    let inside = (u + PAD_ENDS)
        .min(length + PAD_ENDS - u)
        .min(v + PAD_BEHIND)
        .min(PAD_BEFORE - v);
    smooth(inside / PAD_BLEND + 1.0)
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
