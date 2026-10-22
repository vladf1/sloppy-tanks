//! The tracks Pine Village's traffic has worn into its ground: tank treads that
//! wander along the roads, drift between lanes and fade where few drove, arcs
//! swinging through the junctions, pivot scars where a tank turned on the spot,
//! paired treads across the lawns where drivers cut through, and puddles lying
//! in the deepest stretches of rut.
//!
//! The ground plan (`village_ground.rs`) bakes them into its third square, over
//! the same ±[`ARENA_PLAN_HALF`] m as the arena square:
//!
//! - r tread proximity: `1 - d / TREAD_REACH` to the nearest tread's centreline,
//!   a distance field the shader turns into crisp, anti-aliased ruts between
//!   texels;
//! - g how fresh that tread is (0 worn away, 1 freshly cut);
//! - b traffic, how much driving packed the ground here (smooth, metres wide);
//! - a dryness, lower where water stands (kept at least 0.4 so nothing leans
//!   on colour under transparent texels).

use glam::DVec2;

use super::village_ground::{ARENA_PLAN_HALF, GROUND_PLAN_SIZE, SHORTCUTS, hash, noise};
use super::village_roads::village_roads;
use crate::geometry::math::smoothstep;
use crate::sim::arena::arena_layout;
use crate::sim::types::CoverKind;

/// Metres from a tread's centreline over which the proximity ramp falls to 0.
pub const TREAD_REACH: f64 = 1.0;
/// Centre-to-centre spacing of a tank's two tracks.
const TRACK_GAUGE: f64 = 2.7;
/// Spacing of the samples along a driving line.
const LINE_STEP: f64 = 0.5;
/// Cell edge of the coarse traffic grid, in metres.
const TRAFFIC_CELL: f64 = 1.0;
/// Spread of a tread's packing into the ground around it.
const TRAFFIC_SPREAD: f64 = 2.2;

/// One track's mark: a dense polyline in world metres and its freshness there.
struct Tread {
    points: Vec<DVec2>,
    fresh: Vec<f64>,
}

/// A vehicle's path: its centreline and freshness, split into two treads.
struct Path {
    points: Vec<DVec2>,
    fresh: Vec<f64>,
}

impl Path {
    /// The two treads either side of the centreline, `gauge` apart.
    fn treads(&self, gauge: f64) -> [Tread; 2] {
        let normal = |i: usize| {
            let a = self.points[i.saturating_sub(1)];
            let b = self.points[(i + 1).min(self.points.len() - 1)];
            let d = (b - a).normalize_or(DVec2::X);
            DVec2::new(-d.y, d.x)
        };
        [-0.5, 0.5].map(|side| Tread {
            points: (0..self.points.len())
                .map(|i| self.points[i] + normal(i) * gauge * side)
                .collect(),
            fresh: self.fresh.clone(),
        })
    }
}

/// Freshness along a line: some stretches cut deep, others faded to nothing.
fn freshness(t: f64, seed: u64) -> f64 {
    let stretch = smoothstep(noise(t * 0.03, 0.5, seed), 0.3, 0.7);
    stretch * (0.45 + 0.55 * noise(t * 0.12, 3.5, seed + 1))
}

/// Driving lines along every road: each wanders across its strip on long,
/// lazy curves and drifts from lane to lane.
fn road_paths() -> Vec<Path> {
    let mut paths = Vec::new();
    for (index, road) in village_roads().iter().enumerate() {
        let north_south = road.d > road.w;
        let (half, length) = if north_south {
            (road.w / 2.0, road.d)
        } else {
            (road.d / 2.0, road.w)
        };
        // Busy centre roads carry two drivers' lines, the outer ones one or two.
        let lines: usize = if half > 5.5 || hash(index as i64, 9, 7) < 0.5 {
            2
        } else {
            1
        };
        let room = half - 1.7;
        for line in 0..lines {
            let seed = (index * 8 + line) as u64 * 13 + 101;
            let lane = line as f64 - (lines - 1) as f64 / 2.0;
            let base = lane * room * 0.5;
            let phases = [0, 1, 2].map(|k| hash(seed as i64, k, 7) * std::f64::consts::TAU);
            // Each driver's own sway: a long drift across the road and a
            // shorter weave.
            let drift = 0.05 + hash(seed as i64, 3, 7) * 0.05;
            let weave = 0.16 + hash(seed as i64, 4, 7) * 0.12;
            let mut points = Vec::new();
            let mut fresh = Vec::new();
            let mut t = -length / 2.0;
            while t <= length / 2.0 {
                let offset = base
                    + (t * drift + phases[0]).sin() * room * 0.75
                    + (t * weave + phases[1]).sin() * 0.55
                    + (t * 0.43 + phases[2]).sin() * 0.06;
                let across = offset.clamp(-room, room);
                points.push(if north_south {
                    DVec2::new(road.x + across, road.z + t)
                } else {
                    DVec2::new(road.x + t, road.z + across)
                });
                fresh.push(freshness(t, seed));
                t += LINE_STEP;
            }
            paths.push(Path { points, fresh });
        }
    }
    paths
}

