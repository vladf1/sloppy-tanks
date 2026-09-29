//! Port of `barrel-surfaces.ts`: the painted explosive drum.

use super::model_primitives::{Cache, shadowed};
use crate::geometry::{Mesh, cylinder_geometry};
use crate::scene::{Material, Node, TextureRef, Wrap};

/// The drum atlas: the painted wall on the left three quarters, the lid on the right.
pub const PAINTED_DRUM_TEXTURE: &str = "textures/barrels/painted-drum.webp";

static DRUM_MESH: Cache<(), Mesh> = Cache::new();
static DRUM_MATERIAL: Cache<(), Material> = Cache::new();

/// `explosiveBarrel()`: one shared twelve-sided drum (0.6 m radius, 1.6 m tall)
/// whose wall and caps sample separate regions of the atlas.
pub fn explosive_barrel() -> Node {
    let mesh = DRUM_MESH.get_or_insert((), || {
        let mut mesh = cylinder_geometry(0.6, 0.6, 1.6, 12);
        for i in 0..mesh.uvs.len() {
            let [u, v] = mesh.uvs[i].map(f64::from);
            mesh.uvs[i] = if f64::from(mesh.normals[i][1]).abs() > 0.5 {
                [(0.75 + u * 0.25) as f32, (0.25 + v * 0.5) as f32]
            } else {
                [(u * 0.75) as f32, v as f32]
            };
        }
        mesh
    });
    let material = DRUM_MATERIAL.get_or_insert((), || Material {
        map: Some(TextureRef {
            wrap: Wrap::Clamp,
            anisotropy: 4,
            ..TextureRef::file(PAINTED_DRUM_TEXTURE)
        }),
        roughness: 0.82,
        metalness: 0.15,
        ..Material::default()
    });
    shadowed(mesh, material)
}
