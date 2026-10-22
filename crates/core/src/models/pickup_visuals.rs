//! Ports of `pickup-visuals.ts` and `pickup-atlas.ts`: pickup cubes and ammo crates
//! with pictograms from one atlas.
//!
//! Pickup materials are opaque; first person and a recharging pad's ghost fade
//! them through the instance opacity, whose blended variant warm-up compiles.

use std::sync::Arc;

use super::model_primitives::Cache;
use crate::geometry::{Mesh, box_geometry, merge_geometries};
use crate::scene::{Color, Effect, Material, Node, TextureRef, Wrap};
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

/// Where `kind`'s tile sits relative to the first tile, in atlas UV: the faces are
/// mapped to the first tile and the `pickup-face` effect adds this offset, passed
/// as the instance data's `xy`, so every kind shares one mesh and material and
/// all pickups of a shape draw as one instanced call.
pub fn pickup_tile_offset(kind: PickupKind) -> [f32; 2] {
    let (column, row) = pickup_atlas_tile(kind);
    [
        (f64::from(column) * PICKUP_ATLAS_STRIDE / PICKUP_ATLAS_SIZE) as f32,
        (-f64::from(row) * PICKUP_ATLAS_STRIDE / PICKUP_ATLAS_SIZE) as f32,
    ]
}

/// The vertex effect that moves a face's UVs into the instance's atlas tile
/// (`crates/render/src/presentation/shaders/pickup_face.wgsl`).
pub const PICKUP_FACE_EFFECT: &str = "pickup-face";
/// The frame node: corner caps and crate rims, hidden on a recharging pad's ghost.
pub const PICKUP_FRAME_NODE: &str = "pickup-frame";

/// Gunmetal of the frame: corner caps and crate rims, lighter than the panels'
/// painted bevel so the hard case reads as metal around lit glass.
const FRAME_COLOR: u32 = 0x7c8894;
const FACE_EMISSIVE_INTENSITY: f32 = 0.38;
/// The power-up cube's half size, and its corner caps: their size and how far
/// they stand proud of the faces.
const CUBE_HALF: f64 = 0.625;
const CAP_SIZE: f64 = 0.27;
const CAP_PROUD: f64 = 0.03;

static SHARED: Cache<u8, Mesh> = Cache::new();
static MATERIALS: Cache<u8, Material> = Cache::new();

fn boxes(parts: &[([f64; 3], [f64; 3])]) -> Mesh {
    let meshes: Vec<Mesh> = parts
        .iter()
        .map(|&([w, h, d], [x, y, z])| {
            let mut mesh = box_geometry(w, h, d);
            mesh.translate(x, y, z);
            mesh
        })
        .collect();
    merge_geometries(&meshes.iter().collect::<Vec<_>>()).expect("boxes share one layout")
}

/// Eight cubes at the corners of a box of half extents `half`, each `size` wide
/// and standing `proud` out of every face it touches: a rugged hard case's caps.
fn corner_caps(half: [f64; 3], size: f64, proud: f64) -> Vec<([f64; 3], [f64; 3])> {
    let mut caps = Vec::with_capacity(8);
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let at = |h: f64, s: f64| s * (h - size / 2.0 + proud);
                caps.push((
                    [size; 3],
                    [at(half[0], sx), at(half[1], sy), at(half[2], sz)],
                ));
            }
        }
    }
    caps
}

/// The power-up cube's corner caps.
fn cube_frame_geometry() -> Arc<Mesh> {
    SHARED.get_or_insert(1, || {
        boxes(&corner_caps([CUBE_HALF; 3], CAP_SIZE, CAP_PROUD))
    })
}

