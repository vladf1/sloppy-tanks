//! Port of `harbor-surfaces.ts`: dock concrete and painted steel with real-world
//! UV scale, shared by harbor scenery, containers and quarry machinery.

use std::sync::Arc;

use crate::geometry::{Mesh, box_geometry};
use crate::scene::{Material, Node, TextureRef, Wrap};

use super::model_primitives::{Cache, shadowed};

/// The two harbor surface tiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HarborSurface {
    Dock,
    #[default]
    Steel,
}

impl HarborSurface {
    pub fn texture_path(self) -> &'static str {
        match self {
            HarborSurface::Dock => "textures/harbor/dock.webp",
            HarborSurface::Steel => "textures/harbor/steel.webp",
        }
    }

    /// World metres per tile.
    fn tile(self) -> f64 {
        match self {
            HarborSurface::Dock => 10.0,
            HarborSurface::Steel => 4.0,
        }
    }
}

static MATERIALS: Cache<(HarborSurface, u32), Material> = Cache::new();
static GEOMETRIES: Cache<(HarborSurface, [u64; 3]), Mesh> = Cache::new();

/// `harborMaterial(kind, color)`: the tile as albedo and bump.
pub fn harbor_material(kind: HarborSurface, color: u32) -> Arc<Material> {
    MATERIALS.get_or_insert((kind, color), || {
        let texture = TextureRef {
            wrap: Wrap::Mirror,
            anisotropy: 4,
            ..TextureRef::file(kind.texture_path())
        };
        let (bump_scale, metalness, roughness) = match kind {
            HarborSurface::Dock => (0.035, 0.0, 0.95),
            HarborSurface::Steel => (0.012, 0.3, 0.68),
        };
        Material {
            map: Some(texture.clone()),
            bump_map: Some(texture),
            bump_scale,
            ..Material::standard(color, metalness, roughness)
        }
    })
}

/// `harborBox(w, h, d, color, kind = "steel")`: a box with planar world-scale UVs
/// chosen by each face's normal.
pub fn harbor_box(w: f64, h: f64, d: f64, color: u32, kind: HarborSurface) -> Node {
    let mesh = GEOMETRIES.get_or_insert((kind, [w, h, d].map(f64::to_bits)), || {
        let mut geometry = box_geometry(w, h, d);
        let tile = kind.tile();
        for i in 0..geometry.positions.len() {
            let [x, y, z] = geometry.positions[i].map(f64::from);
            let [nx, ny, _] = geometry.normals[i].map(f64::from);
            let u = if nx.abs() > 0.5 { z } else { x };
            let v = if ny.abs() > 0.5 { z } else { y };
            geometry.uvs[i] = [(u / tile) as f32, (v / tile) as f32];
        }
        geometry
    });
    shadowed(mesh, harbor_material(kind, color))
}

/// `harborBox(w, h, d, color)` in painted steel.
pub fn steel_box(w: f64, h: f64, d: f64, color: u32) -> Node {
    harbor_box(w, h, d, color, HarborSurface::Steel)
}
