//! Temporary, minimal ports of the surface modules that the scenery agent owns
//! (`house-surfaces.ts`, `concrete-surfaces.ts`, `harbor-surfaces.ts` and the
//! sandstone meshes of `quarry-surfaces.ts`), so the cover models can be built and
//! verified before the branches merge.
//!
//! PENDING: replaced by the scenery agent's module at integration. Every function
//! keeps the TypeScript signature and geometry exactly; materials name the same
//! textures. The sandstone and quarry-dust node materials are TSL shaders owned by
//! the scenery port: here they are `Effect::Custom` placeholders named
//! `sandstone` and `quarry-dust` without parameters.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::DVec3;

use super::model_primitives::{Cache, shadow_receiver, shadowed};
use super::prop_support::{Random, js_hypot};
use super::quarry_shapes::quarry_rock_shape;
use crate::geometry::math::{compose, js_sign, normalize, quat_from_euler, transform_point};
use crate::geometry::{
    Attribute, ExtrudeOptions, Mesh, Path, Shape, box_geometry, extrude_geometry,
    icosahedron_geometry, rounded_box_geometry, widen,
};
use crate::scene::{Effect, Material, Node, Side, TextureRef, Wrap};

// PENDING: replaced by the scenery agent's module at integration.
/// House siding and shingle tiles repeat every 2.56 m.
const TILE_METRES: f64 = 2.56;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum HouseSurface {
    Siding,
    Shingles,
}

static HOUSE_MATERIALS: Cache<(HouseSurface, u32), Material> = Cache::new();
static HOUSE_GEOMETRY: Cache<(u8, [u64; 3]), Mesh> = Cache::new();

// PENDING: replaced by the scenery agent's module at integration.
/// `surfaceMaterial(kind, color)`. The TypeScript loaded these tiles with
/// `flipY = false` to keep the rows of the original DataTexture; `TextureRef` has
/// no flip flag yet.
fn house_material(kind: HouseSurface, color: u32) -> Arc<Material> {
    HOUSE_MATERIALS.get_or_insert((kind, color), || {
        let texture = TextureRef {
            anisotropy: 4,
            ..TextureRef::file(match kind {
                HouseSurface::Siding => "textures/houses/siding.webp",
                HouseSurface::Shingles => "textures/houses/shingles.webp",
            })
        };
        Material {
            color: crate::scene::Color(color),
            map: Some(texture.clone()),
            bump_map: Some(texture),
            bump_scale: if kind == HouseSurface::Shingles {
                0.035
            } else {
                0.02
            },
            roughness: 0.88,
            side: Side::Double,
            ..Material::default()
        }
    })
}

// PENDING: replaced by the scenery agent's module at integration.
/// `sidingBox(w, h, d, color)`: a box with clapboard UVs at real-world scale.
pub fn siding_box(w: f64, h: f64, d: f64, color: u32) -> Node {
    let mesh = HOUSE_GEOMETRY.get_or_insert((0, [w, h, d].map(f64::to_bits)), || {
        let mut mesh = box_geometry(w, h, d);
        for (i, uv) in mesh.uvs.iter_mut().enumerate() {
            let face = i / 4;
            let u = f64::from(uv[0]) * (if face < 2 { d } else { w }) / TILE_METRES;
            let v = f64::from(uv[1]) * (if face == 2 || face == 3 { d } else { h }) / TILE_METRES;
            *uv = [u as f32, v as f32];
        }
        mesh
    });
    shadowed(mesh, house_material(HouseSurface::Siding, color))
}

// PENDING: replaced by the scenery agent's module at integration.
/// `sidingGable(w, h, d, color)`: the triangular roof body with horizontal boards.
pub fn siding_gable(w: f64, h: f64, d: f64, color: u32) -> Node {
    let mesh = HOUSE_GEOMETRY.get_or_insert((1, [w, h, d].map(f64::to_bits)), || {
        let mut profile = Path::new();
        profile
            .move_to(-w / 2.0, 0.0)
            .line_to(0.0, h)
            .line_to(w / 2.0, 0.0)
            .close_path();
        let mut mesh = extrude_geometry(
            &[Shape::new(profile)],
            &ExtrudeOptions {
                depth: d,
                bevel_enabled: false,
                ..ExtrudeOptions::default()
            },
        );
        mesh.translate(0.0, 0.0, -d / 2.0);
        for (uv, p) in mesh.uvs.iter_mut().zip(&mesh.positions) {
            *uv = [
                ((f64::from(p[0]) + w / 2.0) / TILE_METRES) as f32,
                (f64::from(p[1]) / TILE_METRES) as f32,
            ];
        }
        mesh
    });
    shadowed(mesh, house_material(HouseSurface::Siding, color))
}

