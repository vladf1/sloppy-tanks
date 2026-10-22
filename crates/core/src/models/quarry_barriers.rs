//! Port of `quarry-barriers.ts`: precast dragon's teeth and welded steel hedgehogs,
//! and the course of cut sandstone blocks that lines Dusty Dig's boundary.

use std::f64::consts::PI;
use std::sync::Arc;

use super::concrete_surfaces::concrete_material;
use super::harbor_surfaces::steel_box;
use super::model_primitives::{
    Cache, adopt_children, cylinder_part, material, put, rotated, shadowed,
};
use super::quarry_surfaces::{SAWN, sandstone_material};
use glam::DVec3;

use crate::geometry::math::hex_to_linear;
use crate::geometry::{BoxGeometry, Mesh, TorusGeometry};
use crate::scene::{Material, Node};
use crate::sim::math::Random;
use crate::sim::quarry_barrier_shapes::{
    HEDGEHOG_BEAMS, dragon_tooth_point, dragon_tooth_profile, dragon_tooth_variant,
};

const TOOTH_CONCRETE: u32 = 0xd2d2c9;
const TOOTH_SOIL: u32 = 0x8b816c;
const LIFTING_STEEL: u32 = 0x625447;
const TOOTH_BUMP_SCALE: f32 = 0.055;
/// Every tooth texture tile covers this many metres.
const TOOTH_UV_METRES: f64 = 1.4;

static TEETH: Cache<([u64; 3], usize), Mesh> = Cache::new();
static TOOTH_MATERIAL: Cache<(), Material> = Cache::new();
static LIFTING_ARCH: Cache<(), Mesh> = Cache::new();

/// The concrete wall material with vertex colors and deeper relief.
fn tooth_material() -> Arc<Material> {
    TOOTH_MATERIAL.get_or_insert((), || Material {
        vertex_colors: true,
        bump_scale: TOOTH_BUMP_SCALE,
        ..(*concrete_material()).clone()
    })
}

fn tooth_geometry(w: f64, h: f64, d: f64, variant: usize) -> Arc<Mesh> {
    TEETH.get_or_insert(([w, h, d].map(f64::to_bits), variant), || {
        let mut mesh = BoxGeometry {
            height_segments: 4,
            ..BoxGeometry::default()
        }
        .build();
        let tint = 0.97 + variant as f64 * 0.02;
        let concrete = hex_to_linear(TOOTH_CONCRETE).map(|c| c * tint);
        let soil = hex_to_linear(TOOTH_SOIL);
        let mut colors = Vec::with_capacity(mesh.positions.len());
        for i in 0..mesh.positions.len() {
            let [x, y, z] = mesh.positions[i].map(f64::from);
            let point = dragon_tooth_point(x, y, z, w, h, d, variant);
            mesh.positions[i] = point.map(|v| v as f32);
            let [u, v] = mesh.uvs[i].map(f64::from);
            mesh.uvs[i] = [
                ((u * w) / TOOTH_UV_METRES + variant as f64 * 0.23) as f32,
                ((v * h) / TOOTH_UV_METRES) as f32,
            ];
            // Soil darkens the lowest fifth of the block.
            let alpha = (1.0 - (y + 0.5) * 5.0).max(0.0) * 0.5;
            colors.push([0, 1, 2].map(|k| (concrete[k] + (soil[k] - concrete[k]) * alpha) as f32));
        }
        mesh.colors = colors;
        mesh.compute_vertex_normals();
        mesh
    })
}

/// `dragonTooth(group, w, h, d, x, z)`: a separate precast block with weathered faces,
/// a soil-darkened foot and, on most variants, an exposed rebar lifting eye.
pub fn dragon_tooth(group: &mut Node, w: f64, h: f64, d: f64, x: f64, z: f64) {
    let variant = dragon_tooth_variant(x, z);
    put(
        group,
        shadowed(tooth_geometry(w, h, d, variant), tooth_material()),
        0.0,
        h / 2.0,
        0.0,
    );
    let profile = dragon_tooth_profile(variant);
    if profile.top_scale == 0.0 {
        return;
    }
    let yaw = profile.yaw;
    let arch_material = material(LIFTING_STEEL, 0.55, 0.85);
    let arch_mesh = LIFTING_ARCH.get_or_insert((), || {
        TorusGeometry {
            radius: 0.105,
            tube: 0.022,
            radial_segments: 6,
            tubular_segments: 12,
            arc: PI,
        }
        .build()
    });
    let mut arch = shadowed(arch_mesh, arch_material.clone());
    arch.set_rotation_euler(0.0, yaw, 0.0);
    put(group, arch, 0.0, h + 0.08, 0.0);
    for side in [-1.0, 1.0] {
        let mut stem = cylinder_part(0.022, 0.18, LIFTING_STEEL, 6);
        if let Some(drawable) = &mut stem.drawable {
            drawable.material = arch_material.clone();
        }
        put(
            group,
            stem,
            side * 0.105 * yaw.cos(),
            h - 0.01,
            -side * 0.105 * yaw.sin(),
        );
    }
}

