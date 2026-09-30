//! Model preparation: flatten a `scene::Node` tree into few draws.
//!
//! Named nodes are the movable joints (turret, barrel, wheels, boughs): an instance
//! can override their local transform and visibility each frame. Everything
//! unnamed below a joint is rigid relative to it, so its drawables are merged per
//! material into one mesh, and differently painted opaque parts share a white,
//! vertex-colored material (the `batch()`/`vertexMaterial` rule of `batching.ts`).
//! A lone part keeps its shared `Arc<Mesh>`, which is uploaded once and drawn
//! instanced across every model and instance that uses it.
//!
//! Static scenery flattens the whole tree into world space instead, merges by
//! material within spatial cells (so it can still be frustum culled), and turns
//! meshes repeated many times into static instanced draws.

use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{DMat4, Mat3, Mat4, Vec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::scene::{Drawable, Material, Node};

use crate::camera::Sphere;
use crate::material::{MaterialInterner, is_paintable, paint_color, painted};

/// The uploaded vertex layout (48 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Linear RGB and alpha: white when the mesh has no colors, alpha from its
    /// `VERTEX_ALPHA` attribute (RGBA vertex colors) or 1.
    pub color: [f32; 4],
}

/// CPU vertex data ready for upload.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// `extra_attributes` vec4s per vertex, in the effect's attribute order
    /// (per-vertex attributes only; per-instance ones travel with instances).
    pub extra: Vec<[f32; 4]>,
    pub extra_attributes: u8,
    pub bounds: Sphere,
}

impl MeshData {
    fn finish(mut self) -> Self {
        self.bounds = Sphere::from_points(self.vertices.iter().map(|v| Vec3::from(v.position)));
        self
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    fn append(
        &mut self,
        mesh: &Mesh,
        attributes: &[&str],
        transform: Option<&Mat4>,
        paint: Option<[f32; 3]>,
    ) {
        let base = self.vertices.len() as u32;
        let count = mesh.positions.len();
        self.vertices.reserve(count);
        self.extra.reserve(count * attributes.len());
        self.indices
            .reserve(mesh.indices.as_ref().map_or(count, Vec::len));
        push_vertices(
            &mut self.vertices,
            &mut self.extra,
            mesh,
            attributes,
            0..count,
            transform,
            paint,
        );
        match &mesh.indices {
            Some(indices) => self.indices.extend(indices.iter().map(|i| base + i)),
            None => self
                .indices
                .extend(base..base + mesh.positions.len() as u32),
        }
    }
}

/// Convert vertices `range` of `mesh` to the upload layout, moved by `transform`
/// and painted with `paint` when given, with their `attributes` vec4s.
fn push_vertices(
    vertices: &mut Vec<Vertex>,
    extra: &mut Vec<[f32; 4]>,
    mesh: &Mesh,
    attributes: &[&str],
    range: Range<usize>,
    transform: Option<&Mat4>,
    paint: Option<[f32; 3]>,
) {
    let normal_matrix = transform.map(|m| Mat3::from_mat4(*m).inverse().transpose());
    let alpha = mesh.vertex_alpha();
    for i in range {
        let mut position = Vec3::from(mesh.positions[i]);
        let mut normal = mesh.normals.get(i).map_or(Vec3::Y, |n| Vec3::from(*n));
        if let (Some(matrix), Some(normal_matrix)) = (transform, normal_matrix) {
            position = matrix.transform_point3(position);
            normal = (normal_matrix * normal).normalize_or_zero();
        }
        let [r, g, b] = paint.unwrap_or_else(|| mesh.colors.get(i).copied().unwrap_or([1.0; 3]));
        let a = alpha.and_then(|alpha| alpha.get(i)).copied().unwrap_or(1.0);
        vertices.push(Vertex {
            position: position.into(),
            normal: normal.into(),
            uv: mesh.uvs.get(i).copied().unwrap_or_default(),
            color: [r, g, b, a],
        });
        for name in attributes {
            let mut value = [0.0; 4];
            if let Some(attribute) = mesh.attribute(name).filter(|a| !a.per_instance) {
                let size = attribute.item_size as usize;
                for (k, slot) in value.iter_mut().enumerate().take(size.min(4)) {
                    *slot = attribute.data.get(i * size + k).copied().unwrap_or(0.0);
                }
            }
            extra.push(value);
        }
    }
}

/// Upload data for an unmodified shared mesh.
pub fn mesh_data(mesh: &Mesh, attributes: &[&str]) -> MeshData {
    let mut data = MeshData {
        extra_attributes: attributes.len() as u8,
        ..MeshData::default()
    };
    data.append(mesh, attributes, None, None);
    data.finish()
}

/// Vertices `range` of an unmodified shared mesh as [`mesh_data`] lays them out,
/// with their effect attribute vec4s, so a large mesh can go to the GPU in pieces
/// instead of through one full-size copy.
pub fn shared_vertices(
    mesh: &Mesh,
    attributes: &[&str],
    range: Range<usize>,
) -> (Vec<Vertex>, Vec<[f32; 4]>) {
    let mut vertices = Vec::with_capacity(range.len());
    let mut extra = Vec::with_capacity(range.len() * attributes.len());
    push_vertices(
        &mut vertices,
        &mut extra,
        mesh,
        attributes,
        range,
        None,
        None,
    );
    (vertices, extra)
}

/// One InstancedMesh instance relative to its part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceData {
    pub matrix: Mat4,
    /// Linear RGB multiplier (Three `instanceColor`).
    pub color: [f32; 3],
    /// Effect data from the mesh's per-instance attributes (see
    /// [`instance_attribute_data`]); `None` keeps the placed instance's data.
    pub data: Option<[f32; 4]>,
}

