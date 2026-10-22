//! Port of `harbor-surfaces.ts`: dock concrete and painted steel with real-world
//! UV scale, shared by harbor scenery, containers and quarry machinery, and the
//! quay apron's world-space concrete and paint ([`apron_box`], [`marking`]).

use std::sync::Arc;

use glam::DVec3;

use crate::geometry::math::hex_to_linear;
use crate::geometry::{Mesh, box_geometry, merge_geometries};
use crate::scene::{Effect, Material, Node, Side, TextureRef, Wrap};

use super::effects_scenery::{ApronMode, DOCK_GRAIN, HARBOR_APRON};
use super::harbor_models::{CONTAINER_METALNESS, CONTAINER_ROUGHNESS, container_wear};
use super::model_primitives::{Cache, shadow_receiver, shadowed, span_between};

/// The two harbor surface tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HarborSurface {
    Dock,
    Steel,
}

impl HarborSurface {
    /// World metres per tile.
    fn tile(self) -> f64 {
        match self {
            HarborSurface::Dock => 10.0,
            HarborSurface::Steel => 4.0,
        }
    }
}

const DOCK_TEXTURE: &str = "textures/harbor/dock.webp";
const STEEL_TEXTURE: &str = "textures/harbor/steel.webp";
/// Hull plating: painted steel, a little glossier than the cargo.
const HULL_METALNESS: f32 = 0.25;
const HULL_ROUGHNESS: f32 = 0.6;
/// Relief of the dock concrete's bump.
const DOCK_BUMP: f32 = 0.035;
/// Roughness of bare quay concrete and of road paint.
const APRON_ROUGHNESS: f32 = 0.92;
const PAINT_ROUGHNESS: f32 = 0.84;
/// Markings keep paint where the effect's wear exceeds this.
const PAINT_CUTOFF: f32 = 0.5;
/// A spawn pad's steel tread plate (the effect polishes and paints it further).
const PAD_METALNESS: f32 = 0.55;
const PAD_ROUGHNESS: f32 = 0.5;

static MATERIALS: Cache<(HarborSurface, u32), Material> = Cache::new();
static GEOMETRIES: Cache<(HarborSurface, [u64; 3]), Mesh> = Cache::new();
static APRON_MATERIALS: Cache<(ApronMode, u32), Material> = Cache::new();
static MARKINGS: Cache<([u64; 3], u32), Mesh> = Cache::new();

fn dock_tile(wrap: Wrap) -> TextureRef {
    TextureRef {
        wrap,
        anisotropy: 4,
        ..TextureRef::file(DOCK_TEXTURE)
    }
}

/// `harborMaterial(kind, color)`: the tile as albedo and bump.
pub fn harbor_material(kind: HarborSurface, color: u32) -> Arc<Material> {
    MATERIALS.get_or_insert((kind, color), || {
        let (path, bump_scale, metalness, roughness) = match kind {
            HarborSurface::Dock => (DOCK_TEXTURE, DOCK_BUMP, 0.0, 0.95),
            HarborSurface::Steel => (STEEL_TEXTURE, 0.012, 0.3, 0.68),
        };
        let texture = TextureRef {
            wrap: Wrap::Mirror,
            anisotropy: 4,
            ..TextureRef::file(path)
        };
        Material {
            map: Some(texture.clone()),
            bump_map: Some(texture),
            bump_scale,
            ..Material::standard(color, metalness, roughness)
        }
    })
}