const WEB: u32 = 0x726454;
const FLANGE: u32 = 0x625b50;
const BOLT: u32 = 0x999083;
/// Height of the beams' crossing point above the ground in the unscaled model.
const HEDGEHOG_CENTER_Y: f64 = 1.3;

/// `steelHedgehog(group)`: three crossed I-sections with real flange depth and bolted
/// connecting plates, in a 2.9 x 2.7 x 3.2 m frame that the cover scales to its size.
pub fn steel_hedgehog(group: &mut Node) {
    for beam in HEDGEHOG_BEAMS {
        let mut assembly = rotated(Node::group(""), beam.rx, 0.0, beam.rz);
        assembly.position.y = HEDGEHOG_CENTER_Y;
        assembly
            .children
            .push(steel_box(0.12, beam.length, 0.44, WEB));
        for x in [-0.22, 0.22] {
            let flange = steel_box(0.1, beam.length, 0.52, FLANGE);
            put(&mut assembly, flange, x, 0.0, 0.0);
        }
        // Bake assemblies into the cover's batch without separate draw calls.
        adopt_children(group, assembly);
    }
    for z in [-0.29, 0.29] {
        put(
            group,
            steel_box(0.64, 0.64, 0.08, FLANGE),
            0.0,
            HEDGEHOG_CENTER_Y,
            z,
        );
        for x in [-0.2, 0.2] {
            for y in [1.1, 1.5] {
                let mut bolt = cylinder_part(0.055, 0.1, BOLT, 6);
                bolt.set_rotation_euler(PI / 2.0, 0.0, 0.0);
                put(group, bolt, x, y, z);
            }
        }
    }
}

/// Length range of one cut block along the wall, in metres.
const BLOCK_LENGTH: (f64, f64) = (1.5, 2.9);
/// Open joint between neighbouring blocks, in metres.
const BLOCK_GAP: (f64, f64) = (0.05, 0.13);
/// Width of the arris chamfered round a block's top, in metres.
const BLOCK_ARRIS: (f64, f64) = (0.04, 0.15);
/// First bed number of the wall's blocks in the sandstone shader's palette.
const BLOCK_LAYER: f64 = 100.0;
/// Weathered tints of the wall's stone: buff, iron-stained, grey-brown, pale.
/// Darker than the floor's sand on the whole so the boundary keeps its line.
/// Cool-leaning, so the warm sandstone shading settles to buff and grey-brown
/// stone rather than orange.
const BLOCK_TINTS: [[f32; 3]; 4] = [
    [0.84, 0.85, 0.89],
    [0.88, 0.85, 0.83],
    [0.76, 0.78, 0.83],
    [0.92, 0.92, 0.93],
];
/// Vertex shade of a block's faces against its top, which faces the sky.
const FACE_SHADE: f32 = 0.84;

static BLOCK_WALLS: Cache<[u64; 3], Mesh> = Cache::new();

/// `quarry` boundary wall: one course of big quarried sandstone blocks laid end
/// to end along the wall's footprint (`w` x `d`, base at 0), in the rock cover's
/// own sandstone. Blocks run to their own lengths and heights with open joints
/// between them, their tops tipped a little, corners chipped and the odd corner
/// spalled away; some long faces are sawn and carry drill traces. The tops are
/// lit lips round darker, varnished faces, so the boundary reads from any
/// camera. A wall running across X leaves its ends to the walls running along Z,
/// which own the corners. One mesh per wall shape, one draw.
pub fn cut_stone_wall(w: f64, h: f64, d: f64) -> Node {
    let mesh =
        BLOCK_WALLS.get_or_insert([w, h, d].map(f64::to_bits), || cut_stone_geometry(w, h, d));
    shadowed(mesh, sandstone_material())
}