/// The steel rim, base, latch and corner caps of an ammo crate: a raised rim
/// leaves the top-face symbol visible from the overhead camera.
fn crate_frame_geometry() -> Arc<Mesh> {
    SHARED.get_or_insert(0, || {
        let mut parts = vec![
            ([1.94, 0.14, 0.09], [0.0, 0.49, -0.615]),
            ([1.94, 0.14, 0.09], [0.0, 0.49, 0.615]),
            ([0.09, 0.14, 1.14], [-0.925, 0.49, 0.0]),
            ([0.09, 0.14, 1.14], [0.925, 0.49, 0.0]),
            ([1.9, 0.1, 1.28], [0.0, -0.5, 0.0]),
            ([0.2, 0.28, 0.08], [0.0, 0.38, 0.64]),
        ];
        parts.extend(corner_caps([0.9, 0.525, 0.6], 0.24, CAP_PROUD));
        boxes(&parts)
    })
}

fn frame_material() -> Arc<Material> {
    MATERIALS.get_or_insert(0, || Material {
        color: Color(FRAME_COLOR),
        roughness: 0.38,
        metalness: 0.55,
        ..Material::default()
    })
}

/// The shared face material: the atlas as color and emissive map, its panels
/// glowing and skipping tone mapping so the pictograms stay crisp. Opaque, so
/// every pickup of a shape batches; first person and a recharging ghost fade
/// through the instance opacity's blended variant, which warm-up compiles.
fn face_material() -> Arc<Material> {
    MATERIALS.get_or_insert(1, || {
        let atlas = TextureRef {
            wrap: Wrap::Clamp,
            anisotropy: 4,
            ..TextureRef::file(PICKUP_ATLAS_PATH)
        };
        Material {
            map: Some(atlas.clone()),
            emissive_map: Some(atlas),
            roughness: 0.45,
            metalness: 0.15,
            emissive: Color(0xffffff),
            emissive_intensity: FACE_EMISSIVE_INTENSITY,
            tone_mapped: false,
            effect: Effect::Custom {
                name: PICKUP_FACE_EFFECT,
                params: Vec::new(),
            },
            ..Material::default()
        }
    })
}

/// The crate (ammo) or cube (power-up) with every face mapped to the first tile.
fn face_geometry(ammo: bool) -> Arc<Mesh> {
    SHARED.get_or_insert(if ammo { 2 } else { 3 }, || {
        let mut mesh = if ammo {
            box_geometry(1.8, 1.05, 1.2)
        } else {
            box_geometry(CUBE_HALF * 2.0, CUBE_HALF * 2.0, CUBE_HALF * 2.0)
        };
        for uv in &mut mesh.uvs {
            let [u, v] = pickup_atlas_uv(PickupKind::Spread, f64::from(uv[0]), f64::from(uv[1]));
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

/// `pickupCube(kind)`: a pictogram hard case, a cube for power-ups or a wide crate
/// for ammo, with its gunmetal frame. Every kind of a shape shares it; the
/// instance data's `xy` ([`pickup_tile_offset`]) picks the pictogram.
/// Presentation spins and bobs it (TS `userData.gem`).
pub fn pickup_cube(kind: PickupKind) -> Node {
    let ammo = kind.special_ammo().is_some();
    let frame = if ammo {
        crate_frame_geometry()
    } else {
        cube_frame_geometry()
    };
    let mut group = Node::default();
    group
        .children
        .push(caster(face_geometry(ammo), face_material()));
    let mut frame = caster(frame, frame_material());
    frame.name = PICKUP_FRAME_NODE.into();
    group.children.push(frame);
    group
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tile_offset_moves_the_first_tile_onto_each_kinds_tile() {
        for kind in PickupKind::ALL {
            let [du, dv] = pickup_tile_offset(kind);
            for (u, v) in [(0.0, 0.0), (1.0, 1.0), (0.3, 0.8)] {
                let [u0, v0] = pickup_atlas_uv(PickupKind::Spread, u, v);
                let [u1, v1] = pickup_atlas_uv(kind, u, v);
                assert!((u0 + f64::from(du) - u1).abs() < 1e-6, "{kind:?}");
                assert!((v0 + f64::from(dv) - v1).abs() < 1e-6, "{kind:?}");
            }
        }
    }
}