/// The effect data of instance `index`: the mesh's per-instance attributes packed
/// in attribute order into four floats (extra components are dropped), or `None`
/// when the mesh has none.
pub fn instance_attribute_data(mesh: &Mesh, index: usize) -> Option<[f32; 4]> {
    let mut data = [0.0; 4];
    let mut filled = 0;
    for attribute in mesh.attributes.iter().filter(|a| a.per_instance) {
        let size = usize::from(attribute.item_size);
        for k in 0..size {
            if filled < 4 {
                data[filled] = attribute.data.get(index * size + k).copied().unwrap_or(0.0);
                filled += 1;
            }
        }
    }
    (filled > 0).then_some(data)
}

#[derive(Clone, Debug)]
pub enum PartMesh {
    /// A caller-owned mesh, uploaded once per `Arc` and shared by every user.
    Shared(Arc<Mesh>),
    /// Merged geometry owned by this model (index into `PreparedModel::meshes`).
    Owned(usize),
}

#[derive(Clone, Debug)]
pub struct PreparedPart {
    /// The joint this part moves with (0 is the model root).
    pub node: usize,
    /// Transform from mesh space to the joint's space.
    pub local: Mat4,
    pub mesh: PartMesh,
    pub material: Arc<Material>,
    pub cast_shadow: bool,
    pub receive_shadow: bool,
    pub render_order: i32,
    pub frustum_culled: bool,
    /// InstancedMesh instances, each placed by `local * matrix`.
    pub instances: Option<Vec<InstanceData>>,
}

/// A movable joint: a named node, or the root.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelNode {
    pub name: String,
    pub parent: Option<usize>,
    /// Transform relative to the parent joint, as authored.
    pub rest: Mat4,
    pub visible: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PreparedModel {
    /// Joints in parent-before-child order; index 0 is the root.
    pub nodes: Vec<ModelNode>,
    pub parts: Vec<PreparedPart>,
    pub meshes: Vec<MeshData>,
}

