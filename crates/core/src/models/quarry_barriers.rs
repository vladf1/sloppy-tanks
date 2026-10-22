//! Port of `quarry-barriers.ts`: precast dragon's teeth and welded steel hedgehogs,
//! and the sand-filled bastion cells that line Dusty Dig's boundary.

use std::f64::consts::PI;
use std::sync::Arc;

use super::concrete_surfaces::concrete_material;
use super::harbor_surfaces::steel_box;
use super::model_primitives::{
    Cache, adopt_children, cylinder_part, material, put, rotated, shadowed,
};
use glam::DVec3;

use crate::geometry::math::hex_to_linear;
use crate::geometry::{BoxGeometry, Mesh, TorusGeometry};
use crate::scene::{Material, Node, TextureRef};
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

/// Dusty Dig's bastion atlas (`scripts/generate-quarry-textures.ts`): a cell's side
/// on the left half, its sand-filled top on the right.
pub const BASTION_TEXTURE: &str = "textures/quarry/bastion.webp";
/// A bastion cell's footprint along and across the wall, in metres.
const BASTION_CELL: f64 = 1.0;
/// How far a cell's liner bulges out between its mesh edges, in metres.
const BASTION_BULGE: f64 = 0.055;
/// Height of the cell side the atlas covers, in metres.
const BASTION_ATLAS_HEIGHT: f64 = 1.2;
/// Inset of an atlas half's UVs, so filtering never reaches the other half.
const ATLAS_GUTTER: f64 = 0.004;

static BASTION_WALLS: Cache<[u64; 3], Mesh> = Cache::new();
static BASTION_MATERIAL: Cache<(), Material> = Cache::new();

fn bastion_material() -> Arc<Material> {
    BASTION_MATERIAL.get_or_insert((), || Material {
        map: Some(TextureRef {
            anisotropy: 4,
            ..TextureRef::file(BASTION_TEXTURE)
        }),
        vertex_colors: true,
        ..Material::standard(0xffffff, 0.0, 0.95)
    })
}

/// `quarry` boundary wall: a line of sand-filled wire-mesh bastion cells, two
/// cells deep, standing on the wall's footprint (`w` x `d`, base at 0). The liner
/// bulges between the mesh edges, cells settle to slightly different heights,
/// their tops are sand, and each takes its own tint of sun-faded tan. A wall
/// running across X leaves its ends to the walls running along Z, which own the
/// corners. One mesh per wall shape, one draw.
pub fn bastion_wall(w: f64, h: f64, d: f64) -> Node {
    let mesh = BASTION_WALLS.get_or_insert([w, h, d].map(f64::to_bits), || {
        bastion_geometry(w, h, d)
    });
    shadowed(mesh, bastion_material())
}

