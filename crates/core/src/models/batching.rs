//! Port of `batch()` from `batching.ts`: merge a group's direct mesh children into
//! one mesh per draw material, baking their local transforms into the vertices.
//!
//! Opaque standard paint is baked into vertex colors so parts that differ only in
//! color share one white vertex-color material (and one draw); team paint keeps its
//! emissive glow as a material uniform. Parts of other materials merge per material.

use std::sync::{Arc, Mutex};

use super::model_primitives::shadowed;
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
    polygon_offset: Option<(f32, f32)>,
}

static VERTEX_MATERIALS: Mutex<Vec<(VertexMaterialKey, Arc<Material>)>> = Mutex::new(Vec::new());

/// Whether a material's paint can be baked into vertex colors: opaque standard
/// surfaces without per-pixel alpha, vertex colors, an emissive map or custom
/// shading. The renderer batches parts at runtime by the same rule.
pub fn is_paintable(material: &Material) -> bool {
    material.shading == Shading::Standard
        && material.effect == Effect::None
        && !material.transparent
        && material.opacity == 1.0
        && material.alpha_test == 0.0
        && !material.vertex_colors
        && material.emissive_map.is_none()
}

/// The white, vertex-colored stand-in for a paintable material.
pub fn painted(material: &Material) -> Material {
    Material {
        color: Color(0xffffff),
        vertex_colors: true,
        ..material.clone()
    }
}

/// `vertexMaterial(source)`: the shared vertex-color clone for opaque standard
/// paint, or the source itself for anything else (unlit, transparent, cut-out,
/// already vertex-colored, emissive-mapped, or a custom effect).
pub fn vertex_material(source: &Arc<Material>) -> Arc<Material> {
    if !is_paintable(source) {
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
        polygon_offset: source.polygon_offset,
    };
    let mut materials = VERTEX_MATERIALS.lock().expect("vertex material cache");
    if let Some((_, material)) = materials.iter().find(|(k, _)| *k == key) {
        return material.clone();
    }
    let material = Arc::new(painted(source));
    materials.push((key, material.clone()));
    material
}

/// A mesh part and the paint to bake into its vertices, if its batch uses a
/// vertex-color material.
type PaintedPart = (Node, Option<Color>);

/// A part's geometry de-indexed and moved into its parent's frame, with its paint
/// as a vertex color when the batch uses a vertex-color material. A non-indexed
/// mesh only this part holds is taken rather than copied: merged scenery (the quarry
/// walls) runs to hundreds of thousands of vertices.
fn baked_part(node: Node, paint: Option<Color>) -> Mesh {
    let transform = compose(node.position, node.rotation, node.scale);
    let mesh = node.drawable.expect("batched parts are meshes").mesh;
    let mut baked = match Arc::try_unwrap(mesh) {
        Ok(mesh) if mesh.indices.is_none() => mesh,
        Ok(mesh) => mesh.to_non_indexed(),
        Err(shared) => shared.to_non_indexed(),
    };
    baked.apply_matrix4(&transform);
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
            .into_iter()
            .map(|(node, paint)| baked_part(node, paint))
            .collect();
        // Like mergeGeometries, parts with mismatched attributes produce nothing.
        if let Some(merged) = merge_geometries(&baked.iter().collect::<Vec<_>>()) {
            group.children.push(shadowed(Arc::new(merged), material));
        }
    }
}