impl PreparedModel {
    pub fn node(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|node| node.name == name)
    }

    /// World transforms of every joint for a root placement, applying overrides
    /// (`None` keeps the rest pose). Parents precede children, so one pass works.
    pub fn joint_transforms(&self, root: Mat4, overrides: &[Option<Mat4>], out: &mut Vec<Mat4>) {
        out.clear();
        for (index, node) in self.nodes.iter().enumerate() {
            let local = overrides.get(index).copied().flatten().unwrap_or(node.rest);
            let parent = node.parent.map_or(root, |parent| out[parent]);
            out.push(if index == 0 { root } else { parent * local });
        }
    }
}

/// Which attribute names a material's effect reads (for packing extra vertex data).
pub type AttributesFor<'a> = &'a dyn Fn(&Material) -> &'static [&'static str];

struct Pending<'a> {
    drawable: &'a Drawable,
    transform: DMat4,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct GroupKey {
    node: usize,
    material: usize,
    painted: bool,
    cast_shadow: bool,
    receive_shadow: bool,
    render_order: i32,
    frustum_culled: bool,
    cell: (i32, i32),
}

/// Insertion-ordered groups, so preparation is deterministic.
struct Groups<'a, K> {
    order: Vec<(K, Vec<Pending<'a>>)>,
    index: HashMap<K, usize>,
}

impl<'a, K: Clone + Eq + Hash> Groups<'a, K> {
    fn new() -> Self {
        Self {
            order: Vec::new(),
            index: HashMap::new(),
        }
    }
    fn push(&mut self, key: K, pending: Pending<'a>) {
        let slot = *self.index.entry(key.clone()).or_insert_with(|| {
            self.order.push((key, Vec::new()));
            self.order.len() - 1
        });
        self.order[slot].1.push(pending);
    }
}

fn instances_of(drawable: &Drawable) -> Option<Vec<InstanceData>> {
    drawable.instances.as_ref().map(|instances| {
        instances
            .iter()
            .enumerate()
            .map(|(index, instance)| InstanceData {
                matrix: instance.matrix.as_mat4(),
                color: instance.color.unwrap_or([1.0; 3]),
                data: instance_attribute_data(&drawable.mesh, index),
            })
            .collect()
    })
}

struct Builder<'a, 'b> {
    interner: &'b mut MaterialInterner,
    attributes_for: AttributesFor<'b>,
    model: PreparedModel,
    groups: Groups<'a, GroupKey>,
    /// Drawables that are placed individually (instanced meshes).
    single: Vec<(usize, Pending<'a>)>,
    cell_size: f32,
}

impl<'a> Builder<'a, '_> {
    fn draw_material(&mut self, material: &Arc<Material>) -> (Arc<Material>, bool) {
        if is_paintable(material) {
            (self.interner.intern_value(painted(material)), true)
        } else {
            (self.interner.intern(material), false)
        }
    }

    fn cell(&self, drawable: &Drawable, transform: &DMat4) -> (i32, i32) {
        if self.cell_size <= 0.0 {
            return (0, 0);
        }
        let points = drawable.mesh.positions.iter().map(|p| Vec3::from(*p));
        let center = transform
            .as_mat4()
            .transform_point3(Sphere::from_points(points).center);
        (
            (center.x / self.cell_size).floor() as i32,
            (center.z / self.cell_size).floor() as i32,
        )
    }

    fn add(&mut self, node: usize, drawable: &'a Drawable, transform: DMat4) {
        if drawable.instances.is_some() {
            self.single.push((
                node,
                Pending {
                    drawable,
                    transform,
                },
            ));
            return;
        }
        let (material, painted) = self.draw_material(&drawable.material);
        let key = GroupKey {
            node,
            material: Arc::as_ptr(&material) as usize,
            painted,
            cast_shadow: drawable.cast_shadow,
            receive_shadow: drawable.receive_shadow,
            render_order: drawable.render_order,
            frustum_culled: drawable.frustum_culled,
            cell: self.cell(drawable, &transform),
        };
        self.groups.push(
            key,
            Pending {
                drawable,
                transform,
            },
        );
    }

