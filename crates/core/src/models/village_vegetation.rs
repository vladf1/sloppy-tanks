//! Port of `village-vegetation.ts`: low meadow tufts, reeds and flowers. Detail stays
//! below shell height; the shared tuft instances sway entirely on the GPU
//! ([`effects_scenery::MEADOW_SWAY`]).
//!
//! Inside the arena the grass is thickest where the ground shader frays the roads:
//! tufts line both road shoulders and the foot of the perimeter wall, so the dirt
//! meets real blades instead of a painted edge. Tufts keep off cover, pickups and
//! spawn pads.

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DVec2, DVec3};

use crate::geometry::math::{compose, hex_to_linear, quat_from_euler, quat_from_euler_yxz};
use crate::geometry::{Attribute, Mesh};
use crate::scene::{Effect, Instance, Material, Node, Side};
use crate::sim::arena::{PICKUP_LAYOUT, arena_layout, spawn_positions};
use crate::sim::data::ARENA;
use crate::sim::types::{CoverKind, Team};

use super::effects_scenery::{MEADOW_SWAY, WIND_ORIGIN};
use super::village_ground::{
    SHORTCUTS, VEGETABLE_ROW, farmed, polyline_distance, road_wear, vegetable_plots,
};
use super::village_landscape::{creek_at, creek_distance, valley_height_at};
use super::village_roads::village_roads;
use crate::sim::math::Random;

/// Instance capacities of the tuft and flower meshes.
pub const MEADOW_TUFTS: usize = 12000;
pub const MEADOW_FLOWERS: usize = 3200;
/// Candidate placements tried in the open meadow before the tuft capacity fills.
const MEADOW_CANDIDATES: usize = 5200;
/// Spacing of the tuft candidates along road shoulders and the wall's foot.
const FRINGE_STEP: f64 = 0.42;
/// The TypeScript's full-turn approximation for tuft yaw (kept for identical output).
#[allow(clippy::approx_constant)]
const TUFT_TURN: f64 = 6.28;
/// Tuft tints, matched to the ground shader's grass greens.
const TUFT_COLORS: [u32; 4] = [0x5d9236, 0x7ea845, 0x98b04e, 0x4b7f32];
/// Tints of the tall tufts in the long-grass stands, matched to the ground
/// shader's long turf.
const LONG_TUFT_COLORS: [u32; 3] = [0x4c7a36, 0x5a873c, 0x6a8f44];
/// Spacing of the long-grass candidates across the arena lawns.
const STAND_STEP: f64 = 0.9;
/// Meadow flowers: daisy, buttercup, harebell and campion.
const FLOWER_COLORS: [u32; 4] = [0xf4f1e4, 0xf2c94a, 0xa58be0, 0xe99ab4];
/// The meadow's instances draw in cells, so a view draws only the tufts it can
/// see: the arena split this many ways along each side, and square cells this
/// many metres across out in the valley.
const ARENA_CELLS: f64 = 3.0;
const VALLEY_CELL: f64 = 120.0;
/// Greens in the cottage plots, a crop to a row: lettuce, cabbage and kale.
const CROP_COLORS: [u32; 3] = [0x8fbf4a, 0x6f9c5c, 0x4e7a3c];
/// Metres between heads along a vegetable row.
const CROP_STEP: f64 = 0.34;
/// Most a tuft leans off upright, radians.
const TUFT_LEAN: f64 = 0.24;
/// Farthest a clump's tufts stand from its heart, metres.
const CLUMP_REACH: f64 = 0.42;