// PENDING: replaced by the scenery agent's module at integration.
/// `shingleRoof(w, h, d, color)`: two sloped shingle planes over a gable.
pub fn shingle_roof(w: f64, h: f64, d: f64, color: u32) -> Node {
    let mesh = HOUSE_GEOMETRY.get_or_insert((2, [w, h, d].map(f64::to_bits)), || {
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        let slope = js_hypot(w / 2.0, h);
        for side in [-1.0, 1.0] {
            let points = [
                [0.0, h + 0.018, -d / 2.0],
                [(side * w) / 2.0, 0.018, -d / 2.0],
                [(side * w) / 2.0, 0.018, d / 2.0],
                [0.0, h + 0.018, d / 2.0],
            ];
            let coords = [[0.0, slope], [0.0, 0.0], [d, 0.0], [d, slope]];
            for i in [0, 1, 2, 0, 2, 3] {
                positions.extend_from_slice(&points[i]);
                uvs.extend_from_slice(&[coords[i][0] / TILE_METRES, coords[i][1] / TILE_METRES]);
            }
        }
        let mut mesh = Mesh::from_f64(&positions, &[], &uvs, None);
        mesh.compute_vertex_normals();
        mesh
    });
    shadowed(mesh, house_material(HouseSurface::Shingles, color))
}

static CONCRETE_GEOMETRY: Cache<[u64; 3], Mesh> = Cache::new();
static CONCRETE_MATERIAL: Cache<(), Material> = Cache::new();

// PENDING: replaced by the scenery agent's module at integration.
/// The weathered-concrete material of `concreteWall`. The TypeScript also set
/// `shadowSide: FrontSide` (cast from the sun-facing side); `Material` has no
/// shadow-side field yet.
pub fn concrete_material() -> Arc<Material> {
    CONCRETE_MATERIAL.get_or_insert((), || {
        let texture = TextureRef {
            anisotropy: 4,
            ..TextureRef::file("textures/walls/weathered-concrete.webp")
        };
        Material {
            map: Some(texture.clone()),
            bump_map: Some(texture),
            bump_scale: 0.035,
            roughness: 0.95,
            metalness: 0.0,
            ..Material::default()
        }
    })
}