    fn part(
        &self,
        node: usize,
        pending: &Pending,
        mesh: PartMesh,
        material: Arc<Material>,
    ) -> PreparedPart {
        let drawable = pending.drawable;
        PreparedPart {
            node,
            local: pending.transform.as_mat4(),
            mesh,
            material,
            cast_shadow: drawable.cast_shadow,
            receive_shadow: drawable.receive_shadow,
            render_order: drawable.render_order,
            frustum_culled: drawable.frustum_culled,
            instances: instances_of(drawable),
        }
    }

    fn finish(mut self) -> PreparedModel {
        let singles = std::mem::take(&mut self.single);
        for (node, pending) in &singles {
            let material = self.interner.intern(&pending.drawable.material);
            let part = self.part(
                *node,
                pending,
                PartMesh::Shared(pending.drawable.mesh.clone()),
                material,
            );
            self.model.parts.push(part);
        }
        let groups = std::mem::take(&mut self.groups.order);
        for (key, members) in groups {
            if members.len() == 1 {
                // A lone part keeps its shared mesh and exact material.
                let pending = &members[0];
                let material = self.interner.intern(&pending.drawable.material);
                let part = self.part(
                    key.node,
                    pending,
                    PartMesh::Shared(pending.drawable.mesh.clone()),
                    material,
                );
                self.model.parts.push(part);
                continue;
            }
            let (material, _) = self.draw_material(&members[0].drawable.material);
            let attributes = (self.attributes_for)(&material);
            let mut data = MeshData {
                extra_attributes: attributes.len() as u8,
                ..MeshData::default()
            };
            for pending in &members {
                let paint = key.painted.then(|| paint_color(&pending.drawable.material));
                data.append(
                    &pending.drawable.mesh,
                    attributes,
                    Some(&pending.transform.as_mat4()),
                    paint,
                );
            }
            self.model.meshes.push(data.finish());
            let first = &members[0];
            let mut part = self.part(
                key.node,
                first,
                PartMesh::Owned(self.model.meshes.len() - 1),
                material,
            );
            part.local = Mat4::IDENTITY;
            self.model.parts.push(part);
        }
        self.model
    }
}

/// Prepare a movable model. The root's own transform is ignored: an instance's
/// world matrix places the root, like setting the Three group's position.
pub fn prepare_model(
    root: &Node,
    interner: &mut MaterialInterner,
    attributes_for: AttributesFor,
) -> PreparedModel {
    let mut builder = Builder {
        interner,
        attributes_for,
        model: PreparedModel::default(),
        groups: Groups::new(),
        single: Vec::new(),
        cell_size: 0.0,
    };
    builder.model.nodes.push(ModelNode {
        name: root.name.clone(),
        parent: None,
        rest: Mat4::IDENTITY,
        visible: root.visible,
    });
    if let Some(drawable) = &root.drawable {
        builder.add(0, drawable, DMat4::IDENTITY);
    }
    fn visit<'a>(builder: &mut Builder<'a, '_>, node: &'a Node, joint: usize, relative: DMat4) {
        for child in &node.children {
            let local = relative * child.local_matrix();
            let (joint, relative) = if child.name.is_empty() {
                if !child.visible {
                    continue;
                }
                (joint, local)
            } else {
                builder.model.nodes.push(ModelNode {
                    name: child.name.clone(),
                    parent: Some(joint),
                    rest: local.as_mat4(),
                    visible: child.visible,
                });
                (builder.model.nodes.len() - 1, DMat4::IDENTITY)
            };
            if let Some(drawable) = &child.drawable {
                builder.add(joint, drawable, relative);
            }
            visit(builder, child, joint, relative);
        }
    }
    visit(&mut builder, root, 0, DMat4::IDENTITY);
    builder.finish()
}

/// Static scenery settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneryOptions {
    /// Merge cell edge in metres; 0 merges each material into one draw.
    pub cell_size: f32,
    /// A mesh repeated at least this often draws instanced instead of merged.
    pub instance_threshold: usize,
}

