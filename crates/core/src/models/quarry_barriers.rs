//! Port of `quarry-barriers.ts`: precast dragon's teeth and welded steel hedgehogs,
//! and the course of cut sandstone blocks that lines Dusty Dig's boundary.

use std::f64::consts::PI;
use std::sync::Arc;

use super::concrete_surfaces::concrete_material;
use super::model_primitives::{
    Cache, adopt_children, box_part, cylinder_part, material, put, rotated, shadowed,
};
use super::quarry_surfaces::{SAWN, SKIRT, sandstone_material};
use glam::DVec3;

use crate::geometry::math::hex_to_linear;
use crate::geometry::{BoxGeometry, Mesh, TorusGeometry};
use crate::scene::{Material, Node};
use crate::sim::math::Random;
use crate::sim::quarry_barrier_shapes::{
    HEDGEHOG_BEAMS, dragon_tooth_point, dragon_tooth_profile, dragon_tooth_variant,
};

const TOOTH_CONCRETE: u32 = 0xe4ddcc;
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

/// Dusty, rust-bloomed mill steel: pale enough that the sun on its flanges
/// separates a hedgehog from its shadow instead of drawing a black glyph.
const WEB: u32 = 0x9a8670;
const FLANGE: u32 = 0x8a7660;
const BOLT: u32 = 0x999083;
/// Height of the beams' crossing point above the ground in the unscaled model.
const HEDGEHOG_CENTER_Y: f64 = 1.3;
/// The hedgehog's weathered mill steel: dust has killed the sheen.
const HEDGEHOG_METALNESS: f64 = 0.2;
const HEDGEHOG_ROUGHNESS: f64 = 0.82;

/// One plate of a hedgehog: a plain box of dusty steel. The harbor's tiled steel
/// bumps by box UVs that go degenerate on the long tilted members, turning whole
/// flanges black.
fn hedgehog_plate(w: f64, h: f64, d: f64, color: u32) -> Node {
    let mut plate = box_part(w, h, d, color, 0.0);
    if let Some(drawable) = &mut plate.drawable {
        drawable.material = material(color, HEDGEHOG_METALNESS, HEDGEHOG_ROUGHNESS);
    }
    plate
}