// PENDING: replaced by the scenery agent's module at integration.
/// `concreteWall(w, h, d)`: a rounded block with world-sized UVs every 4 m.
pub fn concrete_wall(w: f64, h: f64, d: f64) -> Node {
    let mesh = CONCRETE_GEOMETRY.get_or_insert([w, h, d].map(f64::to_bits), || {
        let mut mesh = rounded_box_geometry(w, h, d, 1, 0.06);
        for i in 0..mesh.positions.len() {
            let [x, y, z] = mesh.positions[i].map(f64::from);
            let [nx, ny, nz] = mesh.normals[i].map(|n| f64::from(n).abs());
            let u = (if nx > ny && nx > nz { z } else { x }) / 4.0;
            let v = (if ny >= nx && ny >= nz { z } else { y }) / 4.0;
            mesh.uvs[i] = [u as f32, v as f32];
        }
        mesh
    });
    shadowed(mesh, concrete_material())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HarborSurface {
    Dock,
    Steel,
}

static HARBOR_MATERIALS: Cache<(HarborSurface, u32), Material> = Cache::new();
static HARBOR_GEOMETRY: Cache<([u64; 3], HarborSurface), Mesh> = Cache::new();

// PENDING: replaced by the scenery agent's module at integration.
/// `harborMaterial(kind, color)`.
pub fn harbor_material(kind: HarborSurface, color: u32) -> Arc<Material> {
    HARBOR_MATERIALS.get_or_insert((kind, color), || {
        let texture = TextureRef {
            wrap: Wrap::Mirror,
            anisotropy: 4,
            ..TextureRef::file(match kind {
                HarborSurface::Dock => "textures/harbor/dock.webp",
                HarborSurface::Steel => "textures/harbor/steel.webp",
            })
        };
        let steel = kind == HarborSurface::Steel;
        Material {
            color: crate::scene::Color(color),
            map: Some(texture.clone()),
            bump_map: Some(texture),
            bump_scale: if steel { 0.012 } else { 0.035 },
            metalness: if steel { 0.3 } else { 0.0 },
            roughness: if steel { 0.68 } else { 0.95 },
            ..Material::default()
        }
    })
}

// PENDING: replaced by the scenery agent's module at integration.
/// `harborBox(w, h, d, color, kind = "steel")`: a box with world-sized UVs.
pub fn harbor_box(w: f64, h: f64, d: f64, color: u32, kind: HarborSurface) -> Node {
    let mesh = HARBOR_GEOMETRY.get_or_insert(([w, h, d].map(f64::to_bits), kind), || {
        let mut mesh = box_geometry(w, h, d);
        let tile = if kind == HarborSurface::Dock {
            10.0
        } else {
            4.0
        };
        for i in 0..mesh.positions.len() {
            let [x, y, z] = mesh.positions[i].map(f64::from);
            let [nx, ny, _] = mesh.normals[i].map(f64::from);
            mesh.uvs[i] = [
                ((if nx.abs() > 0.5 { z } else { x }) / tile) as f32,
                ((if ny.abs() > 0.5 { z } else { y }) / tile) as f32,
            ];
        }
        mesh
    });
    shadowed(mesh, harbor_material(kind, color))
}

static SANDSTONE_GEOMETRY: Cache<([u64; 3], u32), Mesh> = Cache::new();
static FOOTING_GEOMETRY: Cache<([u64; 2], u32), Mesh> = Cache::new();
static SANDSTONE: Cache<u8, Material> = Cache::new();

// PENDING: replaced by the scenery agent's module at integration.
/// `sandstoneMaterial()`: vertex-colored sandstone; the TSL triplanar strata shading
/// is the scenery port's `sandstone` effect.
pub fn sandstone_material() -> Arc<Material> {
    SANDSTONE.get_or_insert(0, || Material {
        map: Some(TextureRef {
            wrap: Wrap::Mirror,
            anisotropy: 4,
            ..TextureRef::file("textures/quarry/sandstone.webp")
        }),
        roughness: 0.95,
        vertex_colors: true,
        effect: Effect::Custom {
            name: "sandstone",
            params: Vec::new(),
        },
        ..Material::default()
    })
}

// PENDING: replaced by the scenery agent's module at integration.
/// `sandstoneFooting`'s dust material. The TypeScript also used a polygon offset
/// of (-1, -1), which `Material` cannot express yet.
fn quarry_dust_material() -> Arc<Material> {
    SANDSTONE.get_or_insert(1, || Material {
        roughness: 1.0,
        vertex_colors: true,
        transparent: true,
        depth_write: false,
        effect: Effect::Custom {
            name: "quarry-dust",
            params: Vec::new(),
        },
        ..Material::default()
    })
}

// PENDING: replaced by the scenery agent's module at integration.
/// `sandstoneRock(w, h, d, variant)`: the shared rock shape with per-face shade.
pub fn sandstone_rock(w: f64, h: f64, d: f64, variant: u32) -> Node {
    let mesh = SANDSTONE_GEOMETRY.get_or_insert(([w, h, d].map(f64::to_bits), variant), || {
        let mut rng = Random::new(812.0 + f64::from(variant));
        let shape = quarry_rock_shape(w, h, d, variant);
        let vertex = |i: u32| {
            let at = i as usize * 3;
            widen([
                shape.positions[at],
                shape.positions[at + 1],
                shape.positions[at + 2],
            ])
        };
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        let mut colors = Vec::new();
        for triangle in shape.indices.as_chunks::<3>().0 {
            let shade = rng.range(0.9, 1.07);
            let warm = rng.range(-0.02, 0.035);
            let [a, b, c] = triangle.map(vertex);
            let normal = normalize((b - a).cross(c - a));
            let top = normal.y.abs() > 0.65;
            let along_z = normal.x.abs() > normal.z.abs();
            for p in [a, b, c] {
                positions.extend_from_slice(&[p.x, p.y, p.z]);
                uvs.extend_from_slice(&[
                    (if along_z && !top { p.z } else { p.x }) / 5.5,
                    (if top { p.z } else { p.y }) / 5.5,
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
        let mut mesh = Mesh::from_f64(&positions, &[], &uvs, None);
        mesh.colors = colors;
        mesh.to_creased_normals(PI / 5.0)
    });
    shadowed(mesh, sandstone_material())
}

/// One loose stone of `sandstoneRubble` (TS `RubbleStone`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RubbleStone {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
    pub h: f64,
    pub d: f64,
    pub rot_y: f64,
    pub shade: f64,
}

// PENDING: replaced by the scenery agent's module at integration.
/// `roughenStone(vertex, stone)`.
fn roughen_stone(vertex: DVec3, stone: usize) -> DVec3 {
    let hash =
        (stone as f64 * 12.9898 + vertex.x * 78.233 + vertex.y * 37.719 + vertex.z * 11.131).sin();
    let r = hash * 43758.5453 - (hash * 43758.5453).floor();
    vertex * (0.72 + r * 0.5)
}

// PENDING: replaced by the scenery agent's module at integration.
/// `sandstoneRubble(stones)`: angular icosahedron pebbles merged into one mesh.
pub fn sandstone_rubble(stones: &[RubbleStone]) -> Node {
    let template = icosahedron_geometry(0.5, 0);
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    for (s, stone) in stones.iter().enumerate() {
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
        for (v, source) in template.positions.iter().enumerate() {
            let mut vertex = roughen_stone(widen(*source), s);
            if vertex.y < 0.0 {
                vertex.y *= 0.35;
            }
            let vertex = transform_point(&matrix, vertex);
            positions.push([vertex.x as f32, vertex.y as f32, vertex.z as f32]);
            uvs.push([
                (vertex.x / 5.5) as f32,
                ((vertex.y + vertex.z) / 5.5) as f32,
            ]);
            let facet = 0.94 + (((v / 3) * 37 + s * 11) % 9) as f64 * 0.015;
            let under = if vertex.y < stone.y { 0.82 } else { 1.0 };
            let shade = stone.shade * facet * under;
            colors.push([shade as f32, shade as f32, (shade * 0.98) as f32]);
        }
    }
    let mut mesh = Mesh {
        positions,
        uvs,
        colors,
        ..Mesh::default()
    };
    mesh.compute_vertex_normals();
    shadowed(Arc::new(mesh), sandstone_material())
}

// PENDING: replaced by the scenery agent's module at integration.
/// `sandstoneFooting(w, d, variant)`: a feathered sand apron around a rock. Its
/// vertex colors are RGBA; the alpha channel is the custom `color_alpha` attribute.
pub fn sandstone_footing(w: f64, d: f64, variant: u32) -> Node {
    let mesh = FOOTING_GEOMETRY.get_or_insert(([w, d].map(f64::to_bits), variant), || {
        let shape = quarry_rock_shape(w, 1.0, d, variant).positions;
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        let mut colors = Vec::new();
        let mut alpha = Vec::new();
        let mut indices = Vec::new();
        let mut rng = Random::new(f64::from(variant) + 451.0);
        let sides = 16u32;
        let extensions: Vec<f64> = (0..sides).map(|_| rng.range(0.5, 1.5)).collect();
        for ring in 0..3u32 {
            for side in 0..sides {
                let x = f64::from(shape[side as usize * 3]);
                let z = f64::from(shape[side as usize * 3 + 2]);
                let extension = if ring == 0 {
                    -0.35
                } else {
                    extensions[side as usize] * (if ring == 1 { 0.45 } else { 1.0 })
                };
                let px = x + js_sign(x) * extension;
                let pz = z + js_sign(z) * extension;
                let y = match ring {
                    0 => 0.18,
                    1 => 0.055,
                    _ => 0.018,
                };
                positions.extend_from_slice(&[px, y, pz]);
                uvs.extend_from_slice(&[px / 8.0, pz / 8.0]);
                let shade = if ring == 0 { 0.82 } else { 1.0 };
                colors.push([shade as f32; 3]);
                alpha.push(if ring == 2 { 0.0 } else { 1.0 });
                if ring < 2 {
                    let a = ring * sides + side;
                    let b = ring * sides + (side + 1) % sides;
                    indices.extend_from_slice(&[a, b, a + sides, b, b + sides, a + sides]);
                }
            }
        }
        let mut mesh = Mesh::from_f64(&positions, &[], &uvs, Some(indices));
        mesh.colors = colors;
        mesh.attributes.push(Attribute {
            name: "color_alpha",
            item_size: 1,
            data: alpha,
        });
        mesh.compute_vertex_normals();
        mesh
    });
    shadow_receiver(mesh, quarry_dust_material())
}