/// Flat-shaded faces of the wall: every quad keeps its own corners so the cut
/// edges stay crisp.
#[derive(Default)]
struct Faces {
    positions: Vec<f64>,
    uvs: Vec<f64>,
    colors: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl Faces {
    /// A polygon (convex, corners in order) wound to face `facing`, each corner
    /// with its wear (`uv.y`).
    fn polygon(&mut self, corners: &[(DVec3, f64)], layer: f64, color: [f32; 3], facing: DVec3) {
        let base = (self.positions.len() / 3) as u32;
        let normal = (corners[1].0 - corners[0].0).cross(corners[2].0 - corners[0].0);
        for &(p, wear) in corners {
            self.positions.extend([p.x, p.y, p.z]);
            self.uvs.extend([layer, wear]);
            self.colors.push(color);
        }
        let flip = normal.dot(facing) < 0.0;
        for k in 1..corners.len() as u32 - 1 {
            if flip {
                self.indices.extend([base, base + k + 1, base + k]);
            } else {
                self.indices.extend([base, base + k, base + k + 1]);
            }
        }
    }
}

impl Faces {
    /// A closed fan facing up from `centre` round `rim` (corners in order).
    fn fan(&mut self, centre: (DVec3, f64), rim: &[(DVec3, f64)], layer: f64, color: [f32; 3]) {
        let base = (self.positions.len() / 3) as u32;
        for &(p, wear) in std::iter::once(&centre).chain(rim) {
            self.positions.extend([p.x, p.y, p.z]);
            self.uvs.extend([layer, wear]);
            self.colors.push(color);
        }
        let n = rim.len() as u32;
        for k in 0..n {
            let (a, b) = (base + 1 + k, base + 1 + (k + 1) % n);
            let (pa, pb) = (rim[k as usize].0, rim[((k + 1) % n) as usize].0);
            let up = (pa - centre.0).cross(pb - centre.0).y;
            if up >= 0.0 {
                self.indices.extend([base, a, b]);
            } else {
                self.indices.extend([base, b, a]);
            }
        }
    }
}

fn cut_stone_geometry(w: f64, h: f64, d: f64) -> Mesh {
    let along_x = w > d;
    let (length, thickness) = if along_x { (w, d) } else { (d, w) };
    // Walls across X stop short of the corner squares the Z walls fill.
    let span = if along_x {
        length - thickness * 2.0
    } else {
        length
    };
    // Wall space: `a` along the wall, `b` across it, y up.
    let world = |a: f64, y: f64, b: f64| {
        if along_x {
            DVec3::new(a, y, b)
        } else {
            DVec3::new(b, y, -a)
        }
    };
    let mut rng = Random::new(4021.0 + length.round());
    // Lay the blocks end to end, the last one taking up the slack.
    let mut spans = Vec::new();
    let mut at = -span / 2.0;
    while at < span / 2.0 - 0.01 {
        let mut block = rng.range(BLOCK_LENGTH.0, BLOCK_LENGTH.1);
        if span / 2.0 - (at + block) < BLOCK_LENGTH.0 {
            block = span / 2.0 - at;
        }
        spans.push((at, at + block));
        at += block;
    }
    let mut faces = Faces::default();
    for (i, &(a0, a1)) in spans.iter().enumerate() {
        let gap = rng.range(BLOCK_GAP.0, BLOCK_GAP.1) / 2.0;
        let (a0, a1) = (
            if i == 0 && along_x { a0 } else { a0 + gap },
            if i + 1 == spans.len() && along_x {
                a1
            } else {
                a1 - gap
            },
        );
        let b0 = -thickness / 2.0 + rng.range(0.0, 0.07);
        let b1 = thickness / 2.0 - rng.range(0.0, 0.07);
        let centre = DVec3::new((a0 + a1) / 2.0, 0.0, (b0 + b1) / 2.0);
        let top = h + rng.range(-0.14, 0.16);
        let tilt = (rng.range(-0.035, 0.035), rng.range(-0.04, 0.04));
        // Each block was set down by a loader a little askew.
        let skew = rng.range(-0.035, 0.035);
        let arris = rng.range(BLOCK_ARRIS.0, BLOCK_ARRIS.1);
        // Chipped corners: each corner of the outline cut back on both sides.
        let chip = |rng: &mut Random| {
            if rng.next() < 0.7 {
                (rng.range(0.04, 0.3), rng.range(0.04, 0.3))
            } else {
                (0.015, 0.015)
            }
        };
        let chips = [
            chip(&mut rng),
            chip(&mut rng),
            chip(&mut rng),
            chip(&mut rng),
        ];
        // Outline, counter-clockwise from the -b face's start.
        let corners = [(a0, b0), (a1, b0), (a1, b1), (a0, b1)];
        let mut outline: Vec<(f64, f64)> = Vec::with_capacity(8);
        for (k, &(a, b)) in corners.iter().enumerate() {
            let (into_a, into_b) = (-(a - centre.x).signum(), -(b - centre.z).signum());
            let (cut_a, cut_b) = chips[k];
            // Arrive along the previous side, leave along the next.
            if k % 2 == 0 {
                outline.push((a, b + into_b * cut_b));
                outline.push((a + into_a * cut_a, b));
            } else {
                outline.push((a + into_a * cut_a, b));
                outline.push((a, b + into_b * cut_b));
            }
        }
        // One corner of some blocks has spalled away from the top down.
        let spalled = (rng.next() < 0.22).then(|| (rng.next() * 4.0) as usize % 4);
        let spall_drop = rng.range(0.18, 0.4);
        let tint_pick = rng.next();
        let tint = BLOCK_TINTS[(tint_pick * BLOCK_TINTS.len() as f64) as usize]
            .map(|c| c * rng.range(0.94, 1.05) as f32);
        let face_tint = tint.map(|c| c * FACE_SHADE);
        let layer = BLOCK_LAYER + (i % 61) as f64;
        let sawn = rng.next() < 0.65;
        let surface_y = |a: f64, b: f64, corner: Option<usize>| {
            let y = top + tilt.0 * (a - centre.x) + tilt.1 * (b - centre.z);
            if corner.is_some() && corner == spalled {
                y - spall_drop
            } else {
                y
            }
        };
        // Which original corner an outline point belongs to.
        let corner_of = |k: usize| k / 2;
        // Rings: the toe sunk into the drift, a rough mid course pushed out a
        // little, the face's top under the arris, and the top inside it.
        let inset = |k: usize, by: f64| {
            let (a, b) = outline[k];
            // Pull in square to both sides, keeping the chamfer parallel.
            (
                a + (centre.x - a).signum() * by,
                b + (centre.z - b).signum() * by,
            )
        };
        let (skew_sin, skew_cos) = skew.sin_cos();
        let place = |a: f64, y: f64, b: f64| {
            let (da, db) = (a - centre.x, b - centre.z);
            world(
                centre.x + da * skew_cos - db * skew_sin,
                y,
                centre.z + da * skew_sin + db * skew_cos,
            )
        };
        let n = outline.len();
        let mut rings: Vec<Vec<(DVec3, f64)>> = vec![Vec::new(); 4];
        for k in 0..n {
            let (a, b) = outline[k];
            let corner = Some(corner_of(k));
            let face_top = surface_y(a, b, corner) - arris * rng.range(0.6, 1.5);
            // Split faces bulge unevenly; sawn ones stay nearly flat.
            let swell = if sawn {
                rng.range(-0.01, 0.025)
            } else {
                rng.range(-0.01, 0.09)
            };
            let (ma, mb) = inset(k, -swell);
            rings[0].push((place(a, -0.06, b), -0.6));
            rings[1].push((place(ma, face_top * 0.45, mb), 0.0));
            // A spalled corner shows fresh, pale stone where it broke.
            let lip = if corner == spalled { 1.0 } else { 0.35 };
            rings[2].push((place(a, face_top, b), lip));
            let (ia, ib) = inset(k, arris * rng.range(0.6, 1.6));
            rings[3].push((place(ia, surface_y(ia, ib, corner), ib), 1.0));
        }
        for k in 0..n {
            let next = (k + 1) % n;
            let (a, b) = outline[k];
            let (na, nb) = outline[next];
            let outward_ab = DVec3::new(nb - b, 0.0, -(na - a));
            let facing = world(outward_ab.x, 0.0, outward_ab.z);
            let facing = if facing.dot(world(a - centre.x, 0.0, b - centre.z)) < 0.0 {
                -facing
            } else {
                facing
            };
            // The long faces of a sawn block were cut flat and drilled.
            let long_face = (b - b0).abs() < 1e-6 && (nb - b0).abs() < 1e-6
                || (b - b1).abs() < 1e-6 && (nb - b1).abs() < 1e-6;
            let cut = if sawn && long_face { SAWN } else { 0.0 };
            for band in 0..3 {
                let (lo, hi) = (&rings[band], &rings[band + 1]);
                // The arris above the face is struck, never sawn.
                let (shade, up, cut) = if band == 2 {
                    (tint, DVec3::Y * 0.5, 0.0)
                } else {
                    (face_tint, DVec3::ZERO, cut)
                };
                let corners = [lo[k], lo[next], hi[next], hi[k]].map(|(p, wear)| (p, wear + cut));
                faces.polygon(&corners, layer, shade, facing + up);
            }
        }
        // The top, weathered stone crowned a little toward its middle, with its
        // lip as wear.
        let crown = place(
            centre.x,
            surface_y(centre.x, centre.z, None) + rng.range(0.02, 0.08),
            centre.z,
        );
        let cap: Vec<(DVec3, f64)> = rings[3].iter().map(|&(p, _)| (p, 0.2)).collect();
        faces.fan((crown, 0.0), &cap, layer, tint);
    }
    let mut mesh = Mesh::from_f64(&faces.positions, &[], &faces.uvs, Some(faces.indices));
    mesh.colors = faces.colors;
    mesh.compute_vertex_normals();
    mesh
}
