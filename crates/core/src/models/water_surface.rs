//! Port of the mesh and material description of `water-surface.ts`: the village
//! creek and the harbor basin. The renderer owns the planar reflection and the
//! ripple shader; see [`effects_scenery::WATER`] for the parameters and semantics.

use std::sync::Arc;

use glam::DVec3;

use crate::geometry::Mesh;
use crate::geometry::math::{hex_to_linear, normalize};
use crate::scene::{Effect, Material, Node, TextureRef, Wrap};

use super::effects_scenery::WATER;

/// The shared ripple normal tile (linear data, repeat wrapping, 4x anisotropy).
pub const WATER_NORMALS: &str = "textures/water/normals.webp";

/// Which water body a surface is; they differ only in tuning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WaterKind {
    Creek,
    Harbor,
}

pub fn water_normals() -> TextureRef {
    TextureRef {
        wrap: Wrap::Repeat,
        srgb: false,
        anisotropy: 4,
        ..TextureRef::file(WATER_NORMALS)
    }
}

/// The unlit water material: `map` is the ripple normal tile (read only by the
/// effect), and `params` follow the order documented on [`WATER`].
pub fn water_material(kind: WaterKind, height: f64) -> Material {
    let harbor = kind == WaterKind::Harbor;
    let sun = normalize(DVec3::new(-45.0, if harbor { 55.0 } else { 68.0 }, 25.0));
    let rgb = |hex: u32| hex_to_linear(hex).map(|c| c as f32);
    let mut params = vec![
        if harbor { 1.0 } else { 0.0 },
        height as f32,
        if harbor { 1.8 } else { 0.65 },
        if harbor { 4.0 } else { 7.0 },
        if harbor { 1.3 } else { 0.75 },
        sun.x as f32,
        sun.y as f32,
        sun.z as f32,
    ];
    params.extend(rgb(if harbor { 0xffdcc0 } else { 0xffebce }));
    params.extend(rgb(if harbor { 0x164956 } else { 0x244b3f }));
    params.extend(rgb(if harbor { 0x3a807d } else { 0x638466 }));
    params.extend([
        if harbor { 0.7 } else { 0.65 },
        REFLECTION_SIZE,
        REFLECTION_SAMPLES,
        CALM_EXTENT,
    ]);
    Material {
        map: Some(water_normals()),
        effect: Effect::Custom {
            name: WATER,
            params,
        },
        ..Material::basic(0xffffff)
    }
}

/// Fixed reflection target size, matching the original Water renderer.
const REFLECTION_SIZE: f32 = 512.0;
/// Reflection target MSAA samples (the main view's 4x, so pipelines are shared).
const REFLECTION_SAMPLES: f32 = 4.0;
/// Half-extent of the square (the harbor apron) that hides water: the reflection
/// is skipped when all four view-corner rays land inside it.
const CALM_EXTENT: f32 = 62.0;

/// `new WaterSurface(geometry, kind, height)`: the geometry is authored in local XY
/// and the node turns it onto the horizontal mirror plane at `height`.
pub fn water_surface(geometry: Mesh, kind: WaterKind, height: f64) -> Node {
    let mut node = Node::mesh(Arc::new(geometry), Arc::new(water_material(kind, height)));
    node.name = match kind {
        WaterKind::Harbor => "harbor-water",
        WaterKind::Creek => "village-creek",
    }
    .into();
    node.set_rotation_euler(-std::f64::consts::FRAC_PI_2, 0.0, 0.0);
    node.position.y = height;
    node
}