/// Turns through the junctions: a run in along one road, a quarter arc and a
/// run out along the crossing road, fading at both ends into the road's traffic.
fn turning_paths() -> Vec<Path> {
    let roads = village_roads();
    let (along, across) = roads.split_at(3);
    let mut paths = Vec::new();
    for (i, a) in along.iter().enumerate() {
        for (j, b) in across.iter().enumerate() {
            for (k, (sx, sz)) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
                .into_iter()
                .enumerate()
            {
                let seed = (i * 16 + j * 4 + k) as i64;
                if hash(seed, 1, 211) > 0.36 {
                    continue;
                }
                let radius = 5.0 + hash(seed, 2, 211) * 7.0;
                let lead = 9.0 + hash(seed, 3, 211) * 9.0;
                let off_a = (hash(seed, 4, 211) - 0.5) * (a.w / 2.0 - 2.0);
                let off_b = (hash(seed, 5, 211) - 0.5) * (b.d / 2.0 - 2.0);
                let start = DVec2::new(a.x + off_a, b.z + off_b + sz * radius);
                let centre = DVec2::new(a.x + off_a + sx * radius, b.z + off_b + sz * radius);
                let mut points = Vec::new();
                let mut s = lead;
                while s > 0.0 {
                    points.push(start + DVec2::new(0.0, sz * s));
                    s -= LINE_STEP;
                }
                let steps = (radius * std::f64::consts::FRAC_PI_2 / LINE_STEP).ceil() as usize;
                for n in 0..=steps {
                    let angle = n as f64 / steps as f64 * std::f64::consts::FRAC_PI_2;
                    points.push(
                        centre + DVec2::new(-sx * angle.cos() * radius, -sz * angle.sin() * radius),
                    );
                }
                let end = centre + DVec2::new(0.0, -sz * radius);
                let mut s = LINE_STEP;
                while s < lead {
                    points.push(end + DVec2::new(sx * s, 0.0));
                    s += LINE_STEP;
                }
                let count = points.len() as f64;
                let strength = 0.55 + hash(seed, 6, 211) * 0.45;
                let fresh = (0..points.len())
                    .map(|n| {
                        let u = n as f64 / (count - 1.0);
                        strength * smoothstep(u, 0.0, 0.25) * (1.0 - smoothstep(u, 0.75, 1.0))
                    })
                    .collect();
                paths.push(Path { points, fresh });
            }
        }
    }
    paths
}

/// Where tanks pivoted on the spot: centre and how fresh the scar is.
const PIVOTS: [(f64, f64, f64); 6] = [
    (-3.5, -36.5, 0.9),
    (4.0, 2.5, 0.7),
    (-50.5, 1.5, 0.8),
    (53.5, -39.0, 0.6),
    (1.5, 40.0, 0.85),
    (-53.0, 37.0, 0.55),
];

/// A pivot's two tracks swept part way round its centre, scuffed deepest where
/// the turn began and fading where the tank drove off.
fn pivot_treads() -> Vec<Tread> {
    let mut treads = Vec::new();
    for (k, (x, z, fresh)) in PIVOTS.into_iter().enumerate() {
        let start = hash(k as i64, 0, 503) * std::f64::consts::TAU;
        let sweep = std::f64::consts::PI * (1.1 + hash(k as i64, 1, 503) * 0.6);
        for radius in [TRACK_GAUGE / 2.0 - 0.15, TRACK_GAUGE / 2.0 + 0.2] {
            let steps = (radius * sweep / 0.25).ceil() as usize;
            let mut points = Vec::with_capacity(steps + 1);
            let mut strength = Vec::with_capacity(steps + 1);
            for n in 0..=steps {
                let u = n as f64 / steps as f64;
                let angle = start + u * sweep;
                let wobble = radius + (u * 9.0 + k as f64).sin() * 0.08;
                points.push(DVec2::new(
                    x + angle.cos() * wobble,
                    z + angle.sin() * wobble,
                ));
                strength.push(fresh * (1.0 - smoothstep(u, 0.45, 1.0)) * smoothstep(u, 0.0, 0.06));
            }
            treads.push(Tread {
                points,
                fresh: strength,
            });
        }
    }
    treads
}