/// Seven tapering blades fanned around the tuft, darker at the root. Their normals
/// lean up like the ground's, so a tuft shades into the grass it grows from
/// instead of flickering blade by blade.
fn tuft_geometry() -> Mesh {
    const BLADES: usize = 7;
    let mut positions = Vec::with_capacity(BLADES * 9);
    let mut normals = Vec::with_capacity(BLADES * 9);
    let mut colors = Vec::with_capacity(BLADES * 3);
    for i in 0..BLADES {
        let a = i as f64 * 2.39996;
        let (x, z) = (a.cos(), a.sin());
        let reach = 0.2 + (i % 3) as f64 * 0.08;
        let height = 0.5 + ((i * 5) % 7) as f64 * 0.07;
        let root = 0.06 + (i % 2) as f64 * 0.02;
        // Roots stand slightly apart so the clump has a footprint.
        let (bx, bz) = (x * 0.05, z * 0.05);
        positions.extend([
            bx - z * root,
            0.0,
            bz + x * root,
            bx + z * root,
            0.0,
            bz - x * root,
            x * reach,
            height,
            z * reach,
        ]);
        for _ in 0..3 {
            let n = DVec3::new(x * 0.35, 1.0, z * 0.35).normalize();
            normals.extend([n.x, n.y, n.z]);
        }
        colors.extend([
            [0.62f32, 0.68, 0.52],
            [0.62, 0.68, 0.52],
            [1.15, 1.13, 0.96],
        ]);
    }
    let mut geometry = Mesh::from_f64(&positions, &normals, &[], None);
    geometry.colors = colors;
    geometry
}

/// A five-petal bloom facing up with a raised heart, about a unit across; the
/// instance colour tints the petals.
fn bloom_geometry() -> Mesh {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let heart = [1.0f32, 0.78, 0.2];
    for i in 0..5 {
        let a = f64::from(i) * PI * 0.4;
        let (s0, c0) = (a - 0.42).sin_cos();
        let (s1, c1) = (a + 0.42).sin_cos();
        let (s, c) = a.sin_cos();
        // A petal: two triangles from the heart out to a rounded tip.
        let inner = [0.0, 0.08, 0.0];
        let left = [c0 * 0.62, 0.04, s0 * 0.62];
        let tip = [c * 1.0, 0.0, s * 1.0];
        let right = [c1 * 0.62, 0.04, s1 * 0.62];
        for p in [inner, right, tip, inner, tip, left] {
            positions.extend(p);
            colors.push([1.0f32; 3]);
        }
        // The heart: a low pyramid wedge.
        let (h0s, h0c) = (a - 0.63).sin_cos();
        let (h1s, h1c) = (a + 0.63).sin_cos();
        for p in [
            [0.0, 0.2, 0.0],
            [h1c * 0.3, 0.1, h1s * 0.3],
            [h0c * 0.3, 0.1, h0s * 0.3],
        ] {
            positions.extend(p);
            colors.push(heart);
        }
    }
    let normals: Vec<f64> = (0..positions.len() / 3)
        .flat_map(|_| [0.0, 1.0, 0.0])
        .collect();
    let mut geometry = Mesh::from_f64(&positions, &normals, &[], None);
    geometry.colors = colors;
    geometry
}

/// A head of greens about two units across: five cupped leaves round a domed
/// heart, dark at the root and pale at the leaf tips.
fn crop_geometry() -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut push = |p: [f64; 3], shade: f32, n: DVec3| {
        positions.extend(p);
        let n = n.normalize();
        normals.extend([n.x, n.y, n.z]);
        colors.push([shade; 3]);
    };
    for i in 0..5 {
        let a = f64::from(i) * PI * 0.4;
        let (s0, c0) = (a - 0.55).sin_cos();
        let (s1, c1) = (a + 0.55).sin_cos();
        let (s, c) = a.sin_cos();
        let root = [c * 0.12, 0.05, s * 0.12];
        let left = [c0 * 0.62, 0.32, s0 * 0.62];
        let tip = [c, 0.42, s];
        let right = [c1 * 0.62, 0.32, s1 * 0.62];
        let n = DVec3::new(c * 0.5, 1.0, s * 0.5);
        for (p, shade) in [
            (root, 0.6),
            (right, 1.0),
            (tip, 1.12),
            (root, 0.6),
            (tip, 1.12),
            (left, 1.0),
        ] {
            push(p, shade, n);
        }
    }
    for i in 0..4 {
        let a0 = f64::from(i) * PI * 0.5;
        let a1 = a0 + PI * 0.5;
        let mid = a0 + PI * 0.25;
        let n = DVec3::new(mid.cos() * 0.6, 1.0, mid.sin() * 0.6);
        push([0.0, 0.55, 0.0], 1.18, n);
        push([a1.cos() * 0.38, 0.22, a1.sin() * 0.38], 0.95, n);
        push([a0.cos() * 0.38, 0.22, a0.sin() * 0.38], 0.95, n);
    }
    let mut geometry = Mesh::from_f64(&positions, &normals, &[], None);
    geometry.colors = colors;
    geometry
}