/// Collects the bastion's grids: each a small indexed patch whose normals stay
/// smooth within it and break at its edges.
struct Patches {
    positions: Vec<f64>,
    uvs: Vec<f64>,
    colors: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl Patches {
    /// A 3 x 3 grid of points, rows across and columns along it, wound to face
    /// `facing`.
    fn grid(
        &mut self,
        points: [[DVec3; 3]; 3],
        uvs: [[[f64; 2]; 3]; 3],
        colors: [[[f32; 3]; 3]; 3],
        facing: DVec3,
    ) {
        let normal = (points[0][2] - points[0][0]).cross(points[2][0] - points[0][0]);
        let flip = normal.dot(facing) < 0.0;
        let base = (self.positions.len() / 3) as u32;
        for row in 0..3 {
            for column in 0..3 {
                let p = points[row][column];
                self.positions.extend([p.x, p.y, p.z]);
                self.uvs.extend(uvs[row][column]);
                self.colors.push(colors[row][column]);
            }
        }
        for row in 0..2u32 {
            for column in 0..2u32 {
                let a = base + row * 3 + column;
                if flip {
                    self.indices.extend([a, a + 3, a + 1, a + 1, a + 3, a + 4]);
                } else {
                    self.indices.extend([a, a + 1, a + 3, a + 1, a + 4, a + 3]);
                }
            }
        }
    }

    /// A flat quad, corners in order round it, wound to face `facing`.
    fn quad(&mut self, corners: [DVec3; 4], uvs: [[f64; 2]; 4], color: [f32; 3], facing: DVec3) {
        let base = (self.positions.len() / 3) as u32;
        let normal = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
        for (p, uv) in corners.into_iter().zip(uvs) {
            self.positions.extend([p.x, p.y, p.z]);
            self.uvs.extend(uv);
            self.colors.push(color);
        }
        if normal.dot(facing) < 0.0 {
            self.indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
        } else {
            self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
}

fn bastion_geometry(w: f64, h: f64, d: f64) -> Mesh {
    let along_x = w > d;
    let (length, thickness) = if along_x { (w, d) } else { (d, w) };
    // Walls across X stop short of the corner squares the Z walls fill.
    let span = if along_x { length - thickness * 2.0 } else { length };
    let cells = (span / BASTION_CELL).round().max(1.0) as usize;
    let rows = (thickness / BASTION_CELL).round().max(1.0) as usize;
    let (step, depth) = (span / cells as f64, thickness / rows as f64);
    // Wall space: `a` along the wall, `b` across it, y up.
    let world = |a: f64, y: f64, b: f64| {
        if along_x {
            DVec3::new(a, y, b)
        } else {
            DVec3::new(b, y, -a)
        }
    };
    let out_b = |outward: f64| world(0.0, 0.0, outward);
    let out_a = |outward: f64| world(outward, 0.0, 0.0);
    let mut rng = Random::new(4021.0 + length.round());
    let heights: Vec<Vec<f64>> = (0..cells)
        .map(|_| {
            (0..rows)
                .map(|_| {
                    if rng.next() < 0.03 {
                        h - rng.range(0.06, 0.12)
                    } else {
                        h + rng.range(-0.012, 0.008)
                    }
                })
                .collect()
        })
        .collect();
    // Cells go up in runs from one delivery, so a run shares its liner's fade;
    // within it each cell differs only a shade.
    let mut run_tint = [1.0f32; 3];
    let mut run_left = 0usize;
    let tints: Vec<Vec<([f32; 3], f64, bool)>> = (0..cells)
        .map(|_| {
            if run_left == 0 {
                run_left = 4 + (rng.next() * 9.0) as usize;
                let pick = rng.next();
                run_tint = if pick < 0.12 {
                    // An older run of cells in olive drab.
                    [0.82, 0.85, 0.68]
                } else if pick < 0.4 {
                    [1.05, 1.03, 1.0]
                } else if pick < 0.6 {
                    [0.95, 0.92, 0.87]
                } else {
                    [1.0, 0.99, 0.97]
                };
            }
            run_left -= 1;
            (0..rows)
                .map(|_| {
                    let shade = rng.range(0.97, 1.03) as f32;
                    (
                        run_tint.map(|c| c * shade),
                        BASTION_BULGE * rng.range(0.7, 1.35),
                        rng.next() < 0.5,
                    )
                })
                .collect()
        })
        .collect();
    let mut patches = Patches {
        positions: Vec::new(),
        uvs: Vec::new(),
        colors: Vec::new(),
        indices: Vec::new(),
    };
    let side_u = |s: f64, flip: bool| {
        let s = if flip { 1.0 - s } else { s };
        ATLAS_GUTTER + s * (0.5 - ATLAS_GUTTER * 2.0)
    };
    let side_v = |y: f64| (y / BASTION_ATLAS_HEIGHT).clamp(0.0, 1.0);
    for i in 0..cells {
        let a0 = -span / 2.0 + step * i as f64;
        for j in 0..rows {
            let b0 = -thickness / 2.0 + depth * j as f64;
            let top = heights[i][j];
            let (tint, bulge, flip) = tints[i][j];
            // The liner bulges most a little below mid height, where the fill
            // presses hardest, and pinches in at the mesh edges.
            let swell = |s: f64, t: f64| {
                let across = (PI * s).sin();
                let up = [0.35, 1.0, 0.2][(t * 2.0).round() as usize];
                bulge * across * up
            };
            let foot = tint.map(|c| c * 0.82);
            let shades = [foot, tint, tint];
            // The two long sides: the outer face of the first row and of the last.
            for (exposed, b, outward) in [(j == 0, b0, -1.0), (j + 1 == rows, b0 + depth, 1.0)] {
                if !exposed {
                    continue;
                }
                let mut points = [[DVec3::ZERO; 3]; 3];
                let mut uvs = [[[0.0; 2]; 3]; 3];
                let mut colors = [[[0.0f32; 3]; 3]; 3];
                for (row, t) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                    let y = top * if row == 1 { 0.45 } else { t };
                    for (column, s) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                        let a = a0 + step * s;
                        let push = swell(s, t) * outward;
                        points[row][column] = world(a, y, b + push);
                        uvs[row][column] = [side_u(s, flip), side_v(y)];
                        colors[row][column] = shades[row];
                    }
                }
                patches.grid(points, uvs, colors, out_b(outward));
            }
            // The wall's ends, where nothing stands beyond them.
            for (exposed, a, outward) in [(i == 0 && !along_x, a0, -1.0), (i + 1 == cells && !along_x, a0 + step, 1.0)] {
                if !exposed {
                    continue;
                }
                let mut points = [[DVec3::ZERO; 3]; 3];
                let mut uvs = [[[0.0; 2]; 3]; 3];
                let mut colors = [[[0.0f32; 3]; 3]; 3];
                for (row, t) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                    let y = top * if row == 1 { 0.45 } else { t };
                    for (column, s) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                        let b = b0 + depth * s;
                        points[row][column] = world(a + swell(s, t) * outward, y, b);
                        uvs[row][column] = [side_u(s, !flip), side_v(y)];
                        colors[row][column] = shades[row];
                    }
                }
                patches.grid(points, uvs, colors, out_a(outward));
            }
            // The sand fill, settled a little under the rim and heaped or slumped
            // in the middle; its edges meet the sides' top rows.
            let edge_push = |s: f64, side_b: Option<f64>| {
                side_b.map_or(0.0, |outward| swell(s, 1.0) * outward)
            };
            let mut points = [[DVec3::ZERO; 3]; 3];
            let mut uvs = [[[0.0; 2]; 3]; 3];
            let crown = top + rng.range(-0.02, 0.08);
            for (row, t) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                for (column, s) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                    let a = a0 + step * s;
                    let mut b = b0 + depth * t;
                    let rim = row != 1 || column != 1;
                    let y = if !rim {
                        crown
                    } else if row == 1 || column == 1 {
                        top - 0.01
                    } else {
                        top
                    };
                    if row == 0 && j == 0 {
                        b += edge_push(s, Some(-1.0));
                    }
                    if row == 2 && j + 1 == rows {
                        b += edge_push(s, Some(1.0));
                    }
                    points[row][column] = world(a, y, b);
                    uvs[row][column] = [
                        0.5 + ATLAS_GUTTER + s * (0.5 - ATLAS_GUTTER * 2.0),
                        ATLAS_GUTTER + t * (1.0 - ATLAS_GUTTER * 2.0),
                    ];
                }
            }
            patches.grid(points, uvs, [[tint; 3]; 3], DVec3::Y);
            // Steps where a cell stands above its neighbour along or across.
            let next_along = (i + 1 < cells).then(|| heights[i + 1][j]);
            let prev_along = (i > 0).then(|| heights[i - 1][j]);
            let next_across = (j + 1 < rows).then(|| heights[i][j + 1]);
            let prev_across = (j > 0).then(|| heights[i][j - 1]);
            let step_u = |s: f64| side_u(s, flip);
            for (neighbour, a, outward) in [(next_along, a0 + step, 1.0), (prev_along, a0, -1.0)] {
                if let Some(low) = neighbour.filter(|&low| low < top - 0.003) {
                    let (b_from, b_to) = (b0, b0 + depth);
                    patches.quad(
                        [world(a, low, b_from), world(a, low, b_to), world(a, top, b_to), world(a, top, b_from)],
                        [[step_u(0.0), side_v(low)], [step_u(1.0), side_v(low)], [step_u(1.0), side_v(top)], [step_u(0.0), side_v(top)]],
                        tint,
                        out_a(outward),
                    );
                }
            }
            for (neighbour, b, outward) in [(next_across, b0 + depth, 1.0), (prev_across, b0, -1.0)] {
                if let Some(low) = neighbour.filter(|&low| low < top - 0.003) {
                    let (a_from, a_to) = (a0, a0 + step);
                    patches.quad(
                        [world(a_from, low, b), world(a_to, low, b), world(a_to, top, b), world(a_from, top, b)],
                        [[step_u(0.0), side_v(low)], [step_u(1.0), side_v(low)], [step_u(1.0), side_v(top)], [step_u(0.0), side_v(top)]],
                        tint,
                        out_b(outward),
                    );
                }
            }
        }
    }
    let mut mesh = Mesh::from_f64(&patches.positions, &[], &patches.uvs, Some(patches.indices));
    mesh.colors = patches.colors;
    mesh.compute_vertex_normals();
    mesh
}

#[cfg(test)]
mod tmp_count {
    use crate::scene::Node;
    fn casters(node: &Node, out: &mut (usize, usize)) {
        if let Some(d) = &node.drawable {
            let mesh = &d.mesh;
            let t = mesh.indices.as_ref().map_or(mesh.vertex_count(), Vec::len) / 3;
            let n = d.instances.as_ref().map_or(1, |i| i.len());
            if d.cast_shadow { out.0 += t * n; } else { out.1 += t * n; }
        }
        for c in &node.children { casters(c, out); }
    }
    #[test]
    fn tmp_count_triangles() {
        let mut out = (0, 0);
        casters(&crate::models::quarry_scenery::quarry_scenery(), &mut out);
        eprintln!("quarry scenery casters {} receivers-only {}", out.0, out.1);
    }
}