/// The quay apron's world-space concrete or paint ([`HARBOR_APRON`]). Concrete
/// modes bump with the dock tile at the mesh UVs; markings take their paint from
/// vertex colors so every color shares one draw, as do ship hulls (over the steel
/// tile) and the far shore.
pub fn apron_material(mode: ApronMode, color: u32) -> Arc<Material> {
    APRON_MATERIALS.get_or_insert((mode, color), || {
        let effect = Effect::Custom {
            name: HARBOR_APRON,
            params: vec![mode as u8 as f32],
        };
        let extra_textures = vec![(DOCK_GRAIN, dock_tile(Wrap::Repeat))];
        match mode {
            ApronMode::Concrete
            | ApronMode::LaneAlongZ
            | ApronMode::LaneAlongX
            | ApronMode::QuayWall => Material {
                bump_map: Some(dock_tile(Wrap::Mirror)),
                bump_scale: DOCK_BUMP,
                effect,
                extra_textures,
                ..Material::standard(color, 0.0, APRON_ROUGHNESS)
            },
            ApronMode::Marking => Material {
                vertex_colors: true,
                alpha_test: PAINT_CUTOFF,
                // The flaking edge resolves through the coverage mask, so chips
                // read as soft flakes rather than stair-stepped pixels.
                alpha_to_coverage: true,
                effect,
                extra_textures,
                ..Material::standard(color, 0.0, PAINT_ROUGHNESS)
            },
            ApronMode::Label => Material {
                transparent: true,
                depth_write: false,
                effect,
                extra_textures,
                ..Material::standard(color, 0.0, PAINT_ROUGHNESS)
            },
            ApronMode::Hull => Material {
                map: Some(TextureRef {
                    wrap: Wrap::Mirror,
                    anisotropy: 4,
                    ..TextureRef::file(STEEL_TEXTURE)
                }),
                vertex_colors: true,
                effect,
                extra_textures,
                ..Material::standard(color, HULL_METALNESS, HULL_ROUGHNESS)
            },
            ApronMode::Pad => Material {
                vertex_colors: true,
                effect,
                extra_textures,
                ..Material::standard(color, PAD_METALNESS, PAD_ROUGHNESS)
            },
            ApronMode::Container => Material {
                map: Some(container_wear()),
                vertex_colors: true,
                effect,
                extra_textures,
                ..Material::standard(color, CONTAINER_METALNESS, CONTAINER_ROUGHNESS)
            },
            // Walls cast shadows from their sun-facing side, as the shared
            // concrete does, so no sliver of lit ground shows along a shaded foot.
            ApronMode::Parapet => Material {
                bump_map: Some(dock_tile(Wrap::Mirror)),
                bump_scale: DOCK_BUMP,
                shadow_side: Some(Side::Front),
                effect,
                extra_textures,
                ..Material::standard(color, 0.0, APRON_ROUGHNESS)
            },
            ApronMode::Shore => Material {
                vertex_colors: true,
                fog: false,
                effect,
                extra_textures,
                ..Material::standard(color, 0.0, 1.0)
            },
        }
    })
}

/// A box with planar world-scale UVs chosen by each face's normal.
fn tiled_box(kind: HarborSurface, w: f64, h: f64, d: f64) -> Arc<Mesh> {
    GEOMETRIES.get_or_insert((kind, [w, h, d].map(f64::to_bits)), || {
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
    })
}

/// `harborBox(w, h, d, color, kind = "steel")`: a box with planar world-scale UVs
/// chosen by each face's normal.
pub fn harbor_box(w: f64, h: f64, d: f64, color: u32, kind: HarborSurface) -> Node {
    shadowed(tiled_box(kind, w, h, d), harbor_material(kind, color))
}

/// A slab of quay apron: dock-tile UVs for the bump, world-space concrete on top.
/// It only receives shadows; the apron is the ground they fall on.
pub fn apron_box(w: f64, h: f64, d: f64, color: u32, mode: ApronMode) -> Node {
    shadow_receiver(
        tiled_box(HarborSurface::Dock, w, h, d),
        apron_material(mode, color),
    )
}

/// Depth of the parapet's capping beam and how far it overhangs the precast body
/// on each face, metres: enough for its edge to throw a shadow line.
const PARAPET_CAP: f64 = 0.14;
const PARAPET_OVERHANG: f64 = 0.035;
/// The parapet's precast concrete: warm and pale, as the quay's own.
const PARAPET_CONCRETE: u32 = 0xb8b3a6;

/// Harbor Havoc's sea wall parapet, `w` × `h` × `d` and centred on its middle
/// like the concrete wall it replaces: a precast body under a capping beam that
/// overhangs both long faces a little. Joints, stains and paint are the apron
/// effect's ([`ApronMode::Parapet`]); the collider stays the cover's own box.
pub fn harbor_parapet(w: f64, h: f64, d: f64) -> Node {
    static PARAPETS: Cache<[u64; 3], Mesh> = Cache::new();
    let mesh = PARAPETS.get_or_insert([w, h, d].map(f64::to_bits), || {
        let along_x = w > d;
        let mut body = box_geometry(w, h - PARAPET_CAP, d);
        body.translate(0.0, -PARAPET_CAP / 2.0, 0.0);
        let (cap_w, cap_d) = if along_x {
            (w, d + 2.0 * PARAPET_OVERHANG)
        } else {
            (w + 2.0 * PARAPET_OVERHANG, d)
        };
        let mut cap = box_geometry(cap_w, PARAPET_CAP, cap_d);
        cap.translate(0.0, (h - PARAPET_CAP) / 2.0, 0.0);
        merge_geometries(&[&body, &cap]).expect("boxes share a layout")
    });
    shadowed(mesh, apron_material(ApronMode::Parapet, PARAPET_CONCRETE))
}