/// Heads of greens along the rows the ground paints in each cottage's
/// vegetable plot, a few missing where they were picked.
fn vegetable_beds() -> Vec<Instance> {
    let mut rng = Random::new(51973.0);
    let mut heads = Vec::new();
    let rate = std::f64::consts::TAU / VEGETABLE_ROW;
    for (centre, half) in vegetable_plots() {
        let (west, east) = (centre.x - half.x + 0.2, centre.x + half.x - 0.2);
        // The rows stand where the ground's `sin(x * rate)` peaks.
        let mut row = ((west * rate - PI / 2.0) / std::f64::consts::TAU).ceil();
        loop {
            let x = (PI / 2.0 + std::f64::consts::TAU * row) / rate;
            if x > east {
                break;
            }
            let crop = CROP_COLORS[row.rem_euclid(3.0) as usize];
            let mut z = centre.y - half.y + 0.24;
            while z < centre.y + half.y - 0.16 {
                if rng.next() < 0.9 {
                    let size = rng.range(0.15, 0.21);
                    let value = rng.range(0.88, 1.1) as f32;
                    heads.push(Instance {
                        matrix: compose(
                            DVec3::new(
                                x + rng.range(-0.03, 0.03),
                                0.01,
                                z + rng.range(-0.04, 0.04),
                            ),
                            quat_from_euler(0.0, rng.next() * TUFT_TURN, 0.0),
                            DVec3::new(size, size * rng.range(0.85, 1.15), size),
                        ),
                        color: instance_color(crop).map(|c| c.map(|v| v * value)),
                    });
                }
                z += CROP_STEP;
            }
            row += 1.0;
        }
    }
    heads
}

fn instance_color(hex: u32) -> Option<[f32; 3]> {
    Some(hex_to_linear(hex).map(|c| c as f32))
}

/// Where arena tufts must not grow: cover footprints (so blades never poke through
/// timber or house walls), pickup pads and spawn plinths.
struct Clearings {
    boxes: Vec<[f64; 4]>,
    discs: Vec<[f64; 3]>,
}

impl Clearings {
    fn new() -> Self {
        let boxes = arena_layout()
            .iter()
            .filter(|c| !matches!(c.kind, CoverKind::Boundary | CoverKind::Tree))
            .map(|c| [c.x, c.z, c.w / 2.0 + 0.15, c.d / 2.0 + 0.15])
            .collect();
        let mut discs: Vec<[f64; 3]> = PICKUP_LAYOUT.iter().map(|p| [p.x, p.z, 1.7]).collect();
        for team in [Team::Blue, Team::Red] {
            discs.extend(spawn_positions(team, 1.0).map(|s| [s.x, s.z, 3.4]));
        }
        Self { boxes, discs }
    }

    fn blocks(&self, x: f64, z: f64) -> bool {
        self.boxes
            .iter()
            .any(|[cx, cz, hw, hd]| (x - cx).abs() < *hw && (z - cz).abs() < *hd)
            || self
                .discs
                .iter()
                .any(|[cx, cz, r]| (x - cx).powi(2) + (z - cz).powi(2) < r * r)
    }
}

