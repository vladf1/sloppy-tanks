//! Port of `quarry-terrain.ts`: Dusty Dig's floor mesh, its baked soil texture
//! and the drift of windblown sand against cover. The soil pixels are baked in
//! Rust ([`super::quarry_soil`]); the renderer shades soil surfaces with
//! [`effects_scenery::QUARRY_SOIL`].

use std::sync::{Arc, OnceLock};

use crate::geometry::{Mesh, plane_geometry_segments};
use crate::scene::{Effect, Material, Node, TextureRef, TextureSource, Wrap};

use super::effects_scenery::{QUARRY_SOIL, QUARRY_SOIL_TEXTURE};
use super::quarry_soil::{ACCUM_CELLS, QUARRY_TERRAIN_EXTENT};
use crate::geometry::math::js_hypot;
use crate::sim::math::Random;
use crate::sim::quarry_layout::quarry_layout;
use crate::sim::types::CoverKind;

const EXTENT: f64 = QUARRY_TERRAIN_EXTENT;
/// Grid resolution of the floor mesh (1.5 m cells).
const FLOOR_SEGMENTS: u32 = 140;
/// Node name of the quarry floor.
pub const QUARRY_FLOOR: &str = "quarry-compacted-haul-roads";

/// `sandAccum()`: windblown sand piled against cover, splatted once per layout
/// into an `ACCUM_CELLS`² grid (f32, like the Float32Array) and sampled per pixel.
pub fn sand_accum() -> &'static [f32] {
    static GRID: OnceLock<Vec<f32>> = OnceLock::new();
    GRID.get_or_init(|| {
        let mut grid = vec![0.0f32; ACCUM_CELLS * ACCUM_CELLS];
        let mut rng = Random::new(514.0);
        let cell = EXTENT / (ACCUM_CELLS - 1) as f64;
        let last = (ACCUM_CELLS - 1) as f64;
        let mut splat = |x: f64, z: f64, radius: f64, strength: f64| {
            let cx = (x + EXTENT / 2.0) / cell;
            let cz = (z + EXTENT / 2.0) / cell;
            let r = radius / cell;
            let i0 = 0.0f64.max((cx - r).floor()) as usize;
            let i1 = last.min((cx + r).ceil()) as usize;
            let j0 = 0.0f64.max((cz - r).floor()) as usize;
            let j1 = last.min((cz + r).ceil()) as usize;
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let d = js_hypot(&[i as f64 - cx, j as f64 - cz]) / r;
                    if d < 1.0 {
                        let fall = (1.0 - d * d) * (1.0 - d * d);
                        let at = j * ACCUM_CELLS + i;
                        grid[at] = (f64::from(grid[at]) + strength * fall) as f32;
                    }
                }
            }
        };
        for cover in quarry_layout() {
            if cover.kind == CoverKind::Boundary {
                continue;
            }
            // Overlapping blobs biased downwind (+x) read as drift, not stamped circles.
            let base = cover.w.max(cover.d) * 0.5 + 1.4;
            for _ in 0..3 {
                let x = cover.x + rng.range(-1.2, 2.6);
                let z = cover.z + rng.range(-2.2, 2.2);
                let radius = base * rng.range(0.7, 1.15);
                splat(x, z, radius, 0.3);
            }
        }
        grid
    })
}

/// The baked work-yard soil ([`QUARRY_SOIL_TEXTURE`], pixels from
/// `quarry_soil::quarry_soil_pixels` or banded `bake_quarry_soil`): sRGB color,
/// alpha = grittiness, clamped, mipmapped, 8x anisotropy.
pub fn quarry_soil_texture() -> TextureRef {
    TextureRef {
        source: TextureSource::Generated(QUARRY_SOIL_TEXTURE),
        wrap: Wrap::Clamp,
        anisotropy: 8,
        ..TextureRef::file("")
    }
}

/// `soilMaterial()`: baked soil (sampled at the mesh UVs, which map world x/z onto
/// the bake) times world-space grit, with grit relief. Soil meshes all carry vertex
/// colors, so plain soil shares its shader with the tinted spoil and ramp.
pub fn soil_material() -> Material {
    Material {
        map: Some(quarry_soil_texture()),
        vertex_colors: true,
        effect: Effect::Custom {
            name: QUARRY_SOIL,
            params: Vec::new(),
        },
        ..Material::standard(0xffffff, 0.0, 1.0)
    }
}

/// `plainSoilColors(geometry)`: white vertex colors, leaving the baked soil unchanged.
pub fn plain_soil_colors(mesh: &mut Mesh) {
    mesh.colors = vec![[1.0; 3]; mesh.positions.len()];
}

/// `quarryTerrain()`: the metre-scaled work yard floor, flat inside the wall and
/// falling 0.3 per metre to the machinery apron 1.8 m below. Generated once for the
/// retained scenery, never during round reset or rendering.
pub fn quarry_terrain() -> Node {
    let mut geometry = plane_geometry_segments(EXTENT, EXTENT, FLOOR_SEGMENTS, FLOOR_SEGMENTS);
    geometry.rotate_x(-std::f64::consts::FRAC_PI_2);
    for p in &mut geometry.positions {
        let outside = f64::from(p[0]).abs().max(f64::from(p[2]).abs()) - 60.0;
        p[1] = if outside > 0.0 {
            (-(1.8f64.min(outside * 0.3))) as f32
        } else {
            0.0
        };
    }
    geometry.compute_vertex_normals();
    plain_soil_colors(&mut geometry);
    let mut floor = Node::mesh(Arc::new(geometry), Arc::new(soil_material()));
    floor.name = QUARRY_FLOOR.into();
    floor.position.y = 0.008;
    if let Some(drawable) = &mut floor.drawable {
        drawable.receive_shadow = true;
    }
    floor
}