/// A shortcut's polyline, Chaikin-smoothed and resampled, so its treads curve.
fn smooth_polyline(points: &[[f64; 2]]) -> Vec<DVec2> {
    let mut line: Vec<DVec2> = points.iter().map(|&p| DVec2::from(p)).collect();
    for _ in 0..3 {
        let mut next = vec![line[0]];
        for pair in line.windows(2) {
            next.push(pair[0].lerp(pair[1], 0.25));
            next.push(pair[0].lerp(pair[1], 0.75));
        }
        next.push(*line.last().expect("a shortcut has points"));
        line = next;
    }
    let mut dense = vec![line[0]];
    for pair in line.windows(2) {
        let steps = ((pair[1] - pair[0]).length() / LINE_STEP).ceil().max(1.0) as usize;
        for n in 1..=steps {
            dense.push(pair[0].lerp(pair[1], n as f64 / steps as f64));
        }
    }
    dense
}

/// Paired treads across the lawns along the worn shortcuts.
fn shortcut_paths() -> Vec<Path> {
    SHORTCUTS
        .iter()
        .enumerate()
        .map(|(i, (points, _))| {
            let points = smooth_polyline(points);
            let count = points.len() as f64;
            let fresh = (0..points.len())
                .map(|n| {
                    let u = n as f64 / (count - 1.0);
                    (0.7 + 0.3 * noise(u * 6.0, i as f64, 307))
                        * smoothstep(u, 0.0, 0.08)
                        * (1.0 - smoothstep(u, 0.92, 1.0))
                })
                .collect();
            Path { points, fresh }
        })
        .collect()
}

/// Cover footprints the tracks fade out against (nobody drove through a wall).
fn obstacles() -> Vec<(DVec2, DVec2)> {
    arena_layout()
        .iter()
        .filter(|c| !matches!(c.kind, CoverKind::Boundary | CoverKind::Tree))
        .map(|c| (DVec2::new(c.x, c.z), DVec2::new(c.w / 2.0, c.d / 2.0)))
        .collect()
}

/// The baked track channels over the arena square, `GROUND_PLAN_SIZE` texels a
/// side, rows from the most negative z.
pub struct TrackMap {
    pub tread: Vec<f64>,
    pub fresh: Vec<f64>,
    pub traffic: Vec<f64>,
    pub dryness: Vec<f64>,
}

/// World metres of a texel centre along either axis.
fn texel_centre(i: usize) -> f64 {
    ((i as f64 + 0.5) / GROUND_PLAN_SIZE as f64 * 2.0 - 1.0) * ARENA_PLAN_HALF
}

/// Signed distance outside an axis-aligned box (negative inside).
fn box_distance(p: DVec2, centre: DVec2, half: DVec2) -> f64 {
    let q = (p - centre).abs() - half;
    q.max(DVec2::ZERO).length() + q.x.max(q.y).min(0.0)
}