/// Tufts and flowers being placed, with the wind origin of every tuft.
struct Meadow {
    tufts: Vec<Instance>,
    origins: Vec<f32>,
    flowers: Vec<Instance>,
}

impl Meadow {
    /// One tuft: squat and spreading or tall and wispy, leaning a little, its
    /// green a shade lighter or darker than its neighbours'.
    fn tuft(&mut self, rng: &mut Random, x: f64, y: f64, z: f64, size: f64, bloom: Option<u32>) {
        if self.tufts.len() >= MEADOW_TUFTS {
            return;
        }
        let rotation = quat_from_euler_yxz(
            rng.range(-TUFT_LEAN, TUFT_LEAN),
            rng.next() * TUFT_TURN,
            rng.range(-TUFT_LEAN, TUFT_LEAN),
        );
        let tall = rng.range(0.78, 1.3);
        let spread = rng.range(0.82, 1.2) / tall.sqrt();
        let shade = TUFT_COLORS[(rng.next() * 4.0) as usize % 4];
        let value = rng.range(0.86, 1.1) as f32;
        self.tufts.push(Instance {
            matrix: compose(
                DVec3::new(x, y, z),
                rotation,
                DVec3::new(size * 0.85 * spread, size * tall, size * 0.85 * spread),
            ),
            color: instance_color(shade).map(|c| c.map(|v| v * value)),
        });
        self.origins.extend([x as f32, z as f32]);
        if let Some(color) = bloom
            && self.flowers.len() < MEADOW_FLOWERS
        {
            let bloom = rng.range(0.055, 0.09);
            let tilt = quat_from_euler(rng.range(-0.25, 0.25), rng.next() * TUFT_TURN, 0.0);
            self.flowers.push(Instance {
                matrix: compose(DVec3::new(x, y + size * 0.62, z), tilt, DVec3::splat(bloom)),
                color: instance_color(color),
            });
        }
    }

    /// A clump of tufts round (x, z), the biggest at its heart, each where
    /// `open` allows; `members` counts the heart.
    #[allow(clippy::too_many_arguments)]
    fn clump(
        &mut self,
        rng: &mut Random,
        x: f64,
        y: f64,
        z: f64,
        size: f64,
        members: usize,
        bloom: Option<u32>,
        open: &dyn Fn(f64, f64) -> bool,
    ) {
        for k in 0..members {
            let (tx, tz) = if k == 0 {
                (x, z)
            } else {
                let angle = rng.range(0.0, PI * 2.0);
                let reach = rng.range(0.4, 1.0) * CLUMP_REACH;
                (x + angle.cos() * reach, z + angle.sin() * reach)
            };
            if k > 0 && !open(tx, tz) {
                continue;
            }
            let falloff = 1.0 - (k as f64).min(3.0) * 0.14;
            self.tuft(rng, tx, y, tz, size * falloff, bloom.filter(|_| k == 0));
        }
    }
}

/// The cell a meadow instance at (x, z) draws with.
fn meadow_cell(x: f64, z: f64) -> (i32, i32, bool) {
    let reach = ARENA + 1.0;
    if x.abs() < reach && z.abs() < reach {
        let cell = |v: f64| ((v + reach) / (reach * 2.0) * ARENA_CELLS).floor() as i32;
        (cell(x), cell(z), true)
    } else {
        (
            (x / VALLEY_CELL).floor() as i32,
            (z / VALLEY_CELL).floor() as i32,
            false,
        )
    }
}

/// Where long grass stands, positive in the stands: the ground shader's
/// `vg_long_grass` (`meadow_sway.wgsl`), so the tall tufts grow where it paints
/// the long grass.
pub fn long_grass(x: f64, z: f64) -> f64 {
    (x * 0.083 + z * 0.041).sin() * (z * 0.067 - x * 0.029).cos() * 0.65
        + (x * 0.19 - z * 0.16 + 1.3).sin() * 0.35
}

