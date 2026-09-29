//! Port of `batch()` from `batching.ts`: merge a group's direct mesh children into
//! one mesh per draw material, baking their local transforms into the vertices.
//!
//! Opaque standard paint is baked into vertex colors so parts that differ only in
//! color share one white vertex-color material (and one draw); team paint keeps its
//! emissive glow as a material uniform. Parts of other materials merge per material.

use std::sync::{Arc, Mutex};

use crate::geometry::math::{compose, hex_to_linear};
use crate::geometry::{Mesh, merge_geometries};
use crate::scene::{Color, Effect, Material, Node, Shading, Side, TextureRef};

/// The material fields `vertexMaterial` keyed its shared white clones by. Fields
/// outside the key come from the first material that created the clone, as in
/// the TypeScript.
#[derive(Clone, Debug, PartialEq)]
struct VertexMaterialKey {
    metalness: f32,
    roughness: f32,
    tone_mapped: bool,
    emissive: Color,
    emissive_intensity: f32,
    side: Side,
    flat_shading: bool,
    depth_test: bool,
    depth_write: bool,
    map: Option<TextureRef>,
    bump_map: Option<TextureRef>,
    bump_scale: f32,
}

static VERTEX_MATERIALS: Mutex<Vec<(VertexMaterialKey, Arc<Material>)>> = Mutex::new(Vec::new());

/// `vertexMaterial(source)`: the shared vertex-color clone for opaque standard
/// paint, or the source itself for anything else (unlit, transparent, cut-out,
/// already vertex-colored, or a custom effect).
pub fn vertex_material(source: &Arc<Material>) -> Arc<Material> {
    if source.shading != Shading::Standard
        || source.effect != Effect::None
        || source.transparent
        || source.opacity != 1.0
        || source.alpha_test != 0.0
        || source.vertex_colors
    {
        return source.clone();
    }
    let key = VertexMaterialKey {
        metalness: source.metalness,
        roughness: source.roughness,
        tone_mapped: source.tone_mapped,
        emissive: source.emissive,
        emissive_intensity: source.emissive_intensity,
        side: source.side,
        flat_shading: source.flat_shading,
        depth_test: source.depth_test,
        depth_write: source.depth_write,
        map: source.map.clone(),
        bump_map: source.bump_map.clone(),
        bump_scale: source.bump_scale,
    };
    let mut materials = VERTEX_MATERIALS.lock().expect("vertex material cache");
    if let Some((_, material)) = materials.iter().find(|(k, _)| *k == key) {
        return material.clone();
    }
    let material = Arc::new(Material {
        color: Color(0xffffff),
        vertex_colors: true,
        ..(**source).clone()
    });
    materials.push((key, material.clone()));
    material
}

/// A mesh part and the paint to bake into its vertices, if its batch uses a
/// vertex-color material.
type PaintedPart = (Node, Option<Color>);

/// A part's geometry de-indexed and moved into its parent's frame, with its paint
/// as a vertex color when the batch uses a vertex-color material.
fn baked_part(node: &Node, mesh: &Mesh, paint: Option<Color>) -> Mesh {
    let mut baked = mesh.to_non_indexed();
    baked.apply_matrix4(&compose(node.position, node.rotation, node.scale));
    if let Some(Color(hex)) = paint {
        let [r, g, b] = hex_to_linear(hex);
        baked.colors = vec![[r as f32, g as f32, b as f32]; baked.positions.len()];
    }
    baked
}

/// `paintMesh(mesh)`: give one standalone mesh the shared vertex-color material of
/// batched parts, baking its paint into a new color attribute (the geometry keeps
/// its index). Meshes whose material keeps its own shader are unchanged.
pub fn paint_mesh(node: &mut Node) {
    let Some(drawable) = &mut node.drawable else {
        return;
    };
    let source = drawable.material.clone();
    let painted = vertex_material(&source);
    if Arc::ptr_eq(&painted, &source) {
        return;
    }
    let [r, g, b] = hex_to_linear(source.color.0);
    let mut mesh = (*drawable.mesh).clone();
    mesh.colors = vec![[r as f32, g as f32, b as f32]; mesh.positions.len()];
    drawable.mesh = Arc::new(mesh);
    drawable.material = painted;
}

/// `batch(group)`: replace the group's direct, non-instanced mesh children with
/// one merged mesh per draw material, appended after the remaining children in
/// order of first use. Nested groups (movable assemblies) are left alone. Merged
/// meshes cast and receive shadows and have an identity transform.
pub fn batch(group: &mut Node) {
    let mut batches: Vec<(Arc<Material>, Vec<PaintedPart>)> = Vec::new();
    let mut kept = Vec::with_capacity(group.children.len());
    for child in std::mem::take(&mut group.children) {
        let Some(drawable) = child.drawable.as_ref().filter(|d| d.instances.is_none()) else {
            kept.push(child);
            continue;
        };
        let source = drawable.material.clone();
        let material = vertex_material(&source);
        let paint = (!Arc::ptr_eq(&material, &source)).then_some(source.color);
        match batches.iter_mut().find(|(m, _)| Arc::ptr_eq(m, &material)) {
            Some((_, parts)) => parts.push((child, paint)),
            None => batches.push((material, vec![(child, paint)])),
        }
    }
    group.children = kept;
    for (material, parts) in batches {
        let baked: Vec<Mesh> = parts
            .iter()
            .map(|(node, paint)| {
                let drawable = node.drawable.as_ref().expect("batched parts are meshes");
                baked_part(node, &drawable.mesh, *paint)
            })
            .collect();
        // Like mergeGeometries, parts with mismatched attributes produce nothing.
        if let Some(merged) = merge_geometries(&baked.iter().collect::<Vec<_>>()) {
            let mut node = Node::mesh(Arc::new(merged), material);
            if let Some(drawable) = &mut node.drawable {
                drawable.cast_shadow = true;
                drawable.receive_shadow = true;
            }
            group.children.push(node);
        }
    }
}
