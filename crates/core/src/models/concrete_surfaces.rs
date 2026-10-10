//! Port of `concrete-surfaces.ts`: the weathered concrete shared by every
//! perimeter wall, footing and plinth, with UVs repeating every four metres.

use std::sync::{Arc, OnceLock};

use crate::geometry::{Mesh, rounded_box_geometry};
use crate::scene::{Material, Node, Side, TextureRef};

use super::model_primitives::{Cache, shadowed};

/// World metres per concrete tile.
const CONCRETE_TILE: f64 = 4.0;
const CONCRETE_RADIUS: f64 = 0.06;
pub const CONCRETE_TEXTURE: &str = "textures/walls/weathered-concrete.webp";

static GEOMETRIES: Cache<[u64; 3], Mesh> = Cache::new();

/// The shared concrete material: the photo is both albedo and bump. Walls cast
/// shadows from their sun-facing side (`shadowSide = FrontSide`): the default
/// back-face depth let the bias light a sliver of ground along each wall's
/// shaded foot.
pub fn concrete_material() -> Arc<Material> {
    static MATERIAL: OnceLock<Arc<Material>> = OnceLock::new();
    MATERIAL
        .get_or_init(|| {
            let texture = TextureRef {
                anisotropy: 4,
                ..TextureRef::file(CONCRETE_TEXTURE)
            };
            Arc::new(Material {
                map: Some(texture.clone()),
                bump_map: Some(texture),
                bump_scale: 0.035,
                shadow_side: Some(Side::Front),
                ..Material::standard(0xffffff, 0.0, 0.95)
            })
        })
        .clone()
}

/// `concreteWall(w, h, d)`: a rounded box with a world-sized projection that
/// covers long faces, narrow ends and top surfaces.
pub fn concrete_wall(w: f64, h: f64, d: f64) -> Node {
    let mesh = GEOMETRIES.get_or_insert([w, h, d].map(f64::to_bits), || {
        let mut geometry = rounded_box_geometry(w, h, d, 1, CONCRETE_RADIUS);
        for i in 0..geometry.positions.len() {
            let [x, y, z] = geometry.positions[i].map(f64::from);
            let [nx, ny, nz] = geometry.normals[i].map(|n| f64::from(n).abs());
            let u = if nx > ny && nx > nz { z } else { x };
            let v = if ny >= nx && ny >= nz { z } else { y };
            geometry.uvs[i] = [(u / CONCRETE_TILE) as f32, (v / CONCRETE_TILE) as f32];
        }
        geometry
    });
    shadowed(mesh, concrete_material())
}
