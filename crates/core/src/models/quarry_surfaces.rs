//! Port of `quarry-surfaces.ts`: the layered sandstone material, ledge rocks,
//! merged rubble and the feathered sand footings around rock cover. The shading
//! is the renderer's ([`effects_scenery::SANDSTONE`], [`effects_scenery::SAND_DRIFT`]).

use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};

use glam::DVec3;

use crate::geometry::math::{compose, js_sign, normalize, quat_from_euler, transform_point};
use crate::geometry::{Attribute, Mesh, icosahedron_geometry, widen};
use crate::scene::{Effect, Material, Node, TextureRef, Wrap};

use super::effects_scenery::{SAND_DRIFT, SANDSTONE, VERTEX_ALPHA};
use super::model_primitives::{Cache, shadowed};
use super::quarry_terrain::quarry_soil_texture;
use crate::sim::math::Random;
use crate::sim::quarry_rock_shape::quarry_rock_shape;

pub const SANDSTONE_TEXTURE: &str = "textures/quarry/sandstone.webp";
/// World metres per rock bump-UV tile.
const ROCK_UV_TILE: f64 = 5.5;
/// Normals crease above this angle, keeping ledges sharp and shoulders round.
const ROCK_CREASE: f64 = PI / 5.0;

/// `sandstoneMaterial()`: one shared material for every rock, bench and rubble
/// mesh. `map` is the sandstone photo the effect samples triplanar in world space.
pub fn sandstone_material() -> Arc<Material> {
    static MATERIAL: OnceLock<Arc<Material>> = OnceLock::new();
    MATERIAL
        .get_or_init(|| {
            Arc::new(Material {
                map: Some(TextureRef {
                    wrap: Wrap::Mirror,
                    anisotropy: 4,
                    ..TextureRef::file(SANDSTONE_TEXTURE)
                }),
                vertex_colors: true,
                effect: Effect::Custom {
                    name: SANDSTONE,
                    params: Vec::new(),
                },
                ..Material::standard(0xffffff, 0.0, 0.95)
            })
        })
        .clone()
}

static ROCKS: Cache<([u64; 3], u32), Mesh> = Cache::new();
static FOOTINGS: Cache<([u64; 2], u32), Mesh> = Cache::new();

/// `sandstoneRock(w, h, d, variant)`: a low-polygon ledge rock sharing its exact
/// shape with collision, flat-faceted with creased normals and per-face shade.
pub fn sandstone_rock(w: f64, h: f64, d: f64, variant: u32) -> Node {
    let key = ([w, h, d].map(f64::to_bits), variant);
    let mesh = ROCKS.get_or_insert(key, || {
        let mut rng = Random::new(812.0 + f64::from(variant));
        let shape = quarry_rock_shape(w, h, d, variant);
        let vertices: Vec<DVec3> = shape
            .positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| widen(*p))
            .collect();
        let mut positions = Vec::with_capacity(shape.indices.len() * 3);
        let mut uvs = Vec::with_capacity(shape.indices.len() * 2);
        let mut colors = Vec::with_capacity(shape.indices.len());
        for triangle in shape.indices.as_chunks::<3>().0 {
            let shade = rng.range(0.9, 1.07);
            let warm = rng.range(-0.02, 0.035);
            let [a, b, c] = triangle.map(|i| vertices[i as usize]);
            let normal = normalize((b - a).cross(c - a));
            // Dominant-axis projection prevents diagonal faces collapsing into streaks.
            // These UVs feed only the bump map; albedo uses world-space triplanar.
            let top = normal.y.abs() > 0.65;
            let along_z = normal.x.abs() > normal.z.abs();
            for p in [a, b, c] {
                positions.extend([p.x, p.y, p.z]);
                uvs.extend([
                    (if along_z && !top { p.z } else { p.x }) / ROCK_UV_TILE,
                    (if top { p.z } else { p.y }) / ROCK_UV_TILE,
                ]);
                let dust = if top {
                    1.07
                } else {
                    0.88 + 0.12 * (p.y / h).min(1.0)
                };
                colors.push([
                    (shade * dust * (1.0 + warm)) as f32,
                    (shade * dust) as f32,
                    (shade * dust * (1.0 - warm)) as f32,
                ]);
            }
        }
        let mut geometry = Mesh::from_f64(&positions, &[], &uvs, None);
        geometry.colors = colors;
        geometry.to_creased_normals(ROCK_CREASE)
    });
    shadowed(mesh, sandstone_material())
}

/// One fragment of [`sandstone_rubble`], in world metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RubbleStone {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Footprint and height, in metres.
    pub w: f64,
    pub h: f64,
    pub d: f64,
    pub rot_y: f64,
    /// Per-stone brightness; hue comes from the shared sandstone strata.
    pub shade: f64,
}

/// `roughenStone(vertex, stone)`: push a unit stone's corner in or out by a
/// per-stone hash of that corner, so shared corners stay welded and no two stones
/// read as dice.
pub fn roughen_stone(vertex: DVec3, stone: f64) -> DVec3 {
    let hash = (stone * 12.9898 + vertex.x * 78.233 + vertex.y * 37.719 + vertex.z * 11.131).sin();
    let r = hash * 43758.5453 - (hash * 43758.5453).floor();
    vertex * (0.72 + r * 0.5)
}