/// A thin painted marking on the apron (or a wall top), chipped by the effect.
/// Its paint is a vertex color, so [`batch`](super::batching::batch) merges every
/// marking into one draw.
pub fn marking(w: f64, h: f64, d: f64, color: u32) -> Node {
    let mesh = MARKINGS.get_or_insert(([w, h, d].map(f64::to_bits), color), || {
        let mut geometry = visible_faces(box_geometry(w, h, d), [w, h, d]);
        let [r, g, b] = hex_to_linear(color);
        geometry.colors = vec![[r as f32, g as f32, b as f32]; geometry.positions.len()];
        geometry
    });
    shadow_receiver(mesh, apron_material(ApronMode::Marking, 0xffffff))
}

/// A box's faces that can be seen once it lies on the apron or a wall top: never
/// its underside, nor an edge face under a twentieth of its broadest face (a
/// stripe's 2 cm sides, a kerb band's top). Hundreds of hazard stripes then cost
/// two triangles each instead of twelve, in every pass that draws them.
fn visible_faces(mesh: Mesh, [w, h, d]: [f64; 3]) -> Mesh {
    const EDGE_FACE: f64 = 0.05;
    let broadest = (w * h).max(w * d).max(h * d);
    let keep = |normal: [f32; 3]| {
        let area = if normal[0].abs() > 0.5 {
            h * d
        } else if normal[1].abs() > 0.5 {
            w * d
        } else {
            w * h
        };
        normal[1] > -0.5 && area >= broadest * EDGE_FACE
    };
    let indices = mesh.indices.clone().unwrap_or_default();
    let mut out = Mesh {
        indices: Some(Vec::new()),
        ..Mesh::default()
    };
    let mut remap = vec![u32::MAX; mesh.positions.len()];
    for triangle in indices.as_chunks::<3>().0 {
        if !keep(mesh.normals[triangle[0] as usize]) {
            continue;
        }
        for &index in triangle {
            let index = index as usize;
            if remap[index] == u32::MAX {
                remap[index] = out.positions.len() as u32;
                out.positions.push(mesh.positions[index]);
                out.normals.push(mesh.normals[index]);
                out.uvs.push(mesh.uvs[index]);
            }
            out.indices.as_mut().expect("indexed").push(remap[index]);
        }
    }
    out
}

/// A painted lane arrow flat on the apron, pointing along +z from its centre: a
/// shaft and a head, 3.4 m long. Vertex-colored like [`marking`].
pub fn arrow_marking(color: u32) -> Node {
    static ARROWS: Cache<u32, Mesh> = Cache::new();
    let mesh = ARROWS.get_or_insert(color, || {
        let (shaft, head, length, head_length) = (0.14, 0.55, 3.4, 1.2);
        let (tail, neck, tip) = (-length / 2.0, length / 2.0 - head_length, length / 2.0);
        // Counter-clockwise seen from above, so the faces point up.
        let positions = [
            [-shaft, 0.0, neck],
            [shaft, 0.0, neck],
            [-shaft, 0.0, tail],
            [shaft, 0.0, tail],
            [0.0, 0.0, tip],
            [head, 0.0, neck],
            [-head, 0.0, neck],
        ];
        let flat: Vec<f64> = positions.iter().flatten().copied().collect();
        let mut mesh = Mesh::from_f64(&flat, &[], &[], Some(vec![0, 1, 2, 1, 3, 2, 4, 5, 6]));
        mesh.compute_vertex_normals();
        mesh.uvs = vec![[0.0, 0.0]; positions.len()];
        let [r, g, b] = hex_to_linear(color);
        mesh.colors = vec![[r as f32, g as f32, b as f32]; positions.len()];
        mesh
    });
    shadow_receiver(mesh, apron_material(ApronMode::Marking, 0xffffff))
}

/// `harborBox(w, h, d, color)` in painted steel.
pub fn steel_box(w: f64, h: f64, d: f64, color: u32) -> Node {
    harbor_box(w, h, d, color, HarborSurface::Steel)
}

/// `beam(group, a, b, width, color)`: a painted steel member between two points.
pub fn steel_beam(group: &mut Node, a: [f64; 3], b: [f64; 3], width: f64, color: u32) {
    let (from, to) = (DVec3::from_array(a), DVec3::from_array(b));
    let mesh = steel_box(width, from.distance(to), width, color);
    group.children.push(span_between(mesh, from, to));
}
