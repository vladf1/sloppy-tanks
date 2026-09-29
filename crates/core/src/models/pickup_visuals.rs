//! Ports of `pickup-visuals.ts` and `pickup-atlas.ts`: pickup cubes and ammo crates
//! with pictograms from one atlas.
//!
//! Pickup materials are always transparent so first person can fade them through
//! the [`super::effects_props::PICKUP_SURFACE`] opacity uniform without switching
//! pipelines mid-round; at full opacity they look opaque.

use std::sync::Arc;

use super::effects_props::PICKUP_SURFACE;
use super::model_primitives::Cache;
use crate::geometry::{Mesh, box_geometry, merge_geometries};
use crate::scene::{Color, Effect, Material, Node, TextureRef, Wrap};
use crate::sim::ammunition::is_special_ammo;
use crate::sim::types::PickupKind;

/// `PICKUP_ATLAS_TILES[kind]`: (column, row) of the kind's pictogram.
pub fn pickup_atlas_tile(kind: PickupKind) -> (u32, u32) {
    match kind {
        PickupKind::Spread => (0, 0),
        PickupKind::Rocket => (1, 0),
        PickupKind::Ricochet => (2, 0),
        PickupKind::Piercing => (0, 1),
        PickupKind::Rapid => (1, 1),
        PickupKind::Shield => (2, 1),
        PickupKind::Speed => (0, 2),
        PickupKind::Repair => (1, 2),
        PickupKind::Laser => (2, 2),
    }
}

/// The packed pictogram atlas under `public/`; source pixels stay intact.
pub const PICKUP_ATLAS_PATH: &str = "textures/pickups/atlas.webp";
pub const PICKUP_ICON_SIZE: f64 = 256.0;
pub const PICKUP_ATLAS_PADDING: f64 = 16.0;
pub const PICKUP_ATLAS_STRIDE: f64 = PICKUP_ICON_SIZE + PICKUP_ATLAS_PADDING * 2.0;
pub const PICKUP_ATLAS_SIZE: f64 = PICKUP_ATLAS_STRIDE * 3.0;

/// `pickupAtlasUV(kind, u, v)`: a face UV mapped into the kind's padded atlas tile.
pub fn pickup_atlas_uv(kind: PickupKind, u: f64, v: f64) -> [f64; 2] {
    let (column, row) = pickup_atlas_tile(kind);
    [
        (f64::from(column) * PICKUP_ATLAS_STRIDE + PICKUP_ATLAS_PADDING + u * PICKUP_ICON_SIZE)
            / PICKUP_ATLAS_SIZE,
        1.0 - (f64::from(row) * PICKUP_ATLAS_STRIDE
            + PICKUP_ATLAS_PADDING
            + (1.0 - v) * PICKUP_ICON_SIZE)
            / PICKUP_ATLAS_SIZE,
    ]
}

const HARDWARE_COLOR: u32 = 0x273544;
const FACE_EMISSIVE_INTENSITY: f32 = 0.3;

static FACES: Cache<PickupKind, Mesh> = Cache::new();
static SHARED: Cache<u8, Mesh> = Cache::new();
static MATERIALS: Cache<u8, Material> = Cache::new();

/// The steel rim, base and latch of an ammo crate: a raised rim leaves the top-face
/// symbol visible from the overhead camera.
fn hardware_geometry() -> Arc<Mesh> {
    SHARED.get_or_insert(0, || {
        let parts = [
            ([1.94, 0.14, 0.09], [0.0, 0.49, -0.615]),
            ([1.94, 0.14, 0.09], [0.0, 0.49, 0.615]),
            ([0.09, 0.14, 1.14], [-0.925, 0.49, 0.0]),
            ([0.09, 0.14, 1.14], [0.925, 0.49, 0.0]),
            ([1.9, 0.1, 1.28], [0.0, -0.5, 0.0]),
            ([0.2, 0.28, 0.08], [0.0, 0.38, 0.64]),
        ]
        .map(|([w, h, d], [x, y, z])| {
            let mut mesh = box_geometry(w, h, d);
            mesh.translate(x, y, z);
            mesh
        });
        merge_geometries(&parts.iter().collect::<Vec<_>>()).expect("boxes share one layout")
    })
}

fn hardware_material() -> Arc<Material> {
    MATERIALS.get_or_insert(0, || Material {
        color: Color(HARDWARE_COLOR),
        roughness: 0.55,
        metalness: 0.5,
        transparent: true,
        effect: Effect::Custom {
            name: PICKUP_SURFACE,
            params: vec![0.0],
        },
        ..Material::default()
    })
}

/// The shared face material: the atlas as color and emissive map, glowing faintly
/// and skipping tone mapping so the pictograms stay crisp.
fn face_material() -> Arc<Material> {
    MATERIALS.get_or_insert(1, || Material {
        map: Some(TextureRef {
            wrap: Wrap::Clamp,
            anisotropy: 4,
            ..TextureRef::file(PICKUP_ATLAS_PATH)
        }),
        roughness: 0.55,
        metalness: 0.15,
        emissive: Color(0xffffff),
        emissive_intensity: FACE_EMISSIVE_INTENSITY,
        tone_mapped: false,
        transparent: true,
        effect: Effect::Custom {
            name: PICKUP_SURFACE,
            params: vec![1.0],
        },
        ..Material::default()
    })
}

/// `faceGeometry(kind)`: the crate or cube with every face showing the kind's tile.
fn face_geometry(kind: PickupKind) -> Arc<Mesh> {
    FACES.get_or_insert(kind, || {
        let mut mesh = if is_special_ammo(kind) {
            box_geometry(1.8, 1.05, 1.2)
        } else {
            box_geometry(1.25, 1.25, 1.25)
        };
        for uv in &mut mesh.uvs {
            let [u, v] = pickup_atlas_uv(kind, f64::from(uv[0]), f64::from(uv[1]));
            *uv = [u as f32, v as f32];
        }
        mesh
    })
}

fn caster(mesh: Arc<Mesh>, material: Arc<Material>) -> Node {
    let mut node = Node::mesh(mesh, material);
    if let Some(drawable) = &mut node.drawable {
        drawable.cast_shadow = true;
    }
    node
}

/// `pickupCube(kind)`: a pictogram cube for power-ups, or a group of the crate body
/// and its hardware for ammo. Presentation spins and bobs it (TS `userData.gem`).
pub fn pickup_cube(kind: PickupKind) -> Node {
    let body = caster(face_geometry(kind), face_material());
    if !is_special_ammo(kind) {
        return body;
    }
    let mut crate_group = Node::default();
    crate_group.children.push(body);
    crate_group
        .children
        .push(caster(hardware_geometry(), hardware_material()));
    crate_group
}