/// `sandstoneRubble(stones)`: 20-triangle fragments merged into one sandstone mesh
/// (loose rubble and gravel where a full ledge rock would spend ten times the
/// triangles on stones a few pixels wide). Flat facets keep them angular.
pub fn sandstone_rubble(stones: &[RubbleStone]) -> Node {
    static PEBBLE: OnceLock<Mesh> = OnceLock::new();
    let pebble = PEBBLE.get_or_init(|| icosahedron_geometry(0.5, 0).to_non_indexed());
    let per_stone = pebble.positions.len();
    let mut positions = Vec::with_capacity(stones.len() * per_stone);
    let mut uvs = Vec::with_capacity(stones.len() * per_stone);
    let mut colors = Vec::with_capacity(stones.len() * per_stone);
    for (s, stone) in stones.iter().enumerate() {
        // A pebble-specific tilt: a few stones sit on edge, most lie flat.
        let jitter = ((s * 7919) % 13) as f64;
        let rotation = quat_from_euler(
            (jitter - 6.0) * 0.05,
            stone.rot_y,
            (((jitter * 5.0) % 13.0) - 6.0) * 0.04,
        );
        let matrix = compose(
            DVec3::new(stone.x, stone.y, stone.z),
            rotation,
            DVec3::new(stone.w, stone.h, stone.d),
        );
        for (v, corner) in pebble.positions.iter().enumerate() {
            let mut vertex = roughen_stone(widen(*corner), s as f64);
            // Squash the lower half so stones rest on, and sink into, the ground.
            if vertex.y < 0.0 {
                vertex.y *= 0.35;
            }
            let vertex = transform_point(&matrix, vertex);
            positions.push([vertex.x as f32, vertex.y as f32, vertex.z as f32]);
            uvs.push([
                (vertex.x / ROCK_UV_TILE) as f32,
                ((vertex.y + vertex.z) / ROCK_UV_TILE) as f32,
            ]);
            // Facet-level variation plus darker undersides where stones meet soil.
            let facet = 0.94 + (((v / 3) * 37 + s * 11) % 9) as f64 * 0.015;
            let under = if vertex.y < stone.y { 0.82 } else { 1.0 };
            let shade = stone.shade * facet * under;
            colors.push([shade as f32, shade as f32, (shade * 0.98) as f32]);
        }
    }
    let mut geometry = Mesh {
        positions,
        uvs,
        colors,
        ..Mesh::default()
    };
    geometry.compute_vertex_normals();
    shadowed(Arc::new(geometry), sandstone_material())
}

/// The drift material of rock footings: soil-coloured sand with world-space grit,
/// blended over the ground ([`SAND_DRIFT`]).
pub fn sand_drift_material() -> Arc<Material> {
    static MATERIAL: OnceLock<Arc<Material>> = OnceLock::new();
    MATERIAL
        .get_or_init(|| {
            Arc::new(Material {
                map: Some(quarry_soil_texture()),
                vertex_colors: true,
                transparent: true,
                depth_write: false,
                effect: Effect::Custom {
                    name: SAND_DRIFT,
                    params: vec![-1.0, -1.0],
                },
                ..Material::standard(0xffffff, 0.0, 1.0)
            })
        })
        .clone()
}

/// `sandstoneFooting(w, d, variant)`: a low, feathered sand apron around a rock's
/// footprint; cosmetic sediment, never tall enough to imply cover. Its outer ring
/// fades out through [`VERTEX_ALPHA`]. Receives shadows only.
pub fn sandstone_footing(w: f64, d: f64, variant: u32) -> Node {
    let key = ([w, d].map(f64::to_bits), variant);
    let mesh = FOOTINGS.get_or_insert(key, || {
        let shape = quarry_rock_shape(w, 1.0, d, variant).positions;
        let mut rng = Random::new(f64::from(variant) + 451.0);
        let sides = 16u32;
        let extensions: Vec<f64> = (0..sides).map(|_| rng.range(0.5, 1.5)).collect();
        let mut positions = Vec::with_capacity(48 * 3);
        let mut uvs = Vec::with_capacity(48 * 2);
        let mut colors = Vec::with_capacity(48);
        let mut alpha = Vec::with_capacity(48);
        let mut indices = Vec::new();
        for ring in 0..3u32 {
            for side in 0..sides {
                let x = f64::from(shape[side as usize * 3]);
                let z = f64::from(shape[side as usize * 3 + 2]);
                let extension = if ring == 0 {
                    -0.35
                } else {
                    extensions[side as usize] * if ring == 1 { 0.45 } else { 1.0 }
                };
                let px = x + js_sign(x) * extension;
                let pz = z + js_sign(z) * extension;
                let y = match ring {
                    0 => 0.18,
                    1 => 0.055,
                    _ => 0.018,
                };
                positions.extend([px, y, pz]);
                uvs.extend([px / 8.0, pz / 8.0]);
                let shade = if ring == 0 { 0.82 } else { 1.0 };
                colors.push([shade as f32; 3]);
                alpha.push(if ring == 2 { 0.0 } else { 1.0 });
                if ring < 2 {
                    let a = ring * sides + side;
                    let b = ring * sides + (side + 1) % sides;
                    indices.extend([a, b, a + sides, b, b + sides, a + sides]);
                }
            }
        }
        let mut geometry = Mesh::from_f64(&positions, &[], &uvs, Some(indices));
        geometry.colors = colors;
        geometry.set_attribute(Attribute {
            name: VERTEX_ALPHA,
            item_size: 1,
            data: alpha,
        });
        geometry.compute_vertex_normals();
        geometry
    });
    let mut node = Node::mesh(mesh, sand_drift_material());
    if let Some(drawable) = &mut node.drawable {
        drawable.receive_shadow = true;
    }
    node
}