/// Tall, darker tufts through the long-grass stands of the arena lawns, in
/// loose clumps that thin toward the stands' edges and keep off the worn
/// shortcuts tanks drive across the lawns.
fn long_stands(meadow: &mut Meadow, rng: &mut Random, clearings: &Clearings) {
    let reach = ARENA - 1.0;
    let candidates = ((2.0 * reach / STAND_STEP).powi(2)) as usize;
    for _ in 0..candidates {
        let (px, pz) = (rng.range(-reach, reach), rng.range(-reach, reach));
        let stand = long_grass(px, pz);
        if stand < 0.12 || rng.next() > (stand - 0.12) * 3.0 {
            continue;
        }
        if road_wear(px, pz) > -1.5 || clearings.blocks(px, pz) {
            continue;
        }
        let p = DVec2::new(px, pz);
        if SHORTCUTS
            .iter()
            .any(|(points, width)| polyline_distance(p, points) < width + 1.6)
        {
            continue;
        }
        let count = 1 + (rng.next() * 3.0) as usize;
        for _ in 0..count {
            if meadow.tufts.len() >= MEADOW_TUFTS {
                return;
            }
            let (tx, tz) = (px + rng.range(-0.35, 0.35), pz + rng.range(-0.35, 0.35));
            let size = rng.range(0.3, 0.62) * (0.75 + stand.min(0.6) * 0.5);
            let tint = LONG_TUFT_COLORS[(rng.next() * 3.0) as usize % 3];
            let value = rng.range(0.88, 1.08) as f32;
            meadow.tuft(rng, tx, 0.02, tz, size, None);
            if let Some(last) = meadow.tufts.last_mut() {
                last.color = instance_color(tint).map(|c| c.map(|v| v * value));
            }
        }
    }
}

/// Grass lining both shoulders of every road and the foot of the perimeter wall,
/// thickest at the edge and thinning into the meadow, in clumps.
fn fringe(meadow: &mut Meadow, rng: &mut Random, clearings: &Clearings) {
    let reach = ARENA - 1.0;
    let plant = |meadow: &mut Meadow, rng: &mut Random, x: f64, z: f64| {
        if x.abs() > ARENA - 0.25 || z.abs() > ARENA - 0.25 {
            return;
        }
        // Clumps: a slow pattern decides where the verge grows thick.
        if (x * 0.83 + z * 0.31).sin() * (z * 0.67 - x * 0.22).cos() * 0.6 + rng.next() < 0.32 {
            return;
        }
        // Hug the worn edge where it wanders, thinning a metre or so into the
        // lawn.
        let wear = road_wear(x, z);
        if !(-1.6..0.35).contains(&wear) || rng.next() < (-wear - 0.4).max(0.0) * 0.6 {
            return;
        }
        if clearings.blocks(x, z) {
            return;
        }
        let size = rng.range(0.2, 0.44);
        let bloom = (rng.next() < 0.07).then(|| FLOWER_COLORS[(rng.next() * 4.0) as usize % 4]);
        // Thicker clumps on the turf side of the edge.
        let members = 1 + (rng.next() * (1.2 - wear.min(0.0)).min(3.0)) as usize;
        let open = |tx: f64, tz: f64| {
            tx.abs() < ARENA - 0.25
                && tz.abs() < ARENA - 0.25
                && road_wear(tx, tz) < 0.35
                && !clearings.blocks(tx, tz)
        };
        meadow.clump(rng, x, 0.02, z, size, members, bloom, &open);
    };
    for road in village_roads() {
        let north_south = road.d > road.w;
        let (half, length) = if north_south {
            (road.w / 2.0, road.d)
        } else {
            (road.d / 2.0, road.w)
        };
        for side in [-1.0, 1.0] {
            let mut t = -length / 2.0;
            while t < length / 2.0 {
                for _ in 0..3 {
                    // Around the painted edge, which wanders a metre or two either
                    // side of the strip's.
                    let out = rng.next().powf(1.3) * 4.4 - 1.8;
                    let along = t + rng.range(-0.2, 0.2);
                    let across = side * (half + out);
                    let (x, z) = if north_south {
                        (road.x + across, road.z + along)
                    } else {
                        (road.x + along, road.z + across)
                    };
                    plant(meadow, rng, x, z);
                }
                t += FRINGE_STEP;
            }
        }
    }
    // The wall's foot, where mowers never reach.
    let mut t = -reach;
    while t < reach {
        for (nx, nz) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let inset = ARENA - 0.25 - rng.next().powf(1.5) * 1.1;
            let along = t + rng.range(-0.2, 0.2);
            let (x, z) = if nx != 0.0 {
                (nx * inset, along)
            } else {
                (along, nz * inset)
            };
            plant(meadow, rng, x, z);
        }
        t += FRINGE_STEP;
    }
}