/// `steelHedgehog(group)`: three crossed I-sections with real flange depth and bolted
/// connecting plates, in a 2.9 x 2.7 x 3.2 m frame that the cover scales to its size.
pub fn steel_hedgehog(group: &mut Node) {
    for beam in HEDGEHOG_BEAMS {
        let mut assembly = rotated(Node::group(""), beam.rx, 0.0, beam.rz);
        assembly.position.y = HEDGEHOG_CENTER_Y;
        assembly
            .children
            .push(hedgehog_plate(0.12, beam.length, 0.44, WEB));
        for x in [-0.22, 0.22] {
            let flange = hedgehog_plate(0.1, beam.length, 0.52, FLANGE);
            put(&mut assembly, flange, x, 0.0, 0.0);
        }
        // Bake assemblies into the cover's batch without separate draw calls.
        adopt_children(group, assembly);
    }
    for z in [-0.29, 0.29] {
        put(
            group,
            hedgehog_plate(0.64, 0.64, 0.08, FLANGE),
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
const BLOCK_LENGTH: (f64, f64) = (1.3, 3.3);
/// Open joint between neighbouring blocks, in metres.
const BLOCK_GAP: (f64, f64) = (0.06, 0.2);
/// Width of the arris chamfered round a block's top, in metres.
const BLOCK_ARRIS: (f64, f64) = (0.02, 0.05);
/// First bed number of the wall's blocks in the sandstone shader's palette.
const BLOCK_LAYER: f64 = 100.0;
/// Weathered tints of the wall's stone: buff, iron-stained, grey-brown, pale,
/// rust-crusted and weathered grey. One value band, a little darker than the
/// floor's sand so the boundary keeps its line: blocks differ by hue and wear,
/// never by a near-black outlier. Cool-leaning, so the warm sandstone shading
/// settles to buff and grey-brown stone rather than orange.
const BLOCK_TINTS: [[f32; 3]; 6] = [
    [0.86, 0.86, 0.9],
    [0.94, 0.85, 0.79],
    [0.8, 0.8, 0.84],
    [1.0, 0.97, 0.93],
    [0.9, 0.77, 0.69],
    [0.76, 0.75, 0.79],
];
/// Vertex shade of a block's faces against its top, which faces the sky, and
/// of a face's foot, where the drift and the sky's shade darken it.
const FACE_SHADE: f32 = 0.82;
const FOOT_SHADE: f32 = 0.7;

static BLOCK_WALLS: Cache<[u64; 3], Mesh> = Cache::new();

/// `quarry` boundary wall: a line of big quarried sandstone blocks along the
/// wall's footprint (`w` x `d`, base at 0), in the rock cover's own sandstone,
/// set down by a loader rather than laid by a mason. Most are single blocks of
/// their own length and height; some are two narrower blocks side by side, some
/// a low slab with a smaller block left on top, a few broken stubs. Joints open
/// between them, tops tip, corners chip and spall; some long faces are sawn and
/// carry drill traces. Every block keeps to the footprint, so the boundary
/// stays the wall the tanks collide with. A wall running across X leaves its
/// ends to the walls running along Z, which own the corners. One mesh per wall
/// shape, one draw.
pub fn cut_stone_wall(w: f64, h: f64, d: f64) -> Node {
    let mesh =
        BLOCK_WALLS.get_or_insert([w, h, d].map(f64::to_bits), || cut_stone_geometry(w, h, d));
    shadowed(mesh, sandstone_material())
}

/// One loose cut block (`length` x `height` x `depth`, base at 0) of the wall's
/// stone, from `seed`: the spares stacked on the apron and the blocks toppled
/// off the boundary.
pub fn cut_stone_block(length: f64, height: f64, depth: f64, seed: f64) -> Node {
    let mut rng = Random::new(seed);
    let mut faces = Faces::default();
    let block = Block {
        a: (-length / 2.0, length / 2.0),
        b: (-depth / 2.0, depth / 2.0),
        base: 0.0,
        top: height,
        skew: 0.0,
        layer: BLOCK_LAYER + (seed % 61.0).floor(),
    };
    block.build(&mut faces, &mut rng, &|a, y, b| DVec3::new(a, y, b));
    shadowed(Arc::new(faces.mesh()), sandstone_material())
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
    /// with its wear (`uv.y`) and shade.
    fn polygon(&mut self, corners: &[(DVec3, f64, [f32; 3])], layer: f64, facing: DVec3) {
        let base = (self.positions.len() / 3) as u32;
        let normal = (corners[1].0 - corners[0].0).cross(corners[2].0 - corners[0].0);
        for &(p, wear, color) in corners {
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

    /// A closed fan facing up from `centre` round `rim` (corners in order),
    /// each facet with its own corners so the struck top stays faceted rather
    /// than shading round like a loaf.
    fn fan(&mut self, centre: (DVec3, f64), rim: &[(DVec3, f64)], layer: f64, color: [f32; 3]) {
        let n = rim.len();
        for k in 0..n {
            let (a, b) = (rim[k], rim[(k + 1) % n]);
            let base = (self.positions.len() / 3) as u32;
            for (p, wear) in [centre, a, b] {
                self.positions.extend([p.x, p.y, p.z]);
                self.uvs.extend([layer, wear]);
                self.colors.push(color);
            }
            let up = (a.0 - centre.0).cross(b.0 - centre.0).y;
            if up >= 0.0 {
                self.indices.extend([base, base + 1, base + 2]);
            } else {
                self.indices.extend([base, base + 2, base + 1]);
            }
        }
    }

    fn mesh(self) -> Mesh {
        let mut mesh = Mesh::from_f64(&self.positions, &[], &self.uvs, Some(self.indices));
        mesh.colors = self.colors;
        mesh.compute_vertex_normals();
        mesh
    }
}

/// One cut block in wall space: `a` along the wall, `b` across it, standing on
/// `base` with its top at about `top`, turned by `skew` about its centre.
struct Block {
    a: (f64, f64),
    b: (f64, f64),
    base: f64,
    top: f64,
    skew: f64,
    layer: f64,
}

impl Block {
    /// The block's faces into `faces`, placed by `world` (wall space to model
    /// space). Returns the height of its top at its centre.
    fn build(
        &self,
        faces: &mut Faces,
        rng: &mut Random,
        world: &dyn Fn(f64, f64, f64) -> DVec3,
    ) -> f64 {
        let ((a0, a1), (b0, b1)) = (self.a, self.b);
        let centre = DVec3::new((a0 + a1) / 2.0, 0.0, (b0 + b1) / 2.0);
        let top = self.top;
        let tilt = (rng.range(-0.035, 0.035), rng.range(-0.05, 0.05));
        let arris = rng.range(BLOCK_ARRIS.0, BLOCK_ARRIS.1);
        // Chipped corners: each corner of the outline cut back on both sides.
        let span = (a1 - a0).min(b1 - b0);
        // Most corners stay crisp, sawn square; a few have broken back.
        let chip = |rng: &mut Random| {
            let pick = rng.next();
            if pick < 0.22 {
                let most = (span * 0.2).min(0.3);
                (rng.range(0.08, most), rng.range(0.08, most))
            } else if pick < 0.6 {
                (rng.range(0.02, 0.07), rng.range(0.02, 0.07))
            } else {
                (0.012, 0.012)
            }
        };
        let chips = [chip(rng), chip(rng), chip(rng), chip(rng)];
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
        // Corners of some blocks have spalled away from the top down.
        let spalled = [
            (rng.next() < 0.3).then(|| (rng.next() * 4.0) as usize % 4),
            (rng.next() < 0.12).then(|| (rng.next() * 4.0) as usize % 4),
        ];
        let height = top - self.base;
        let spall_drop = rng.range(0.15, 0.32) * height.min(1.2);
        let tint_pick = rng.next();
        let value = rng.range(0.92, 1.06) as f32;
        let tint = BLOCK_TINTS[(tint_pick * BLOCK_TINTS.len() as f64) as usize].map(|c| c * value);
        let face_tint = tint.map(|c| c * FACE_SHADE);
        let foot_tint = tint.map(|c| c * FOOT_SHADE);
        let sawn = rng.next() < 0.6;
        let surface_y = |a: f64, b: f64, corner: Option<usize>| {
            let y = top + tilt.0 * (a - centre.x) + tilt.1 * (b - centre.z);
            if corner.is_some() && spalled.contains(&corner) {
                y - spall_drop
            } else {
                y
            }
        };
        // Which original corner an outline point belongs to.
        let corner_of = |k: usize| k / 2;
        let inset = |k: usize, by: f64| {
            let (a, b) = outline[k];
            // Pull in square to both sides, keeping the chamfer parallel.
            (
                a + (centre.x - a).signum() * by,
                b + (centre.z - b).signum() * by,
            )
        };
        let (skew_sin, skew_cos) = self.skew.sin_cos();
        let place = |a: f64, y: f64, b: f64| {
            let (da, db) = (a - centre.x, b - centre.z);
            world(
                centre.x + da * skew_cos - db * skew_sin,
                y,
                centre.z + da * skew_sin + db * skew_cos,
            )
        };
        // Rings: the toe sunk into what it stands on, a rough lower course
        // pushed out a little, the face's top under the arris, and the top
        // inside it.
        let n = outline.len();
        let mut rings: Vec<Vec<(DVec3, f64)>> = vec![Vec::new(); 4];
        for (k, &(a, b)) in outline.iter().enumerate() {
            let corner = Some(corner_of(k));
            let face_top = surface_y(a, b, corner) - arris * rng.range(0.6, 1.5);
            // Split faces bulge unevenly; sawn ones stay nearly flat.
            let swell = if sawn {
                rng.range(-0.01, 0.02)
            } else {
                rng.range(-0.01, 0.025)
            };
            let (ma, mb) = inset(k, -swell);
            rings[0].push((place(a, self.base - 0.06, b), -0.7));
            rings[1].push((place(ma, self.base + (face_top - self.base) * 0.4, mb), 0.0));
            // A spalled corner shows fresh, pale stone where it broke.
            let lip = if spalled.contains(&corner) { 1.0 } else { 0.4 };
            rings[2].push((place(a, face_top, b), lip));
            let (ia, ib) = inset(k, arris * rng.range(0.6, 1.6));
            rings[3].push((place(ia, surface_y(ia, ib, corner), ib), 1.0));
        }
        for k in 0..n {
            let next = (k + 1) % n;
            let (a, b) = outline[k];
            let (na, nb) = outline[next];
            let outward_ab = DVec3::new(nb - b, 0.0, -(na - a));
            let facing = world(outward_ab.x, 0.0, outward_ab.z) - world(0.0, 0.0, 0.0);
            let from_centre = world(a - centre.x, 0.0, b - centre.z) - world(0.0, 0.0, 0.0);
            let facing = if facing.dot(from_centre) < 0.0 {
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
                // The arris above the face is struck, never sawn; the foot of
                // the face sits in the drift's shade.
                let (low_shade, high_shade, up, cut) = match band {
                    0 => (foot_tint, face_tint, DVec3::ZERO, cut),
                    1 => (face_tint, face_tint, DVec3::ZERO, cut),
                    _ => (face_tint, tint, DVec3::Y * 0.5, 0.0),
                };
                let corners = [
                    (lo[k].0, lo[k].1 + cut, low_shade),
                    (lo[next].0, lo[next].1 + cut, low_shade),
                    (hi[next].0, hi[next].1 + cut, high_shade),
                    (hi[k].0, hi[k].1 + cut, high_shade),
                ];
                faces.polygon(&corners, self.layer, facing + up);
            }
        }
        // The top, weathered stone barely crowned toward its middle, with its
        // lip as wear.
        let crown_y = surface_y(centre.x, centre.z, None) + rng.range(0.0, 0.01);
        let crown = place(centre.x, crown_y, centre.z);
        let cap: Vec<(DVec3, f64)> = rings[3].iter().map(|&(p, _)| (p, 0.2)).collect();
        faces.fan((crown, 0.0), &cap, self.layer, tint);
        crown_y
    }
}

/// How the loader left one place along the wall.
#[derive(Clone, Copy, PartialEq)]
enum Setting {
    /// One block the wall's full thickness.
    Single,
    /// Two narrower blocks side by side across the wall, at their own heights.
    Pair,
    /// A low slab with a smaller block left on top of it.
    Stacked,
    /// A low, broken stub.
    Stub,
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
    let mut previous = Setting::Single;
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
        let b0 = -thickness / 2.0 + rng.range(0.0, 0.05);
        let b1 = thickness / 2.0 - rng.range(0.0, 0.05);
        let layer = BLOCK_LAYER + (i % 61) as f64;
        // Never two of the odd settings in a row: the wall stays a wall.
        let pick = rng.next();
        let setting = if previous != Setting::Single || a1 - a0 < 1.5 {
            Setting::Single
        } else if pick < 0.2 {
            Setting::Pair
        } else if pick < 0.34 {
            Setting::Stacked
        } else if pick < 0.42 {
            Setting::Stub
        } else {
            Setting::Single
        };
        previous = setting;
        // Each block was set down by a loader a little askew; a short block
        // can turn further and still keep to the footprint.
        let skew_limit = 0.05 * (1.8 / (a1 - a0)).min(1.0);
        let skew = rng.range(-skew_limit, skew_limit);
        let block = |a: (f64, f64), b: (f64, f64), base: f64, top: f64, skew: f64| Block {
            a,
            b,
            base,
            top,
            skew,
            layer,
        };
        match setting {
            Setting::Single => {
                let top = h * rng.range(0.86, 1.14);
                block((a0, a1), (b0, b1), 0.0, top, skew).build(&mut faces, &mut rng, &world);
            }
            Setting::Pair => {
                let split = rng.range(-0.25, 0.25);
                let joint = rng.range(0.05, 0.1) / 2.0;
                let (low, high) = (h * rng.range(0.72, 0.92), h * rng.range(1.0, 1.2));
                let outer_high = rng.next() < 0.5;
                for (k, b) in [(b0, split - joint), (split + joint, b1)]
                    .into_iter()
                    .enumerate()
                {
                    let top = if (k == 1) == outer_high { high } else { low };
                    let skew = rng.range(-0.025, 0.025);
                    block((a0, a1), b, 0.0, top, skew).build(&mut faces, &mut rng, &world);
                }
            }
            Setting::Stacked => {
                let slab = h * rng.range(0.52, 0.66);
                let seat =
                    block((a0, a1), (b0, b1), 0.0, slab, skew).build(&mut faces, &mut rng, &world);
                // The block on top, shorter and narrower, left off-centre.
                let (la, lb) = (
                    (a1 - a0) * rng.range(0.5, 0.72),
                    (b1 - b0) * rng.range(0.55, 0.8),
                );
                let ca = (a0 + a1) / 2.0 + rng.range(-0.5, 0.5) * ((a1 - a0) - la);
                let cb = rng.range(-0.5, 0.5) * ((b1 - b0) - lb);
                let top = seat + h * rng.range(0.4, 0.55);
                let skew = skew + rng.range(-0.12, 0.12);
                block(
                    (ca - la / 2.0, ca + la / 2.0),
                    (cb - lb / 2.0, cb + lb / 2.0),
                    seat - 0.03,
                    top,
                    skew,
                )
                .build(&mut faces, &mut rng, &world);
            }
            Setting::Stub => {
                let top = h * rng.range(0.6, 0.76);
                block((a0, a1), (b0, b1), 0.0, top, skew).build(&mut faces, &mut rng, &world);
            }
        }
    }
    joint_drifts(&mut faces, &spans, thickness, length, &world);
    faces.mesh()
}

/// How far a joint's sand drift runs along the wall, out from its face and up
/// it, metres (ranges).
const DRIFT_ALONG: (f64, f64) = (0.45, 0.95);
const DRIFT_OUT: (f64, f64) = (0.45, 0.95);
const DRIFT_HEIGHT: (f64, f64) = (0.18, 0.45);

/// Wind-blown sand banked into the joints between the blocks, on both faces of
/// the wall: a low wedge against the stone, shaded as the floor's own sand (the
/// sandstone shader's skirt). Visual only; the collider stays the wall's box. A
/// stream of its own, so the blocks keep their layout.
fn joint_drifts(
    faces: &mut Faces,
    spans: &[(f64, f64)],
    thickness: f64,
    length: f64,
    world: &dyn Fn(f64, f64, f64) -> DVec3,
) {
    let mut rng = Random::new(9011.0 + length.round());
    let sand = [0.97f32, 0.97, 0.97];
    for &(joint, _) in spans.iter().skip(1) {
        for side in [-1.0, 1.0] {
            if rng.next() < 0.25 {
                continue;
            }
            let face = side * thickness / 2.0;
            let along = rng.range(DRIFT_ALONG.0, DRIFT_ALONG.1);
            let out = rng.range(DRIFT_OUT.0, DRIFT_OUT.1);
            let height = rng.range(DRIFT_HEIGHT.0, DRIFT_HEIGHT.1);
            let lean = rng.range(-0.25, 0.25);
            let inner = face - side * 0.12;
            let left = world(joint - along, 0.0, inner);
            let right = world(joint + along * rng.range(0.7, 1.2), 0.0, inner);
            let toe = world(joint + lean, 0.0, face + side * out);
            let crest = world(joint + lean * 0.3, height, inner);
            let facing = world(0.0, 1.0, side) - world(0.0, 0.0, 0.0);
            for corners in [[left, crest, toe], [crest, right, toe]] {
                let corners = corners.map(|p| (p, SKIRT, sand));
                faces.polygon(&corners, BLOCK_LAYER, facing);
            }
        }
    }
}
