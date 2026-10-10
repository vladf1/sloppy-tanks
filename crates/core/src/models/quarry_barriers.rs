//! Port of `quarry-barriers.ts`: precast dragon's teeth and welded steel hedgehogs.

use std::f64::consts::PI;
use std::sync::Arc;

use super::concrete_surfaces::concrete_material;
use super::harbor_surfaces::steel_box;
use super::model_primitives::{
    Cache, adopt_children, cylinder_part, material, put, rotated, shadowed,
};
use crate::geometry::math::hex_to_linear;
use crate::geometry::{BoxGeometry, Mesh, TorusGeometry};
use crate::scene::{Material, Node};
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