/// Reed beds in the shallows and on the banks of the creek: clumps of tall
/// tufts at the water's edge, thicker on the inside of the bends.
fn reed_beds(meadow: &mut Meadow, rng: &mut Random) {
    for i in 0..90 {
        let t = (f64::from(i) + rng.next()) / 90.0;
        let (p, tangent) = creek_at(t);
        let side = if rng.next() < 0.5 { -1.0 } else { 1.0 };
        let edge = rng.range(5.6, 7.2) * side;
        let (cx, cz) = (p.x - tangent.z * edge, p.z + tangent.x * edge);
        if cx.abs() < 64.0 && cz.abs() < 64.0 {
            continue;
        }
        let count = 5 + (rng.next() * 6.0) as usize;
        for _ in 0..count {
            let angle = rng.range(0.0, PI * 2.0);
            let reach = rng.next().sqrt() * 1.1;
            let (x, z) = (cx + angle.cos() * reach, cz + angle.sin() * reach);
            let river = creek_distance(x, z);
            let size = rng.range(0.75, 1.3);
            meadow.tuft(rng, x, valley_height_at(x, z, river) + 0.02, z, size, None);
        }
    }
}

/// `new VillageVegetation().group` (`village-meadow`): the instanced tufts (with
/// the per-instance [`WIND_ORIGIN`] attribute) and flowers.
pub fn village_vegetation() -> Node {
    let mut group = Node::group("village-meadow");
    let mut rng = Random::new(91387.0);
    let clearings = Clearings::new();
    let mut meadow = Meadow {
        tufts: Vec::with_capacity(MEADOW_TUFTS),
        origins: Vec::with_capacity(MEADOW_TUFTS * 2),
        flowers: Vec::with_capacity(MEADOW_FLOWERS),
    };
    let mut i = 0;
    while i < MEADOW_CANDIDATES {
        let early = i < 2600;
        let x = if early {
            rng.range(-45.0, 45.0)
        } else {
            rng.range(-120.0, 120.0)
        };
        let z = if early {
            rng.range(-56.0, 56.0)
        } else {
            rng.range(-125.0, 110.0)
        };
        let candidate = i;
        i += 1;
        let inside = x.abs() < 59.0 && z.abs() < 59.0;
        if inside && (road_wear(x, z) > -1.2 || clearings.blocks(x, z)) {
            continue;
        }
        if !inside && farmed(x, z) {
            continue;
        }
        let river = creek_distance(x, z);
        if !inside && (river < 7.0 || (x.abs() < 65.0 && z.abs() < 65.0)) {
            continue;
        }
        if (x * 0.24).sin() * (z * 0.18).cos() + rng.next() < 0.1 {
            continue;
        }
        let height = if inside {
            0.0
        } else {
            valley_height_at(x, z, river)
        };
        let reed = !inside && river < 11.0;
        let size = if reed {
            rng.range(0.6, 1.05)
        } else {
            rng.range(0.24, 0.48)
        };
        let bloom = (!reed && candidate % 3 == 0).then_some(FLOWER_COLORS[candidate % 4]);
        // Tufts gather in clumps where a slow pattern lets them, and stand
        // alone elsewhere.
        let gathering = ((x * 0.29).sin() * (z * 0.23 + 1.1).cos() * 0.5 + 0.5).powf(1.5);
        // Out in the valley the clumps are too far to tell apart.
        let reach = if inside { 1.0 + gathering * 3.0 } else { 1.4 };
        let members = 1 + (rng.next() * reach) as usize;
        let open = |tx: f64, tz: f64| {
            if inside {
                road_wear(tx, tz) <= -1.2 && !clearings.blocks(tx, tz)
            } else {
                creek_distance(tx, tz) >= 7.0 && !farmed(tx, tz)
            }
        };
        meadow.clump(&mut rng, x, height + 0.025, z, size, members, bloom, &open);
    }
    fringe(&mut meadow, &mut rng, &clearings);
    reed_beds(&mut meadow, &mut rng);
    long_stands(&mut meadow, &mut rng, &clearings);
    let Meadow {
        tufts,
        origins,
        flowers,
    } = meadow;
    let grass = Arc::new(Material {
        side: Side::Double,
        vertex_colors: true,
        effect: Effect::Custom {
            name: MEADOW_SWAY,
            params: Vec::new(),
        },
        ..Material::standard(0xffffff, 0.0, 1.0)
    });
    let blooms = Arc::new(Material {
        side: Side::Double,
        vertex_colors: true,
        ..Material::standard(0xffffff, 0.0, 0.8)
    });
    // Each cell's tufts carry their own wind origins, so each has its own copy
    // of the small tuft mesh; the flowers share one.
    let mut tuft_cells: BTreeMap<(i32, i32, bool), (Vec<Instance>, Vec<f32>)> = BTreeMap::new();
    for (tuft, origin) in tufts.into_iter().zip(origins.chunks_exact(2)) {
        let cell = meadow_cell(f64::from(origin[0]), f64::from(origin[1]));
        let (instances, origins) = tuft_cells.entry(cell).or_default();
        instances.push(tuft);
        origins.extend_from_slice(origin);
    }
    let tuft_shape = tuft_geometry();
    let bloom_shape = Arc::new(bloom_geometry());
    let mut flower_cells: BTreeMap<(i32, i32, bool), Vec<Instance>> = BTreeMap::new();
    for flower in flowers {
        let at = flower.matrix.w_axis;
        flower_cells
            .entry(meadow_cell(at.x, at.z))
            .or_default()
            .push(flower);
    }
    for (instances, origins) in tuft_cells.into_values() {
        let mut mesh = tuft_shape.clone();
        mesh.set_attribute(Attribute::instance(WIND_ORIGIN, 2, origins));
        group
            .children
            .push(meadow_node(Arc::new(mesh), grass.clone(), instances));
    }
    for instances in flower_cells.into_values() {
        group
            .children
            .push(meadow_node(bloom_shape.clone(), blooms.clone(), instances));
    }
    let greens = Arc::new(Material {
        side: Side::Double,
        vertex_colors: true,
        ..Material::standard(0xffffff, 0.0, 0.75)
    });
    let crop_shape = Arc::new(crop_geometry());
    let mut crop_cells: BTreeMap<(i32, i32, bool), Vec<Instance>> = BTreeMap::new();
    for head in vegetable_beds() {
        let at = head.matrix.w_axis;
        crop_cells
            .entry(meadow_cell(at.x, at.z))
            .or_default()
            .push(head);
    }
    for instances in crop_cells.into_values() {
        group
            .children
            .push(meadow_node(crop_shape.clone(), greens.clone(), instances));
    }
    group
}

/// One cell's instanced tufts or flowers.
fn meadow_node(mesh: Arc<Mesh>, material: Arc<Material>, instances: Vec<Instance>) -> Node {
    let mut node = Node::mesh(mesh, material);
    let drawable = node.drawable.as_mut().expect("meadow mesh");
    drawable.receive_shadow = true;
    drawable.instances = Some(instances);
    node
}
