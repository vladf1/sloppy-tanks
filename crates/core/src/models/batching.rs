//! Port of `batch()` from `batching.ts`: merge a group's direct mesh children into
//! one mesh per draw material, baking their local transforms into the vertices.
//!
//! Opaque standard paint is baked into vertex colors so parts that differ only in
//! color share one white vertex-color material (and one draw); team paint keeps its
//! emissive glow as a material uniform. Parts of other materials merge per material.

use std::sync::{Arc, Mutex};

use glam::DVec3;

use super::model_primitives::shadowed;
use crate::geometry::math::{compose, hex_to_linear};
use crate::geometry::{Mesh, merge_geometries};
use crate::scene::{Color, Effect, Material, Node, Shading};

/// The shared white clones: one per distinct painted material. They were keyed by
/// some of its fields, the others coming from whichever material asked first, so a
/// clone could depend on the order models were built in (scenery before or after
/// another map's covers), which the page's geometry cache must not.
static VERTEX_MATERIALS: Mutex<Vec<Arc<Material>>> = Mutex::new(Vec::new());

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
    shared_vertex_material(Arc::new(painted(source)))
}

/// The shared vertex material equal to `material`, which becomes it if none is.
fn shared_vertex_material(material: Arc<Material>) -> Arc<Material> {
    let mut materials = VERTEX_MATERIALS.lock().expect("vertex material cache");
    if let Some(held) = materials.iter().find(|held| **held == material) {
        return held.clone();
    }
    materials.push(material.clone());
    material
}

/// Each shared vertex material's address (`shared_values::VERTEX_MATERIALS`).
pub(super) fn each_vertex_material(visit: &mut dyn FnMut(usize)) {
    for material in VERTEX_MATERIALS
        .lock()
        .expect("vertex material cache")
        .iter()
    {
        visit(Arc::as_ptr(material) as usize);
    }
}

/// The vertex material to use for one a model read back from the geometry cache
/// brings: the shared one equal to it, else this one, shared from now on.
pub(super) fn adopt_vertex_material(material: Arc<Material>) -> Arc<Material> {
    shared_vertex_material(material)
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

/// [`batch`] a group's direct mesh children square by square: parts whose bounds
/// centre in the same `cell`-metre square of the x/z plane merge together, so a view
/// culls the squares it cannot see instead of drawing one merge that spans the map.
pub fn batch_in_cells(group: &mut Node, cell: f64) {
    let mut cells: Vec<((i64, i64), Node)> = Vec::new();
    let mut kept = Vec::new();
    for child in std::mem::take(&mut group.children) {
        let Some(drawable) = child.drawable.as_ref().filter(|d| d.instances.is_none()) else {
            kept.push(child);
            continue;
        };
        let (low, high) = drawable.mesh.positions.iter().fold(
            (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
            |(low, high), p| {
                let p = DVec3::new(p[0].into(), p[1].into(), p[2].into());
                (low.min(p), high.max(p))
            },
        );
        let centre = compose(child.position, child.rotation, child.scale)
            .transform_point3((low + high) / 2.0);
        let key = (
            (centre.x / cell).floor() as i64,
            (centre.z / cell).floor() as i64,
        );
        match cells.iter_mut().find(|(at, _)| *at == key) {
            Some((_, square)) => square.children.push(child),
            None => {
                let mut square = Node::group("");
                square.children.push(child);
                cells.push((key, square));
            }
        }
    }
    group.children = kept;
    for (_, mut square) in cells {
        batch(&mut square);
        group.children.append(&mut square.children);
    }
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
