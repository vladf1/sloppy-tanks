//! Pine Village's painted ground plan: where the grass wears through to dirt, how
//! the yards are kept and how the farmland outside the wall is laid out.
//!
//! The village ground shader ([`super::ground_surfaces::village_ground_material`])
//! samples one baked plan, [`VILLAGE_GROUND_TEXTURE`], and frays its edges at
//! pixel scale. The plan is [`PLAN_SQUARES`] squares side by side, each
//! [`GROUND_PLAN_SIZE`] pixels, row 0 at the most negative z (uploaded without the
//! flip):
//!
//! - first, the arena out to ±[`ARENA_PLAN_HALF`] m: r worn dirt (roads with
//!   corners worn anywhere from nearly square to cut far back, edges that bulge
//!   into broad bites and pinch, grass surviving in the quiet middle of a road,
//!   door paths and the drip line round each cottage) ramping over
//!   [`EDGE_RAMP`] m either side of the edge, g trampled grass, b garden beds (0.5
//!   a vegetable plot, 1 a flower bed), a open ground (1, lower in the contact
//!   shade at the foot of walls, cottages, timber, trees and drums);
//! - second, the valley out to ±[`VALLEY_PLAN_HALF`] m: r farm lanes, the
//!   farmyard and the creek's sandy banks, g crop fields, b each field's crop and
//!   row direction ([`field_code`]), a dryness (0 at the water's edge);
//! - third, the arena's tracks ([`super::village_tracks`]): treads, their
//!   freshness, traffic and standing water.
//!
//! Alpha stays high over most of the plan, so nothing leans on colour kept under
//! transparent pixels.
//!
//! `scripts/generate-village-ground.ts` bakes it through the `village_ground`
//! example. Vegetation and the background forest ask the same functions where the
//! dirt and the fields are, so tufts line the painted edges and no tree grows in a
//! field or a lane.

use std::sync::OnceLock;

use glam::DVec2;

use super::house_model::{cottage_turns, door_direction};
use super::village_landscape::creek_distance;
use super::village_roads::village_roads;
use super::village_tracks::bake_tracks;
use crate::geometry::math::smoothstep;
use crate::sim::arena::{CoverDef, PICKUP_LAYOUT, arena_layout};
use crate::sim::types::CoverKind;

/// The baked plan the village ground samples.
pub const VILLAGE_GROUND_TEXTURE: &str = "textures/ground/village-ground.webp";
/// Pixels along each side of each square of the plan.
pub const GROUND_PLAN_SIZE: usize = 512;
/// Squares side by side in the plan: arena, valley, arena tracks.
pub const PLAN_SQUARES: usize = 3;
/// Half size of the arena half's square in metres (0.25 m pixels).
pub const ARENA_PLAN_HALF: f64 = 64.0;
/// Half size of the valley half's square in metres.
pub const VALLEY_PLAN_HALF: f64 = 170.0;

/// How far road corners round into the lawns, in metres of smooth union: the
/// tightest corners and the most cut-away ones.
const CORNER_ROUNDING: [f64; 2] = [3.0, 16.0];
/// Metres either side of a road's edge over which the plan's dirt ramps.
const EDGE_RAMP: f64 = 2.4;
/// Extra rounding of the trampled grass where tanks cut the corners.
const CORNER_TRAMPLE: f64 = 6.0;