impl Default for SceneryOptions {
    fn default() -> Self {
        Self {
            cell_size: 60.0,
            instance_threshold: 24,
        }
    }
}

/// Prepare static scenery in world space (the root transform applies). The result
/// has a single root joint drawn at identity.
pub fn prepare_scenery(
    root: &Node,
    interner: &mut MaterialInterner,
    attributes_for: AttributesFor,
    options: SceneryOptions,
) -> PreparedModel {
    let mut all = Vec::new();
    collect_visible(root, DMat4::IDENTITY, &mut all);
    // Count repeated (mesh, material, flags) so crowds of identical props become
    // one instanced draw instead of copied vertices.
    let repeat_key = |d: &Drawable| {
        (
            Arc::as_ptr(&d.mesh) as usize,
            Arc::as_ptr(&d.material) as usize,
            d.cast_shadow,
            d.receive_shadow,
            d.render_order,
        )
    };
    let mut counts: HashMap<_, usize> = HashMap::new();
    for (drawable, _) in &all {
        if drawable.instances.is_none() {
            *counts.entry(repeat_key(drawable)).or_default() += 1;
        }
    }
    let mut builder = Builder {
        interner,
        attributes_for,
        model: PreparedModel::default(),
        groups: Groups::new(),
        single: Vec::new(),
        cell_size: options.cell_size,
    };
    builder.model.nodes.push(ModelNode {
        name: root.name.clone(),
        parent: None,
        rest: Mat4::IDENTITY,
        visible: true,
    });
    let mut repeated: Groups<(usize, usize, bool, bool, i32)> = Groups::new();
    for (drawable, world) in all {
        let key = repeat_key(drawable);
        if drawable.instances.is_none() && counts[&key] >= options.instance_threshold {
            repeated.push(
                key,
                Pending {
                    drawable,
                    transform: world,
                },
            );
        } else {
            builder.add(0, drawable, world);
        }
    }
    for (_, members) in repeated.order {
        let first = members[0].drawable;
        let material = builder.interner.intern(&first.material);
        builder.model.parts.push(PreparedPart {
            node: 0,
            local: Mat4::IDENTITY,
            mesh: PartMesh::Shared(first.mesh.clone()),
            material,
            cast_shadow: first.cast_shadow,
            receive_shadow: first.receive_shadow,
            render_order: first.render_order,
            frustum_culled: first.frustum_culled,
            instances: Some(
                members
                    .iter()
                    .map(|pending| InstanceData {
                        matrix: pending.transform.as_mat4(),
                        color: [1.0; 3],
                        data: None,
                    })
                    .collect(),
            ),
        });
    }
    builder.finish()
}

