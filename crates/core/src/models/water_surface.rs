//! Port of the mesh and material description of `water-surface.ts`: the village
//! creek and the harbor basin. The renderer owns the planar reflection, the ripple
//! shader and their tuning; see [`effects_scenery::WATER`].

use std::sync::Arc;

use crate::geometry::Mesh;
use crate::scene::{Effect, Material, Node, TextureRef};

use super::effects_scenery::WATER;

/// The shared ripple normal tile (linear data, repeat wrapping, 4x anisotropy).
pub const WATER_NORMALS: &str = "textures/water/normals.webp";

/// Which water body a surface is; it names the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WaterKind {
    Creek,
    Harbor,
}

pub fn water_normals() -> TextureRef {
    TextureRef {
        srgb: false,
        anisotropy: 4,
        ..TextureRef::file(WATER_NORMALS)
    }
}

/// The unlit water material: `map` is the ripple normal tile (read only by the
/// renderer's water pass), and the one param is the mirror height ([`WATER`]).
fn water_material(height: f64) -> Material {
    Material {
        map: Some(water_normals()),
        effect: Effect::Custom {
            name: WATER,
            params: vec![height as f32],
        },
        ..Material::basic(0xffffff)
    }
}

/// `new WaterSurface(geometry, kind, height)`: the geometry is authored in local XY
/// and the node turns it onto the horizontal mirror plane at `height`.
pub fn water_surface(geometry: Mesh, kind: WaterKind, height: f64) -> Node {
    let mut node = Node::mesh(Arc::new(geometry), Arc::new(water_material(height)));
    node.name = match kind {
        WaterKind::Harbor => "harbor-water",
        WaterKind::Creek => "village-creek",
    }
    .into();
    node.set_rotation_euler(-std::f64::consts::FRAC_PI_2, 0.0, 0.0);
    node.position.y = height;
    node
}
