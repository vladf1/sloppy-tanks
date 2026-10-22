//! Port of `ground-surfaces.ts`: the dry-grass and packed-dirt ground materials,
//! world-aligned ground UVs and the Pine Village ground, whose shader wears the
//! village roads into the grass.

use std::sync::{Arc, OnceLock};

use crate::geometry::Mesh;
use crate::scene::{Effect, Material, TextureRef, Wrap};
use crate::sim::data::ARENA;

use super::effects_scenery::MEADOW_SWAY;
use super::village_ground::VILLAGE_GROUND_TEXTURE;
use super::village_roads::village_roads;

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

/// The Pine Village ground shared by the arena floor and the valley: the
/// [`MEADOW_SWAY`] effect's ground mode paints lush grass from the dry-grass tile
/// (at [`ground_uvs`]) and wears the village roads into it from the packed-dirt
/// tile, with ragged grass edges, wheel ruts, pebbles and wildflowers. The roads
/// reach the shader as data from [`village_roads`], so its ruts follow the strips
/// the track dust ([`super::village_roads::is_village_dirt`]) expects; the worn
/// dirt itself, the cottage gardens and the fields outside the wall come from the
/// baked plan ([`super::village_ground`]). Vertex colours multiply the grass
/// (creek banks, far hills).
pub fn village_ground_material() -> Arc<Material> {
    static MATERIAL: OnceLock<Arc<Material>> = OnceLock::new();
    MATERIAL
        .get_or_init(|| {
            let roads = village_roads();
            let (along, across) = roads.split_at(3);
            // The shader knows one width for the outer strips of each family and
            // one for the centre strip.
            debug_assert!(along[0].w == along[2].w && across[0].d == across[2].d);
            let params = [
                [1.0, ARENA, 0.0, 0.0],
                [along[0].x, along[1].x, along[2].x, 0.0],
                [across[0].z, across[1].z, across[2].z, 0.0],
                [
                    along[0].w / 2.0,
                    along[1].w / 2.0,
                    across[0].d / 2.0,
                    across[1].d / 2.0,
                ],
            ]
            .concat()
            .into_iter()
            .map(|value| value as f32)
            .collect();
            let grass = TextureRef {
                wrap: Wrap::Mirror,
                anisotropy: GROUND_ANISOTROPY,
                ..TextureRef::file("textures/ground/dry-grass.webp")
            };
            let dirt = TextureRef {
                anisotropy: GROUND_ANISOTROPY,
                ..TextureRef::file("textures/ground/packed-dirt.webp")
            };
            // Data, not colour: linear, stored rows from the most negative z.
            let plan = TextureRef {
                wrap: Wrap::Clamp,
                srgb: false,
                flip_y: false,
                anisotropy: GROUND_ANISOTROPY,
                ..TextureRef::file(VILLAGE_GROUND_TEXTURE)
            };
            Arc::new(Material {
                map: Some(grass),
                extra_textures: vec![("dirt", dirt), ("plan", plan)],
                vertex_colors: true,
                effect: Effect::Custom {
                    name: MEADOW_SWAY,
                    params,
                },
                ..Material::standard(0xffffff, 0.0, 0.95)
            })
        })
        .clone()
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
