//! Port of `house-surfaces.ts`: board siding and shingle materials with world-scale
//! UVs, shared by village houses, the watermill, crates and timber props.
//!
//! The tiles were once DataTextures and keep that row order (`flipY = false`).
//! `TextureRef` has no flip flag, so the maps express it as the equivalent UV
//! transform `v' = 1 - v` (repeat `[1, -1]`, offset `[0, 1]`); with repeat wrapping
//! this samples exactly what the unflipped upload did.

use std::sync::Arc;

use crate::geometry::{ExtrudeOptions, Mesh, Path, Shape, box_geometry, extrude_geometry};
use crate::scene::{Material, Node, Side, TextureRef, Wrap};

use super::model_primitives::{Cache, shadowed};
use super::scenery::js_hypot;

/// World metres per siding or shingle tile.
const TILE_METRES: f64 = 2.56;
const SURFACE_ANISOTROPY: u8 = 4;
const SURFACE_ROUGHNESS: f32 = 0.88;

/// The two house surface tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HouseSurface {
    Siding,
    Shingles,
}

impl HouseSurface {
    fn texture_path(self) -> &'static str {
        match self {
            HouseSurface::Siding => "textures/houses/siding.webp",
            HouseSurface::Shingles => "textures/houses/shingles.webp",
        }
    }

    fn bump_scale(self) -> f32 {
        match self {
            HouseSurface::Siding => 0.02,
            HouseSurface::Shingles => 0.035,
        }
    }
}

/// The shared siding or shingle tile, sampled unflipped (see the module docs).
pub fn house_texture(kind: HouseSurface) -> TextureRef {
    TextureRef {
        wrap: Wrap::Repeat,
        repeat: [1.0, -1.0],
        offset: [0.0, 1.0],
        anisotropy: SURFACE_ANISOTROPY,
        ..TextureRef::file(kind.texture_path())
    }
}

static MATERIALS: Cache<(HouseSurface, u32), Material> = Cache::new();
static GEOMETRIES: Cache<(u8, [u64; 3]), Mesh> = Cache::new();

/// `surfaceMaterial(kind, color)`: the tile as both albedo and bump, double-sided.
pub fn house_material(kind: HouseSurface, color: u32) -> Arc<Material> {
    MATERIALS.get_or_insert((kind, color), || {
        let texture = house_texture(kind);
        Material {
            map: Some(texture.clone()),
            bump_map: Some(texture),
            bump_scale: kind.bump_scale(),
            side: Side::Double,
            ..Material::standard(color, 0.0, SURFACE_ROUGHNESS)
        }
    })
}

fn size_key(kind: u8, w: f64, h: f64, d: f64) -> (u8, [u64; 3]) {
    (kind, [w, h, d].map(f64::to_bits))
}

/// `sidingBox(w, h, d, color)`: a box whose UVs repeat every tile on each face.
pub fn siding_box(w: f64, h: f64, d: f64, color: u32) -> Node {
    let mesh = GEOMETRIES.get_or_insert(size_key(0, w, h, d), || {
        let mut geometry = box_geometry(w, h, d);
        for (i, uv) in geometry.uvs.iter_mut().enumerate() {
            let face = i / 4;
            let across = if face < 2 { d } else { w };
            let up = if face == 2 || face == 3 { d } else { h };
            *uv = [
                (f64::from(uv[0]) * across / TILE_METRES) as f32,
                (f64::from(uv[1]) * up / TILE_METRES) as f32,
            ];
        }
        geometry
    });
    shadowed(mesh, house_material(HouseSurface::Siding, color))
}

/// `sidingGable(w, h, d, color)`: triangular roof body with horizontal boards at
/// the same scale as the walls.
pub fn siding_gable(w: f64, h: f64, d: f64, color: u32) -> Node {
    let mesh = GEOMETRIES.get_or_insert(size_key(1, w, h, d), || {
        let mut profile = Path::new();
        profile
            .move_to(-w / 2.0, 0.0)
            .line_to(0.0, h)
            .line_to(w / 2.0, 0.0)
            .close_path();
        let mut geometry = extrude_geometry(
            &[Shape::new(profile)],
            &ExtrudeOptions {
                depth: d,
                bevel_enabled: false,
                ..ExtrudeOptions::default()
            },
        );
        geometry.translate(0.0, 0.0, -d / 2.0);
        geometry.uvs = geometry
            .positions
            .iter()
            .map(|p| {
                [
                    ((f64::from(p[0]) + w / 2.0) / TILE_METRES) as f32,
                    (f64::from(p[1]) / TILE_METRES) as f32,
                ]
            })
            .collect();
        geometry
    });
    shadowed(mesh, house_material(HouseSurface::Siding, color))
}

/// `shingleRoof(w, h, d, color)`: two sloped shingle planes, 1.8 cm above the
/// gable so they never z-fight.
pub fn shingle_roof(w: f64, h: f64, d: f64, color: u32) -> Node {
    let mesh = GEOMETRIES.get_or_insert(size_key(2, w, h, d), || {
        let mut positions = Vec::with_capacity(36);
        let mut uvs = Vec::with_capacity(24);
        let slope = js_hypot(&[w / 2.0, h]);
        for side in [-1.0, 1.0] {
            let points = [
                [0.0, h + 0.018, -d / 2.0],
                [(side * w) / 2.0, 0.018, -d / 2.0],
                [(side * w) / 2.0, 0.018, d / 2.0],
                [0.0, h + 0.018, d / 2.0],
            ];
            let coords = [[0.0, slope], [0.0, 0.0], [d, 0.0], [d, slope]];
            for i in [0, 1, 2, 0, 2, 3] {
                positions.extend(points[i]);
                uvs.extend([coords[i][0] / TILE_METRES, coords[i][1] / TILE_METRES]);
            }
        }
        let mut geometry = Mesh::from_f64(&positions, &[], &uvs, None);
        geometry.compute_vertex_normals();
        geometry
    });
    shadowed(mesh, house_material(HouseSurface::Shingles, color))
}
