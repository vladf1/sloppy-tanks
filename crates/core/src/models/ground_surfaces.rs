//! Port of `ground-surfaces.ts`: the dry-grass and packed-dirt ground materials,
//! world-aligned ground UVs and the feathered village road strips.

use crate::geometry::{Attribute, Mesh, VERTEX_ALPHA};
use crate::scene::{Material, TextureRef, Wrap};

use super::village_roads::ROAD_SHOULDER;

pub use crate::sim::maps::GroundKind;

/// World metres per ground texture tile.
const GROUND_TILE: f64 = 8.0;
/// Ground maps ask for 4x anisotropy (`Math.min(4, renderer.getMaxAnisotropy())`);
/// the renderer clamps to the adapter's limit.
const GROUND_ANISOTROPY: u8 = 4;

/// `groundMaterial(renderer, kind)`: a new (unshared) material, since callers tint
/// or blend it; the texture is shared by value. The albedo tile mirrors, which
/// joins the generated edges without relying on perfect AI tiling; mipmaps keep
/// distant ground stable and cheap.
pub fn ground_material(kind: GroundKind) -> Material {
    let (path, color) = match kind {
        GroundKind::DryGrass => ("textures/ground/dry-grass.webp", 0xe2e8d5),
        GroundKind::PackedDirt => ("textures/ground/packed-dirt.webp", 0xe5dbcc),
    };
    let map = TextureRef {
        wrap: Wrap::Mirror,
        anisotropy: GROUND_ANISOTROPY,
        ..TextureRef::file(path)
    };
    Material {
        map: Some(map),
        ..Material::standard(color, 0.0, 1.0)
    }
}

/// `groundUVs(geometry, x, z)`: one tile per eight world metres, aligned across roads
/// and intersections. `x`/`z` is the mesh's world offset.
pub fn ground_uvs(mesh: &mut Mesh, x: f64, z: f64) {
    mesh.uvs = mesh
        .positions
        .iter()
        .map(|p| {
            [
                ((f64::from(p[0]) + x) / GROUND_TILE) as f32,
                ((f64::from(p[2]) + z) / GROUND_TILE) as f32,
            ]
        })
        .collect();
}

/// `roadGeometry(w, d, x, z)`: a 4x4 grid whose outer ring fades to transparent
/// (narrow alpha shoulders soften road borders; the opaque center stays flat).
/// Colors are white; the RGBA color's alpha is the [`VERTEX_ALPHA`] attribute.
pub fn road_geometry(w: f64, d: f64, x: f64, z: f64) -> Mesh {
    let shoulder = ROAD_SHOULDER;
    let xs = [-w / 2.0, -w / 2.0 + shoulder, w / 2.0 - shoulder, w / 2.0];
    let zs = [-d / 2.0, -d / 2.0 + shoulder, d / 2.0 - shoulder, d / 2.0];
    let mut positions = Vec::with_capacity(48);
    let mut uvs = Vec::with_capacity(32);
    let mut alpha = Vec::with_capacity(16);
    for (row, &pz) in zs.iter().enumerate() {
        for (col, &px) in xs.iter().enumerate() {
            positions.extend([px, 0.0, pz]);
            uvs.extend([(px + x) / GROUND_TILE, (pz + z) / GROUND_TILE]);
            let edge = row == 0 || row == 3 || col == 0 || col == 3;
            alpha.push(if edge { 0.0 } else { 1.0 });
        }
    }
    let mut indices = Vec::with_capacity(54);
    for row in 0..3 {
        for col in 0..3 {
            let i = row * 4 + col;
            indices.extend([i, i + 4, i + 1, i + 1, i + 4, i + 5]);
        }
    }
    let mut mesh = Mesh::from_f64(&positions, &[], &uvs, Some(indices));
    mesh.colors = vec![[1.0; 3]; 16];
    mesh.set_attribute(Attribute::vertex(VERTEX_ALPHA, 1, alpha));
    mesh.compute_vertex_normals();
    mesh
}