fn collect_visible<'a>(node: &'a Node, parent: DMat4, out: &mut Vec<(&'a Drawable, DMat4)>) {
    if !node.visible {
        return;
    }
    let world = parent * node.local_matrix();
    if let Some(drawable) = &node.drawable {
        out.push((drawable, world));
    }
    for child in &node.children {
        collect_visible(child, world, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{DQuat, DVec3};
    use sloppy_core::scene::{Color, Instance};

    fn quad() -> Arc<Mesh> {
        Arc::new(Mesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: Some(vec![0, 1, 2, 0, 2, 3]),
            ..Mesh::default()
        })
    }

    fn part(mesh: &Arc<Mesh>, material: &Arc<Material>, x: f64) -> Node {
        let mut node = Node::mesh(mesh.clone(), material.clone());
        node.position = DVec3::new(x, 0.0, 0.0);
        node
    }

    fn no_attributes(_: &Material) -> &'static [&'static str] {
        &[]
    }

    fn tank() -> (Node, Arc<Mesh>) {
        let mesh = quad();
        let green = Arc::new(Material::standard(0x3a5f3a, 0.05, 0.65));
        let grey = Arc::new(Material::standard(0x777777, 0.05, 0.65));
        let glass = Arc::new(Material {
            transparent: true,
            opacity: 0.5,
            ..Material::default()
        });
        let mut root = Node::group("tank");
        let mut hull = Node::group("");
        hull.children = vec![
            part(&mesh, &green, 0.0),
            part(&mesh, &grey, 2.0),
            part(&mesh, &glass, 4.0),
        ];
        let mut turret = Node::group("turret");
        turret.position = DVec3::new(0.0, 1.5, 0.0);
        turret.rotation = DQuat::from_rotation_y(0.5);
        let mut barrel = Node::group("barrel");
        barrel.position = DVec3::new(0.0, 0.2, 1.0);
        barrel.children = vec![part(&mesh, &grey, 0.0), part(&mesh, &grey, 0.5)];
        turret.children = vec![part(&mesh, &green, 0.0), barrel];
        root.children = vec![hull, turret];
        (root, mesh)
    }

    #[test]
    fn joints_merge_rigid_parts_and_paint_them() {
        let (root, _) = tank();
        let mut interner = MaterialInterner::default();
        let model = prepare_model(&root, &mut interner, &no_attributes);
        let names: Vec<_> = model.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["tank", "turret", "barrel"]);
        assert_eq!(model.nodes[2].parent, Some(1));
        // Root: green + grey merge into one painted draw; glass stays alone.
        let root_parts: Vec<_> = model.parts.iter().filter(|p| p.node == 0).collect();
        assert_eq!(root_parts.len(), 2);
        let merged = root_parts
            .iter()
            .find(|p| matches!(p.mesh, PartMesh::Owned(_)))
            .unwrap();
        assert!(merged.material.vertex_colors);
        assert_eq!(merged.material.color, Color(0xffffff));
        let PartMesh::Owned(index) = merged.mesh else {
            unreachable!()
        };
        let data = &model.meshes[index];
        assert_eq!(data.vertices.len(), 8);
        assert_eq!(data.triangle_count(), 4);
        assert_eq!(data.vertices[4].position, [2.0, 0.0, 0.0]);
        assert_ne!(data.vertices[0].color, data.vertices[4].color);
        // The turret's single part keeps the shared mesh and its exact material.
        let turret = model.parts.iter().find(|p| p.node == 1).unwrap();
        assert!(matches!(turret.mesh, PartMesh::Shared(_)));
        assert!(!turret.material.vertex_colors);
        // The barrel's two grey parts merge.
        assert_eq!(model.parts.iter().filter(|p| p.node == 2).count(), 1);
    }

    #[test]
    fn joint_transforms_apply_overrides() {
        let (root, _) = tank();
        let mut interner = MaterialInterner::default();
        let model = prepare_model(&root, &mut interner, &no_attributes);
        let mut joints = Vec::new();
        let place = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        model.joint_transforms(place, &[], &mut joints);
        let barrel_origin = joints[2].transform_point3(Vec3::ZERO);
        let expected = Vec3::new(10.0 + 0.5f32.sin(), 1.7, 0.5f32.cos());
        assert!(barrel_origin.distance(expected) < 1e-5, "{barrel_origin:?}");
        let mut overrides = vec![None; model.nodes.len()];
        overrides[1] = Some(Mat4::from_translation(Vec3::new(0.0, 1.5, 0.0)));
        model.joint_transforms(place, &overrides, &mut joints);
        let barrel_origin = joints[2].transform_point3(Vec3::ZERO);
        assert!(barrel_origin.distance(Vec3::new(10.0, 1.7, 1.0)) < 1e-5);
    }

    #[test]
    fn vertex_alpha_and_instance_attributes_reach_the_upload() {
        use sloppy_core::geometry::{Attribute, VERTEX_ALPHA};
        let mut faded = (*quad()).clone();
        faded.colors = vec![[0.5; 3]; 4];
        faded.set_attribute(Attribute::vertex(VERTEX_ALPHA, 1, vec![1.0, 0.5, 0.0, 1.0]));
        faded.set_attribute(Attribute::instance(
            "origin",
            3,
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        ));
        faded.set_attribute(Attribute::instance("phase", 1, vec![0.25, 0.75]));
        let data = mesh_data(&faded, &["origin"]);
        assert_eq!(data.vertices[1].color, [0.5, 0.5, 0.5, 0.5]);
        // Per-instance attributes never become per-vertex effect inputs.
        assert_eq!(data.extra[0], [0.0; 4]);
        assert_eq!(
            instance_attribute_data(&faded, 1),
            Some([4.0, 5.0, 6.0, 0.75])
        );
        assert_eq!(instance_attribute_data(&quad(), 0), None);
        assert_eq!(mesh_data(&quad(), &[]).vertices[0].color, [1.0; 4]);
    }

    #[test]
    fn shared_vertices_stream_the_whole_upload_in_pieces() {
        use sloppy_core::geometry::{Attribute, VERTEX_ALPHA, sphere_geometry};
        let mut mesh = sphere_geometry(1.0, 12, 8);
        let count = mesh.positions.len();
        mesh.colors = (0..count)
            .map(|i| [i as f32 / count as f32, 0.5, 0.25])
            .collect();
        let alpha = (0..count).map(|i| (i % 3) as f32 / 2.0).collect();
        mesh.set_attribute(Attribute::vertex(VERTEX_ALPHA, 1, alpha));
        let origin = (0..count * 3).map(|i| i as f32).collect();
        mesh.set_attribute(Attribute::vertex("origin", 3, origin));
        let whole = mesh_data(&mesh, &["origin"]);
        let (mut vertices, mut extra) = (Vec::new(), Vec::new());
        for start in (0..count).step_by(17) {
            let (v, e) = shared_vertices(&mesh, &["origin"], start..(start + 17).min(count));
            vertices.extend(v);
            extra.extend(e);
        }
        assert_eq!(vertices, whole.vertices);
        assert_eq!(extra, whole.extra);
    }

    #[test]
    fn scenery_merges_by_cell_and_instances_repeats() {
        let mesh = quad();
        let other = quad();
        let stone = Arc::new(Material::standard(0x888888, 0.0, 0.9));
        let moss = Arc::new(Material::standard(0x335533, 0.0, 0.9));
        let mut root = Node::group("yard");
        for i in 0..30 {
            // Thirty identical crates become one instanced part.
            root.children.push(part(&mesh, &stone, i as f64 * 3.0));
        }
        for x in [0.0, 5.0, 100.0] {
            root.children.push(part(&other, &moss, x));
        }
        let mut hidden = part(&other, &moss, 7.0);
        hidden.visible = false;
        root.children.push(hidden);
        let mut grass = Node::mesh(other.clone(), moss.clone());
        grass.drawable.as_mut().unwrap().instances = Some(vec![
            Instance {
                matrix: DMat4::from_translation(DVec3::new(1.0, 0.0, 1.0)),
                color: Some([0.5, 1.0, 0.5]),
            };
            3
        ]);
        root.children.push(grass);
        let mut interner = MaterialInterner::default();
        let scenery = prepare_scenery(
            &root,
            &mut interner,
            &no_attributes,
            SceneryOptions::default(),
        );
        let instanced: Vec<_> = scenery
            .parts
            .iter()
            .filter(|p| p.instances.is_some())
            .collect();
        assert_eq!(instanced.len(), 2);
        assert!(
            instanced
                .iter()
                .any(|p| p.instances.as_ref().unwrap().len() == 30)
        );
        let merged: Vec<_> = scenery
            .parts
            .iter()
            .filter(|p| p.instances.is_none())
            .collect();
        // Moss at x 0 and 5 share a cell and merge; x 100 is another cell.
        assert_eq!(merged.len(), 2);
        let vertices: usize = scenery.meshes.iter().map(|m| m.vertices.len()).sum();
        assert_eq!(vertices, 8);
    }
}