/// Rasterise every tread, the traffic they pack and the puddles in them.
pub fn bake_tracks() -> TrackMap {
    let size = GROUND_PLAN_SIZE;
    let obstacles = obstacles();
    let clearance = |p: DVec2| {
        obstacles
            .iter()
            .map(|&(centre, half)| box_distance(p, centre, half))
            .fold(f64::INFINITY, f64::min)
    };
    let mut treads = Vec::new();
    for path in road_paths().into_iter().chain(turning_paths()) {
        treads.extend(path.treads(TRACK_GAUGE));
    }
    for path in shortcut_paths() {
        treads.extend(path.treads(TRACK_GAUGE * 0.9));
    }
    treads.extend(pivot_treads());
    for tread in &mut treads {
        for (p, fresh) in tread.points.iter().zip(&mut tread.fresh) {
            *fresh *= smoothstep(clearance(*p), 0.6, 3.0);
        }
    }

    let mut map = TrackMap {
        tread: vec![0.0; size * size],
        fresh: vec![0.0; size * size],
        traffic: vec![0.0; size * size],
        dryness: vec![1.0; size * size],
    };
    let to_texel = |v: f64| (v / ARENA_PLAN_HALF * 0.5 + 0.5) * size as f64 - 0.5;
    let texel_range = |low: f64, high: f64| {
        let first = to_texel(low).floor().max(0.0) as usize;
        let last = (to_texel(high).ceil().max(0.0) as usize).min(size - 1);
        first..=last
    };
    // Treads: the nearest live tread's proximity and freshness.
    for tread in &treads {
        for (n, pair) in tread.points.windows(2).enumerate() {
            let fresh = (tread.fresh[n] + tread.fresh[n + 1]) / 2.0;
            if fresh < 0.02 {
                continue;
            }
            let (a, b) = (pair[0], pair[1]);
            let ab = b - a;
            let low = a.min(b) - DVec2::splat(TREAD_REACH);
            let high = a.max(b) + DVec2::splat(TREAD_REACH);
            for row in texel_range(low.y, high.y) {
                for column in texel_range(low.x, high.x) {
                    let p = DVec2::new(texel_centre(column), texel_centre(row));
                    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                    let proximity = 1.0 - (p - a - ab * t).length() / TREAD_REACH;
                    let at = row * size + column;
                    // A fresher tread wins where marks overlap; a faded one
                    // still shows where nothing fresher crosses it.
                    if proximity * (0.5 + fresh) > map.tread[at] * (0.5 + map.fresh[at]) {
                        map.tread[at] = proximity;
                        map.fresh[at] = fresh;
                    }
                }
            }
        }
    }
    // Traffic: each tread packs the ground around it, on a coarse grid.
    let cells = (ARENA_PLAN_HALF * 2.0 / TRAFFIC_CELL) as usize;
    let mut packed = vec![0.0; cells * cells];
    let reach = (TRAFFIC_SPREAD * 3.0 / TRAFFIC_CELL).ceil() as i64;
    for tread in &treads {
        for (p, fresh) in tread.points.iter().zip(&tread.fresh).step_by(2) {
            let cx = ((p.x + ARENA_PLAN_HALF) / TRAFFIC_CELL).floor() as i64;
            let cz = ((p.y + ARENA_PLAN_HALF) / TRAFFIC_CELL).floor() as i64;
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let (x, z) = (cx + dx, cz + dz);
                    if x < 0 || z < 0 || x >= cells as i64 || z >= cells as i64 {
                        continue;
                    }
                    let centre = DVec2::new(
                        (x as f64 + 0.5) * TRAFFIC_CELL - ARENA_PLAN_HALF,
                        (z as f64 + 0.5) * TRAFFIC_CELL - ARENA_PLAN_HALF,
                    );
                    let d2 = (centre - *p).length_squared();
                    packed[z as usize * cells + x as usize] += (0.3 + fresh)
                        * (-d2 / (2.0 * TRAFFIC_SPREAD * TRAFFIC_SPREAD)).exp()
                        * LINE_STEP
                        * 2.0;
                }
            }
        }
    }
    for row in 0..size {
        for column in 0..size {
            let grid = |v: f64| {
                ((v + ARENA_PLAN_HALF) / TRAFFIC_CELL - 0.5).clamp(0.0, cells as f64 - 1.0)
            };
            let (gx, gz) = (grid(texel_centre(column)), grid(texel_centre(row)));
            let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
            let (x1, z1) = ((x0 + 1).min(cells - 1), (z0 + 1).min(cells - 1));
            let (u, v) = (gx - x0 as f64, gz - z0 as f64);
            let at = |x: usize, z: usize| packed[z * cells + x];
            let value = (at(x0, z0) * (1.0 - u) + at(x1, z0) * u) * (1.0 - v)
                + (at(x0, z1) * (1.0 - u) + at(x1, z1) * u) * v;
            map.traffic[row * size + column] = 1.0 - (-value * 0.05).exp();
        }
    }
    // Puddles in the deepest stretches of fresh rut, and wider ones in the
    // potholes of the busiest junctions: centre, direction, half length and
    // half width.
    let mut puddles: Vec<(DVec2, DVec2, f64, f64)> = Vec::new();
    for (k, tread) in treads.iter().enumerate() {
        let mut n = 12;
        while n + 12 < tread.points.len() {
            let seed = (k * 977 + n) as i64;
            if tread.fresh[n] > 0.65 && hash(seed, 0, 401) < 0.016 {
                let direction = (tread.points[n + 1] - tread.points[n - 1]).normalize_or(DVec2::X);
                let length = 0.9 + hash(seed, 1, 401) * 2.2;
                let width = 0.32 + hash(seed, 2, 401) * 0.18;
                puddles.push((tread.points[n], direction, length, width));
                n += 40;
            }
            n += 6;
        }
    }
    for (x, z, radius) in [(2.5, -3.0, 1.4), (-49.5, -36.0, 1.0), (51.0, 2.0, 1.2)] {
        puddles.push((DVec2::new(x, z), DVec2::new(0.8, 0.6), radius, radius * 0.7));
    }
    for (centre, direction, length, width) in puddles {
        if clearance(centre) < 1.5 {
            continue;
        }
        let side = DVec2::new(-direction.y, direction.x);
        let span = length.max(width) + 0.5;
        for row in texel_range(centre.y - span, centre.y + span) {
            for column in texel_range(centre.x - span, centre.x + span) {
                let p = DVec2::new(texel_centre(column), texel_centre(row)) - centre;
                let u = p.dot(direction) / length;
                let v = p.dot(side) / width;
                let ragged = (noise(p.x * 3.0, p.y * 3.0, 409) - 0.5) * 0.35;
                let wet = 1.0 - smoothstep((u * u + v * v).sqrt() + ragged, 0.75, 1.0);
                let at = row * size + column;
                map.dryness[at] = map.dryness[at].min(1.0 - wet * 0.6);
            }
        }
    }
    map
}
