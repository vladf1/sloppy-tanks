//! Port of `village-vegetation.ts`: low meadow tufts, reeds and flowers. Detail stays
//! below shell height; the shared tuft instances sway entirely on the GPU
//! ([`effects_scenery::MEADOW_SWAY`]).
//!
//! Inside the arena the grass is thickest where the ground shader frays the roads:
//! tufts line both road shoulders and the foot of the perimeter wall, so the dirt
//! meets real blades instead of a painted edge. Tufts keep off cover, pickups and
//! spawn pads.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::math::{compose, hex_to_linear, quat_from_euler};
use crate::geometry::{Attribute, Mesh};
use crate::scene::{Effect, Instance, Material, Node, Side};
use crate::sim::arena::{PICKUP_LAYOUT, arena_layout, spawn_positions};
use crate::sim::data::ARENA;
use crate::sim::types::{CoverKind, Team};

use super::effects_scenery::{MEADOW_SWAY, WIND_ORIGIN};
use super::village_ground::{farmed, road_wear};
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
/// Meadow flowers: daisy, buttercup, harebell and campion.
const FLOWER_COLORS: [u32; 4] = [0xf4f1e4, 0xf2c94a, 0xa58be0, 0xe99ab4];

/// Nine tapering blades fanned around the tuft, darker at the root. Their normals
/// lean up like the ground's, so a tuft shades into the grass it grows from
/// instead of flickering blade by blade.
fn tuft_geometry() -> Mesh {
    const BLADES: usize = 9;
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
        colors.extend([[0.5f32, 0.56, 0.42], [0.5, 0.56, 0.42], [1.12, 1.1, 0.95]]);
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
    fn tuft(&mut self, rng: &mut Random, x: f64, y: f64, z: f64, size: f64, bloom: Option<u32>) {
        if self.tufts.len() >= MEADOW_TUFTS {
            return;
        }
        let rotation = quat_from_euler(0.0, rng.next() * TUFT_TURN, 0.0);
        let shade = self.tufts.len() % 4;
        self.tufts.push(Instance {
            matrix: compose(
                DVec3::new(x, y, z),
                rotation,
                DVec3::new(size * 0.85, size, size * 0.85),
            ),
            color: instance_color(TUFT_COLORS[shade]),
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
        meadow.tuft(rng, x, 0.02, z, size, bloom);
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
        meadow.tuft(&mut rng, x, height + 0.025, z, size, bloom);
    }
    fringe(&mut meadow, &mut rng, &clearings);
    reed_beds(&mut meadow, &mut rng);
    let Meadow {
        tufts,
        origins,
        flowers,
    } = meadow;
    let mut tuft_mesh = tuft_geometry();
    tuft_mesh.set_attribute(Attribute::instance(WIND_ORIGIN, 2, origins));
    let grass = Material {
        side: Side::Double,
        vertex_colors: true,
        effect: Effect::Custom {
            name: MEADOW_SWAY,
            params: Vec::new(),
        },
        ..Material::standard(0xffffff, 0.0, 1.0)
    };
    let mut tuft_node = Node::mesh(Arc::new(tuft_mesh), Arc::new(grass));
    let mut flower_node = Node::mesh(
        Arc::new(bloom_geometry()),
        Arc::new(Material {
            side: Side::Double,
            vertex_colors: true,
            ..Material::standard(0xffffff, 0.0, 0.8)
        }),
    );
    for (node, instances) in [(&mut tuft_node, tufts), (&mut flower_node, flowers)] {
        let drawable = node.drawable.as_mut().expect("meadow mesh");
        drawable.receive_shadow = true;
        drawable.instances = Some(instances);
    }
    group.children.push(tuft_node);
    group.children.push(flower_node);
    group
}