pub(super) fn hash(ix: i64, iz: i64, seed: u64) -> f64 {
    let mut h = (ix as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add((iz as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f))
        .wrapping_add(seed.wrapping_mul(0x1656_67b1_9e37_79f9));
    h ^= h >> 31;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 29;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Smooth value noise in [0, 1].
pub(super) fn noise(x: f64, z: f64, seed: u64) -> f64 {
    let (ix, iz) = (x.floor(), z.floor());
    let (fx, fz) = (x - ix, z - iz);
    let (ux, uz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let (ix, iz) = (ix as i64, iz as i64);
    let a = hash(ix, iz, seed);
    let b = hash(ix + 1, iz, seed);
    let c = hash(ix, iz + 1, seed);
    let d = hash(ix + 1, iz + 1, seed);
    let top = a + (b - a) * ux;
    let bottom = c + (d - c) * ux;
    top + (bottom - top) * uz
}

fn fbm(x: f64, z: f64, seed: u64) -> f64 {
    noise(x, z, seed) * 0.55
        + noise(x * 2.03 + 17.3, z * 2.03 - 4.1, seed + 1) * 0.3
        + noise(x * 4.11 - 9.1, z * 4.11 + 3.7, seed + 2) * 0.15
}

/// Polynomial smooth maximum: `max(a, b)` with the crease rounded over `k`.
fn smooth_max(a: f64, b: f64, k: f64) -> f64 {
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.max(b) + h * h * k * 0.25
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (p - a - ab * t).length()
}

pub(super) fn polyline_distance(p: DVec2, points: &[[f64; 2]]) -> f64 {
    points
        .windows(2)
        .map(|pair| segment_distance(p, DVec2::from(pair[0]), DVec2::from(pair[1])))
        .fold(f64::INFINITY, f64::min)
}

/// Signed distance outside an axis-aligned box (negative inside).
fn box_distance(p: DVec2, centre: DVec2, half: DVec2) -> f64 {
    let q = (p - centre).abs() - half;
    q.max(DVec2::ZERO).length() + q.x.max(q.y).min(0.0)
}

/// The two road families' signed insides at (x, z): north-south strips, then
/// east-west ones.
fn road_families(x: f64, z: f64) -> (f64, f64) {
    let mut along = f64::NEG_INFINITY;
    let mut across = f64::NEG_INFINITY;
    for (i, road) in village_roads().iter().enumerate() {
        let inside = (road.w / 2.0 - (x - road.x).abs()).min(road.d / 2.0 - (z - road.z).abs());
        if i < 3 {
            along = along.max(inside);
        } else {
            across = across.max(inside);
        }
    }
    (along, across)
}

/// Slow wander of the worn edges, in metres: the roads bulge into the lawns in
/// some stretches and pinch in others instead of keeping one width, and in a
/// few places swell into broad bites where traffic spilled onto the grass.
fn edge_wander(x: f64, z: f64) -> f64 {
    let swell = 1.0 + 1.7 * smoothstep(noise(x * 0.027 + 4.0, z * 0.027 - 2.0, 14), 0.52, 0.86);
    ((fbm(x * 0.05, z * 0.05, 11) - 0.45) * 3.4 + (noise(x * 0.023, z * 0.023, 13) - 0.4) * 2.2)
        * swell
        + (noise(x * 0.23, z * 0.23, 12) - 0.5) * 0.7
}

/// How round the junction corner near (x, z) is worn: some kept nearly square,
/// others cut far back across the lawn.
fn corner_rounding(x: f64, z: f64) -> f64 {
    let [tight, cut] = CORNER_ROUNDING;
    let pick = smoothstep(noise(x * 0.021 + 3.1, z * 0.021 - 1.7, 15), 0.2, 0.85);
    tight + (cut - tight) * pick
}

/// Signed metres into the worn road dirt at (x, z): the road strips joined with
/// rounded inner corners, their edges wandering. Positive on the dirt.
pub fn road_wear(x: f64, z: f64) -> f64 {
    let (along, across) = road_families(x, z);
    smooth_max(along, across, corner_rounding(x, z)) + edge_wander(x, z)
}

/// Worn tracks across the lawns, where drivers cut through to the pickups and the
/// cottage gardens: each a wandering polyline in world metres and its width.
pub(super) const SHORTCUTS: [(&[[f64; 2]], f64); 8] = [
    (
        &[
            [-9.5, -7.0],
            [-13.5, -12.5],
            [-16.0, -18.0],
            [-20.0, -23.5],
            [-23.0, -27.0],
        ],
        1.3,
    ),
    (
        &[
            [27.0, -33.5],
            [33.0, -28.0],
            [38.0, -22.0],
            [42.0, -15.0],
            [46.5, -9.0],
        ],
        1.2,
    ),
    (
        &[
            [-9.5, 9.0],
            [-16.0, 14.5],
            [-26.0, 18.0],
            [-38.0, 22.0],
            [-46.0, 25.0],
        ],
        1.1,
    ),
    (
        &[
            [10.0, 7.0],
            [13.5, 12.5],
            [16.0, 18.0],
            [20.0, 23.0],
            [23.5, 27.0],
        ],
        1.25,
    ),
    (
        &[
            [-9.5, -50.5],
            [-18.0, -48.5],
            [-29.0, -46.0],
            [-39.0, -46.5],
            [-46.5, -48.0],
        ],
        1.0,
    ),
    (
        &[[9.5, 47.5], [19.0, 47.0], [29.0, 46.0], [37.0, 47.5]],
        1.0,
    ),
    (&[[-46.5, -30.0], [-42.0, -25.0], [-38.0, -22.0]], 0.9),
    (
        &[
            [46.5, 31.0],
            [42.0, 26.0],
            [38.0, 22.0],
            [33.0, 15.0],
            [27.0, 9.0],
        ],
        1.0,
    ),
];

/// A cottage's footprint, door and gardens in world space.
struct Cottage {
    centre: DVec2,
    half: DVec2,
    /// Unit direction the door faces.
    front: DVec2,
    seed: u64,
}

impl Cottage {
    fn new(index: usize, c: &CoverDef) -> Self {
        Self {
            centre: DVec2::new(c.x, c.z),
            half: DVec2::new(c.w / 2.0, c.d / 2.0),
            front: door_direction(cottage_turns(c.x, c.z)),
            seed: index as u64 * 7 + 3,
        }
    }

    /// The cottage's half extent along a horizontal unit axis (x or z).
    fn reach(&self, axis: DVec2) -> f64 {
        if axis.x.abs() > 0.5 {
            self.half.x
        } else {
            self.half.y
        }
    }

    fn side(&self) -> DVec2 {
        DVec2::new(self.front.y, -self.front.x)
    }

    fn door(&self) -> DVec2 {
        self.centre + self.front * self.reach(self.front)
    }

    /// Local coordinates: along the front wall, and out from the wall's face.
    fn local(&self, p: DVec2) -> (f64, f64) {
        let d = p - self.centre;
        (
            d.dot(self.side()),
            d.dot(self.front) - self.reach(self.front),
        )
    }

    /// The door path, from the step out to where it meets the road dirt (at
    /// most 9 m).
    fn path(&self) -> (DVec2, DVec2) {
        let door = self.door();
        let mut length = 0.8;
        while length < 9.0 && {
            let at = door + self.front * length;
            road_wear(at.x, at.y) < 0.6
        } {
            length += 0.25;
        }
        (door, door + self.front * length)
    }

    /// The vegetable plot beside one side wall, if the lawn there is clear of the
    /// roads: centre and half extents.
    fn plot(&self) -> Option<(DVec2, DVec2)> {
        if self.seed % 3 == 1 {
            return None;
        }
        let side = self.side()
            * if self.seed.is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
        let centre = self.centre + side * (self.reach(side) + 3.0) - self.front * 0.6;
        let half = if side.x.abs() > 0.5 {
            DVec2::new(1.6, 2.1)
        } else {
            DVec2::new(2.1, 1.6)
        };
        let clear = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
            .iter()
            .all(|(sx, sz)| {
                road_wear(
                    centre.x + sx * (half.x + 1.0),
                    centre.y + sz * (half.y + 1.0),
                ) < -0.8
            });
        clear.then_some((centre, half))
    }
}

/// The arena's covers the plan reads, gathered once per bake.
struct ArenaPlan {
    cottages: Vec<Cottage>,
    plots: Vec<(DVec2, DVec2)>,
    paths: Vec<(DVec2, DVec2)>,
    /// Footprints that shade the ground at their foot: centre, half size, reach
    /// of the shade and its strength.
    shades: Vec<(DVec2, DVec2, f64, f64)>,
    trees: Vec<DVec2>,
}

impl ArenaPlan {
    fn new() -> Self {
        let layout = arena_layout();
        let cottages: Vec<Cottage> = layout
            .iter()
            .filter(|c| c.kind == CoverKind::House)
            .enumerate()
            .map(|(i, c)| Cottage::new(i, c))
            .collect();
        let plots = cottages.iter().filter_map(Cottage::plot).collect();
        let paths = cottages.iter().map(Cottage::path).collect();
        let mut shades = Vec::new();
        let mut trees = Vec::new();
        for c in &layout {
            let centre = DVec2::new(c.x, c.z);
            let half = DVec2::new(c.w / 2.0, c.d / 2.0);
            match c.kind {
                CoverKind::Tree => trees.push(centre),
                CoverKind::House => shades.push((centre, half, 1.1, 0.55)),
                CoverKind::Timber => shades.push((centre, half, 0.7, 0.5)),
                CoverKind::Boundary => shades.push((centre, half, 1.4, 0.45)),
                CoverKind::Drum => shades.push((centre, half * 0.45, 0.55, 0.45)),
                CoverKind::Tower => shades.push((centre, half * 0.8, 1.6, 0.3)),
                _ => {}
            }
        }
        Self {
            cottages,
            plots,
            paths,
            shades,
            trees,
        }
    }

    /// (dirt, trampled grass, garden, open ground) at (x, z), given how much
    /// traffic packed the ground there.
    fn sample(&self, x: f64, z: f64, traffic: f64) -> [f64; 4] {
        let p = DVec2::new(x, z);
        let (along, across) = road_families(x, z);
        let wander = edge_wander(x, z);
        let rounding = corner_rounding(x, z);
        let road = smooth_max(along, across, rounding) + wander;
        // A wide ramp across the edge: the shader cuts it crisply in some
        // stretches and lets it crumble over a few metres in others.
        let mut dirt = smoothstep(road, -EDGE_RAMP, EDGE_RAMP);
        // Grass survives in the quiet middle of a road, between and beside the
        // wheel lines, in ragged islands and medians.
        let quiet = 1.0 - smoothstep(traffic, 0.1, 0.34);
        let islands = smoothstep(fbm(x * 0.11 + 7.0, z * 0.11 - 3.0, 24), 0.42, 0.62);
        let survives = quiet * islands * smoothstep(road, 1.2, 2.8);
        dirt *= 1.0 - survives * 0.65;
        // Trampled grass: wide where tanks cut the corners, a soft verge elsewhere.
        let corner = smooth_max(along, across, rounding + CORNER_TRAMPLE) + wander;
        let patchy = 0.55 + 0.45 * fbm(x * 0.09, z * 0.09, 21);
        // Grass surviving in a road is sparse and beaten flat.
        let mut trampled = (smoothstep(corner, -4.5, 0.2) * patchy).max(survives);

        for (points, width) in SHORTCUTS {
            let distance =
                polyline_distance(p, points) + (noise(x * 0.35, z * 0.35, 31) - 0.5) * 0.5;
            // The treads cut the turf (`village_tracks`); the grass between and
            // beside them is flattened and scuffed.
            let scuff = 1.0 - smoothstep(distance, width * 0.3, width * 1.1);
            dirt = dirt.max(scuff * 0.38 * noise(x * 0.5, z * 0.5, 32));
            trampled = trampled.max(1.0 - smoothstep(distance, width * 1.2, width * 2.4));
        }

        let mut garden: f64 = 0.0;
        for cottage in &self.cottages {
            let outside = box_distance(p, cottage.centre, cottage.half);
            if outside > 12.0 {
                continue;
            }
            // Bare, rain-splashed soil along the drip line under the eaves.
            let ragged = (noise(x * 0.9, z * 0.9, cottage.seed) - 0.5) * 0.3;
            let drip = 1.0 - smoothstep(outside + ragged, 0.1, 0.65);
            dirt = dirt.max(drip * 0.75);
            let (u, v) = cottage.local(p);
            let front_half = cottage.reach(cottage.side());
            // Flower beds along the front wall either side of the steps.
            if (0.0..0.8).contains(&v) && u.abs() > 1.0 && u.abs() < front_half - 0.1 {
                garden = 1.0;
            }
            // A bed along one side wall on some cottages.
            if cottage.seed.is_multiple_of(4) {
                let s = (p - cottage.centre).dot(cottage.side()) - front_half;
                let along = (p - cottage.centre).dot(cottage.front);
                if (0.0..0.7).contains(&s) && along.abs() < 2.2 {
                    garden = 1.0;
                }
            }
        }
        for &(door, end) in &self.paths {
            let distance = segment_distance(p, door, end);
            let gravel = 1.0 - smoothstep(distance, 0.45, 0.7);
            dirt = dirt.max(gravel);
            trampled = trampled.max(1.0 - smoothstep(distance, 0.7, 1.4));
            garden = garden.min(1.0 - gravel);
        }
        for &(centre, half) in &self.plots {
            let outside = box_distance(p, centre, half);
            if outside < 0.0 {
                garden = garden.max(0.5);
            }
            trampled = trampled.max(1.0 - smoothstep(outside, 0.0, 0.9));
        }
        let mut shade: f64 = 0.0;
        for &(centre, half, reach, strength) in &self.shades {
            let outside = box_distance(p, centre, half);
            if outside < reach {
                shade = shade.max((1.0 - smoothstep(outside, 0.0, reach)) * strength);
            }
        }
        for &tree in &self.trees {
            let distance = (p - tree).length();
            // Needle litter and a soft contact shade round each trunk.
            trampled = trampled.max((1.0 - smoothstep(distance, 1.2, 3.2)) * 0.7);
            shade = shade.max((1.0 - smoothstep(distance, 0.6, 2.4)) * 0.5);
        }
        for pickup in PICKUP_LAYOUT {
            let distance = (p - DVec2::new(pickup.x, pickup.z)).length();
            trampled = trampled.max((1.0 - smoothstep(distance, 1.6, 3.2)) * 0.85);
        }
        [dirt, trampled.min(1.0), garden, 1.0 - shade]
    }
}

/// Whether the turf stands whole around (x, z): inside a lawn, clear of its worn
/// edge, the gardens, trampled patches and the treads cut across it, so a tuft
/// planted there can stand on the raised turf the ground shader draws.
pub fn deep_turf(x: f64, z: f64) -> bool {
    static PLAN: OnceLock<ArenaPlan> = OnceLock::new();
    let plan = PLAN.get_or_init(ArenaPlan::new);
    let p = DVec2::new(x, z);
    if SHORTCUTS
        .iter()
        .any(|(points, width)| polyline_distance(p, points) < width + 2.2)
    {
        return false;
    }
    [(0.0, 0.0), (0.6, 0.0), (-0.6, 0.0), (0.0, 0.6), (0.0, -0.6)]
        .iter()
        .all(|(dx, dz)| {
            let [dirt, trampled, garden, _] = plan.sample(x + dx, z + dz, 0.0);
            dirt < 0.01 && trampled < 0.25 && garden < 0.05
        })
}

/// The cottages' vegetable plots: centre and half extents in world metres. The
/// ground paints their soil and rows; the vegetation grows heads along them.
pub fn vegetable_plots() -> Vec<(DVec2, DVec2)> {
    ArenaPlan::new().plots
}

/// Metres between the vegetable rows the ground paints (`veg_row` in
/// `meadow_sway.wgsl`, `sin(x * 11.4)`); the rows run north-south.
pub const VEGETABLE_ROW: f64 = std::f64::consts::TAU / 11.4;

/// A crop field outside the wall: centre, full size, row direction (radians from
/// +x) and crop (0 ripe wheat, 1 young rows, 2 ploughed, 3 mown hay).
struct Field {
    centre: [f64; 2],
    size: [f64; 2],
    angle: f64,
    crop: u32,
}

const fn field(x: f64, z: f64, w: f64, d: f64, angle: f64, crop: u32) -> Field {
    Field {
        centre: [x, z],
        size: [w, d],
        angle,
        crop,
    }
}

const FIELDS: [Field; 15] = [
    // East of the village, around the farmstead on the lane.
    field(86.0, -51.0, 24.0, 32.0, 0.12, 0),
    field(119.0, -52.0, 30.0, 34.0, 0.12, 2),
    field(86.0, 12.0, 26.0, 28.0, -0.06, 1),
    field(119.0, 14.0, 28.0, 32.0, -0.06, 3),
    field(90.0, 48.0, 28.0, 26.0, 0.18, 0),
    field(124.0, 50.0, 30.0, 26.0, 0.18, 1),
    // The strip between the creek and the mountains.
    field(42.0, -101.0, 34.0, 12.0, 0.03, 0),
    field(-20.0, -102.0, 30.0, 11.0, -0.02, 2),
    field(-104.0, -98.0, 28.0, 18.0, 0.35, 1),
    // West across the creek.
    field(-120.0, -30.0, 26.0, 34.0, 0.08, 3),
    field(-121.0, 10.0, 26.0, 30.0, 0.08, 0),
    field(-130.0, 76.0, 24.0, 30.0, -0.2, 2),
    // South beyond the treeline.
    field(-40.0, 96.0, 34.0, 22.0, 0.05, 0),
    field(34.0, 101.0, 30.0, 24.0, -0.08, 1),
    field(84.0, 94.0, 24.0, 26.0, 0.1, 3),
];

/// Farm lanes: wandering polylines in world metres.
const LANES: [&[[f64; 2]]; 6] = [
    // From the north road over the footbridge and along the fields.
    &[
        [0.0, -61.0],
        [0.0, -73.0],
        [0.0, -91.0],
        [3.0, -97.0],
        [22.0, -94.5],
        [60.0, -93.5],
        [90.0, -98.0],
    ],
    // To the mill's yard.
    &[[0.0, -64.5], [-12.0, -66.0], [-26.0, -66.5]],
    // The east perimeter track and the lane out to the farmstead.
    &[
        [64.0, -72.0],
        [66.5, -40.0],
        [66.0, 0.0],
        [66.5, 40.0],
        [64.0, 72.0],
    ],
    &[
        [66.0, -20.0],
        [84.0, -22.0],
        [104.0, -21.0],
        [128.0, -24.0],
        [160.0, -20.0],
    ],
    // West over the second footbridge.
    &[
        [-63.0, 38.0],
        [-76.0, 38.0],
        [-94.0, 38.0],
        [-104.0, 40.0],
        [-120.0, 47.0],
        [-150.0, 50.0],
    ],
    // South to the far fields.
    &[
        [0.0, 63.0],
        [2.0, 80.0],
        [-4.0, 96.0],
        [-1.0, 118.0],
        [4.0, 160.0],
    ],
];

/// The farmstead east of the village: its yard centre and radius.
pub const FARMYARD: ([f64; 2], f64) = ([104.0, -12.0], 11.0);

/// Where the north wheat field's sheaves stand in stooks: centre, size and row
/// angle of the seventh field.
pub fn north_wheat() -> ([f64; 2], [f64; 2], f64) {
    let field = &FIELDS[6];
    (field.centre, field.size, field.angle)
}

/// The hay field's windrows, where the farmstead's round bales lie: the field's
/// centre, size and row angle (the fourth field).
pub fn hay_field() -> ([f64; 2], [f64; 2], f64) {
    let field = &FIELDS[3];
    (field.centre, field.size, field.angle)
}

/// Rail fences beside the farm lanes and round the fields by the farmstead.
pub const FARM_FENCES: [&[[f64; 2]]; 5] = [
    &[[8.0, -92.2], [22.0, -91.4], [40.0, -91.0], [58.0, -91.2]],
    &[[70.0, -24.0], [84.0, -24.3], [100.0, -23.6]],
    &[[70.0, -18.5], [84.0, -19.5], [96.0, -18.8]],
    &[
        [104.5, -3.0],
        [104.0, 30.0],
        [133.0, 28.5],
        [133.5, -1.5],
        [124.0, -1.0],
    ],
    &[[73.0, -34.0], [73.5, -67.0], [99.0, -67.5], [99.5, -35.0]],
];

/// The b channel of a field: its crop in the top two bits, its row direction in
/// eighths of a half turn below, at the middle of its bin.
fn field_code(field: &Field) -> f64 {
    let turn = field.angle.rem_euclid(std::f64::consts::PI) / std::f64::consts::PI;
    let bin = ((turn * 8.0).round() as u32) % 8;
    f64::from(field.crop * 64 + bin * 8 + 4) / 255.0
}

/// Field mask at `p` (1 inside the planted area) and the field's code.
fn field_at(p: DVec2) -> (f64, f64) {
    let mut best = (0.0, 0.0);
    for field in &FIELDS {
        let (sin, cos) = field.angle.sin_cos();
        let d = p - DVec2::from(field.centre);
        let local = DVec2::new(d.x * cos + d.y * sin, -d.x * sin + d.y * cos);
        let half = DVec2::from(field.size) / 2.0;
        let outside = box_distance(local, DVec2::ZERO, half);
        if outside > 1.0 {
            continue;
        }
        let wobble = (noise(p.x * 0.3, p.y * 0.3, 41) - 0.5) * 0.6;
        let planted = 1.0 - smoothstep(outside + wobble, -1.2, -0.2);
        if planted > best.0 {
            best = (planted, field_code(field));
        }
    }
    best
}

/// Whether trees and meadow tufts should leave (x, z) open: a field, a lane or the
/// farmyard.
pub fn farmed(x: f64, z: f64) -> bool {
    let p = DVec2::new(x, z);
    let (centre, radius) = FARMYARD;
    field_at(p).0 > 0.0
        || (p - DVec2::from(centre)).length() < radius + 3.0
        || LANES.iter().any(|lane| polyline_distance(p, lane) < 3.5)
}

/// (lane dirt, field, field code, dryness) at (x, z) in the valley.
fn valley_sample(x: f64, z: f64) -> [f64; 4] {
    let p = DVec2::new(x, z);
    let mut dirt: f64 = 0.0;
    for lane in LANES {
        let distance = polyline_distance(p, lane) + (noise(x * 0.25, z * 0.25, 51) - 0.5) * 0.9;
        dirt = dirt.max(1.0 - smoothstep(distance, 1.0, 2.2));
    }
    let (centre, radius) = FARMYARD;
    let yard = (p - DVec2::from(centre)).length() + (fbm(x * 0.15, z * 0.15, 52) - 0.5) * 4.0;
    dirt = dirt.max(1.0 - smoothstep(yard, radius - 3.0, radius));
    let river = creek_distance(x, z);
    // Sandy, pebbly banks where the water rises and falls, wet at the edge.
    let ragged = (noise(x * 0.2, z * 0.2, 53) - 0.5) * 1.6;
    let bank = (1.0 - smoothstep(river + ragged, 6.0, 9.0)) * 0.85;
    dirt = dirt.max(bank);
    let dry = smoothstep(river, 5.2, 7.6);
    let (planted, code) = field_at(p);
    [dirt * (1.0 - planted), planted, code, dry]
}

/// `value` in [0, 1] rounded to `levels` steps.
fn quantize(value: f64, levels: f64) -> f64 {
    (value.clamp(0.0, 1.0) * levels).round() / levels
}

/// The baked plan: `GROUND_PLAN_SIZE * 3` wide, `GROUND_PLAN_SIZE` tall, RGBA8
/// rows from the most negative z: the arena, the valley and the arena's tracks
/// (`village_tracks.rs`).
pub fn bake_village_ground() -> Vec<u8> {
    let size = GROUND_PLAN_SIZE;
    let plan = ArenaPlan::new();
    let tracks = bake_tracks();
    let mut pixels = vec![0u8; size * PLAN_SQUARES * size * 4];
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    for row in 0..size {
        let t = (row as f64 + 0.5) / size as f64 * 2.0 - 1.0;
        for column in 0..size {
            let s = (column as f64 + 0.5) / size as f64 * 2.0 - 1.0;
            let at = row * size + column;
            let arena = plan.sample(s * ARENA_PLAN_HALF, t * ARENA_PLAN_HALF, tracks.traffic[at]);
            let valley = valley_sample(s * VALLEY_PLAN_HALF, t * VALLEY_PLAN_HALF);
            // Coarser steps than a byte where the shader needs no more keep the
            // lossless WebP small: 1.6 cm steps of tread distance, 16 freshnesses.
            let worn = [
                quantize(tracks.tread[at], 64.0),
                quantize(tracks.fresh[at], 16.0),
                quantize(tracks.traffic[at], 64.0),
                quantize(tracks.dryness[at], 32.0),
            ];
            for (square, values) in [(0, arena), (1, valley), (2, worn)] {
                let at = (row * size * PLAN_SQUARES + square * size + column) * 4;
                for (k, value) in values.into_iter().enumerate() {
                    pixels[at + k] = byte(value);
                }
            }
        }
    }
    pixels
}
