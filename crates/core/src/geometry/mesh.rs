use std::collections::HashMap;

use glam::{DMat4, DVec3};

use super::math::{
    normal_matrix, normalize, rotation_x, rotation_y, rotation_z, to_int32, transform_normal,
    transform_point,
};

/// Name of the per-vertex alpha attribute (`item_size` 1) of meshes whose Three.js
/// color attribute had four components, such as the village road shoulders and
/// quarry rock footings. The RGB part stays in [`Mesh::colors`]; with
/// `Material::vertex_colors` the renderer multiplies the fragment alpha by it, as
/// the RGBA vertex color did. Meshes without it are opaque (alpha 1).
pub const VERTEX_ALPHA: &str = "vertex-alpha";

/// One attribute beyond the standard set, read only by the effect shader that
/// names it (for example a foliage sway weight), with `item_size` floats per item.
///
/// A per-vertex attribute has one item per position. A per-instance attribute
/// (Three's `InstancedBufferAttribute`) has one item per entry of the drawable's
/// `instances` (it may hold more, for spare capacity): the renderer packs a mesh's
/// per-instance attributes, in [`Mesh::attributes`] order, into the four floats of
/// each instance's effect data, so they total at most four floats.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    pub name: &'static str,
    pub item_size: u8,
    pub data: Vec<f32>,
    /// One item per instance rather than per vertex.
    pub per_instance: bool,
}

impl Attribute {
    /// A per-vertex attribute.
    pub fn vertex(name: &'static str, item_size: u8, data: Vec<f32>) -> Self {
        Self {
            name,
            item_size,
            data,
            per_instance: false,
        }
    }

    /// A per-instance attribute (see the type docs).
    pub fn instance(name: &'static str, item_size: u8, data: Vec<f32>) -> Self {
        Self {
            name,
            item_size,
            data,
            per_instance: true,
        }
    }
}

/// A triangle list in model units (metres). `normals`, `uvs` and `colors` are either
/// empty or have one entry per position. Without `indices`, every three positions
/// form a triangle, like a non-indexed Three.js BufferGeometry.
///
/// The operations below port the BufferGeometry methods the game used. Like Three,
/// they read the stored f32 values, compute in f64 and store f32 again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Linear RGB vertex colors, used when the material enables vertex colors.
    pub colors: Vec<[f32; 3]>,
    pub indices: Option<Vec<u32>>,
    pub attributes: Vec<Attribute>,
}

/// Widen a stored f32 triple for f64 arithmetic.
pub fn widen(v: [f32; 3]) -> DVec3 {
    DVec3::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2]))
}

/// Store an f64 vector the way a Float32Array assignment does (round to nearest).
pub fn narrow(v: DVec3) -> [f32; 3] {
    [v.x as f32, v.y as f32, v.z as f32]
}

/// Items of `N` f64 values stored as f32, like `new Float32BufferAttribute(array, N)`.
fn narrow_chunks<const N: usize>(values: &[f64]) -> Vec<[f32; N]> {
    values
        .as_chunks::<N>()
        .0
        .iter()
        .map(|item| item.map(|v| v as f32))
        .collect()
}

impl Mesh {
    /// A mesh from f64 arrays, narrowed like `new Float32BufferAttribute(array)`.
    pub fn from_f64(
        positions: &[f64],
        normals: &[f64],
        uvs: &[f64],
        indices: Option<Vec<u32>>,
    ) -> Self {
        Self {
            positions: narrow_chunks(positions),
            normals: narrow_chunks(normals),
            uvs: narrow_chunks(uvs),
            indices,
            ..Self::default()
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.as_ref().map_or(self.positions.len(), Vec::len) / 3
    }

    pub fn attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name == name)
    }

    /// The [`VERTEX_ALPHA`] values, when the mesh has RGBA vertex colors.
    pub fn vertex_alpha(&self) -> Option<&[f32]> {
        self.attribute(VERTEX_ALPHA)
            .filter(|attribute| !attribute.per_instance)
            .map(|attribute| attribute.data.as_slice())
    }

    /// `BufferGeometry.setAttribute` for a custom attribute: replaces one of the
    /// same name.
    pub fn set_attribute(&mut self, attribute: Attribute) {
        self.attributes.retain(|other| other.name != attribute.name);
        self.attributes.push(attribute);
    }

    /// `BufferGeometry.applyMatrix4`: positions by the matrix, normals by its normal
    /// matrix (renormalised). UVs, colors and custom attributes are unchanged.
    pub fn apply_matrix4(&mut self, matrix: &DMat4) -> &mut Self {
        for p in &mut self.positions {
            *p = narrow(transform_point(matrix, widen(*p)));
        }
        if !self.normals.is_empty() {
            let normal_matrix = normal_matrix(matrix);
            for n in &mut self.normals {
                *n = narrow(transform_normal(&normal_matrix, widen(*n)));
            }
        }
        self
    }

    /// `BufferGeometry.translate` (`Matrix4.makeTranslation`).
    pub fn translate(&mut self, x: f64, y: f64, z: f64) -> &mut Self {
        self.apply_matrix4(&DMat4::from_translation(DVec3::new(x, y, z)))
    }

    pub fn rotate_x(&mut self, angle: f64) -> &mut Self {
        self.apply_matrix4(&rotation_x(angle))
    }

    pub fn rotate_y(&mut self, angle: f64) -> &mut Self {
        self.apply_matrix4(&rotation_y(angle))
    }

    pub fn rotate_z(&mut self, angle: f64) -> &mut Self {
        self.apply_matrix4(&rotation_z(angle))
    }

    /// `BufferGeometry.scale` (`Matrix4.makeScale`).
    pub fn scale(&mut self, x: f64, y: f64, z: f64) -> &mut Self {
        self.apply_matrix4(&DMat4::from_scale(DVec3::new(x, y, z)))
    }

    /// `BufferGeometry.center`: translate the bounding box center to the origin.
    pub fn center(&mut self) -> &mut Self {
        let offset = -self.bounding_box().center();
        self.translate(offset.x, offset.y, offset.z)
    }

    /// `BufferGeometry.toNonIndexed`: one vertex per triangle corner. A mesh that is
    /// already non-indexed is returned unchanged (Three warns and returns itself).
    pub fn to_non_indexed(&self) -> Mesh {
        let Some(indices) = &self.indices else {
            return self.clone();
        };
        fn gather<T: Copy>(values: &[T], indices: &[u32]) -> Vec<T> {
            if values.is_empty() {
                return Vec::new();
            }
            indices.iter().map(|&i| values[i as usize]).collect()
        }
        Mesh {
            positions: gather(&self.positions, indices),
            normals: gather(&self.normals, indices),
            uvs: gather(&self.uvs, indices),
            colors: gather(&self.colors, indices),
            indices: None,
            attributes: self
                .attributes
                .iter()
                .map(|attribute| {
                    if attribute.per_instance {
                        return attribute.clone();
                    }
                    let size = usize::from(attribute.item_size);
                    Attribute {
                        data: indices
                            .iter()
                            .flat_map(|&i| {
                                let start = i as usize * size;
                                attribute.data[start..start + size].iter().copied()
                            })
                            .collect(),
                        ..attribute.clone()
                    }
                })
                .collect(),
        }
    }

    /// `BufferGeometry.computeVertexNormals`: area-weighted face normals summed per
    /// shared vertex (indexed) or flat per triangle (non-indexed), then normalised.
    /// The per-vertex sums are stored as f32 between triangles, as Three does.
    pub fn compute_vertex_normals(&mut self) {
        let mut normals = vec![[0.0f32; 3]; self.positions.len()];
        let face_normal = |a: usize, b: usize, c: usize| {
            let (pa, pb, pc) = (
                widen(self.positions[a]),
                widen(self.positions[b]),
                widen(self.positions[c]),
            );
            (pc - pb).cross(pa - pb)
        };
        match &self.indices {
            Some(indices) => {
                for triangle in indices.as_chunks::<3>().0 {
                    let [a, b, c] = [0, 1, 2].map(|k| triangle[k] as usize);
                    let cb = face_normal(a, b, c);
                    for vertex in [a, b, c] {
                        normals[vertex] = narrow(widen(normals[vertex]) + cb);
                    }
                }
            }
            None => {
                for start in (0..self.positions.len() / 3 * 3).step_by(3) {
                    let cb = narrow(face_normal(start, start + 1, start + 2));
                    normals[start..start + 3].fill(cb);
                }
            }
        }
        self.normals = normals;
        self.normalize_normals();
    }

    /// `BufferGeometry.normalizeNormals`.
    pub fn normalize_normals(&mut self) {
        for n in &mut self.normals {
            *n = narrow(normalize(widen(*n)));
        }
    }

    /// `BufferGeometryUtils.toCreasedNormals`: de-indexes, then gives each corner the
    /// normalised sum of the face normals around its position (quantised to 1 cm)
    /// that lie within `crease_angle` of its own face normal.
    pub fn to_creased_normals(&self, crease_angle: f64) -> Mesh {
        let mut result = self.to_non_indexed();
        let crease_dot = crease_angle.cos();
        let hash_multiplier = (1.0 + 1e-10) * 1e2;
        let face_normals: Vec<DVec3> = result
            .positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|corners| {
                let [a, b, c] = corners.map(widen);
                normalize((c - b).cross(a - b))
            })
            .collect();
        // Faces around each quantised position, in ascending face order like the
        // bucket sort in Three's implementation, so sums add in the same order.
        let mut buckets: HashMap<[i32; 3], Vec<usize>> = HashMap::new();
        let keys: Vec<[i32; 3]> = result
            .positions
            .iter()
            .map(|p| p.map(|v| to_int32(f64::from(v) * hash_multiplier)))
            .collect();
        for (vertex, key) in keys.iter().enumerate() {
            buckets.entry(*key).or_default().push(vertex / 3);
        }
        let mut normals = vec![[0.0f32; 3]; result.positions.len()];
        for (vertex, key) in keys.iter().enumerate() {
            let own = face_normals[vertex / 3];
            let sum: DVec3 = buckets[key]
                .iter()
                .map(|&face| face_normals[face])
                .filter(|other| own.dot(*other) > crease_dot)
                .sum();
            normals[vertex] = narrow(normalize(sum));
        }
        result.normals = normals;
        result
    }
}

/// `BufferGeometryUtils.mergeGeometries(geometries)` without groups: concatenates
/// attributes, offsetting indices. Returns `None` when the meshes are not all
/// indexed or all non-indexed, or do not share the same attribute set.
///
/// A mesh without vertices (a degenerate shape, a decal clipped flat against an
/// edge) stores empty attribute arrays where Three kept empty attributes, so it
/// merges with any layout and contributes nothing, as it did in Three.
pub fn merge_geometries(meshes: &[&Mesh]) -> Option<Mesh> {
    let first = meshes.first()?;
    let parts: Vec<&Mesh> = meshes
        .iter()
        .copied()
        .filter(|mesh| mesh.vertex_count() > 0)
        .collect();
    let Some(template) = parts.first() else {
        return Some(Mesh {
            indices: first.indices.as_ref().map(|_| Vec::new()),
            attributes: first
                .attributes
                .iter()
                .map(|attribute| Attribute {
                    data: Vec::new(),
                    ..attribute.clone()
                })
                .collect(),
            ..Mesh::default()
        });
    };
    let layout = |mesh: &Mesh| {
        let mut names: Vec<&str> = mesh.attributes.iter().map(|a| a.name).collect();
        names.sort_unstable();
        (
            mesh.indices.is_some(),
            !mesh.normals.is_empty(),
            !mesh.uvs.is_empty(),
            !mesh.colors.is_empty(),
            names,
        )
    };
    let expected = layout(template);
    if parts.iter().any(|mesh| layout(mesh) != expected) {
        return None;
    }
    // Sized up front: merged scenery reaches hundreds of thousands of vertices,
    // and growing by doubling would copy it repeatedly and keep the slack.
    let vertices: usize = parts.iter().map(|mesh| mesh.vertex_count()).sum();
    fn sized<T>(template: &[T], vertices: usize) -> Vec<T> {
        Vec::with_capacity(if template.is_empty() { 0 } else { vertices })
    }
    let mut merged = Mesh {
        positions: Vec::with_capacity(vertices),
        normals: sized(&template.normals, vertices),
        uvs: sized(&template.uvs, vertices),
        colors: sized(&template.colors, vertices),
        indices: template.indices.as_ref().map(|_| {
            Vec::with_capacity(
                parts
                    .iter()
                    .map(|mesh| mesh.indices.as_ref().map_or(0, Vec::len))
                    .sum(),
            )
        }),
        ..Mesh::default()
    };
    for mesh in &parts {
        if let (Some(target), Some(source)) = (&mut merged.indices, &mesh.indices) {
            let offset = merged.positions.len() as u32;
            target.extend(source.iter().map(|i| i + offset));
        }
        merged.positions.extend_from_slice(&mesh.positions);
        merged.normals.extend_from_slice(&mesh.normals);
        merged.uvs.extend_from_slice(&mesh.uvs);
        merged.colors.extend_from_slice(&mesh.colors);
    }
    for attribute in &template.attributes {
        let mut data = Vec::with_capacity(
            parts
                .iter()
                .map(|mesh| mesh.attribute(attribute.name).map_or(0, |a| a.data.len()))
                .sum(),
        );
        for mesh in &parts {
            let source = mesh.attribute(attribute.name)?;
            if source.item_size != attribute.item_size {
                return None;
            }
            data.extend_from_slice(&source.data);
        }
        merged.attributes.push(Attribute {
            data,
            ..attribute.clone()
        });
    }
    Some(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(offset: f32) -> Mesh {
        Mesh {
            positions: vec![
                [offset, 0.0, 0.0],
                [offset + 1.0, 0.0, 0.0],
                [offset, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            ..Mesh::default()
        }
    }

    #[test]
    fn merging_concatenates_and_rejects_mismatched_layouts() {
        let merged = merge_geometries(&[&triangle(0.0), &triangle(2.0)]).expect("same layout");
        assert_eq!(merged.vertex_count(), 6);
        assert_eq!(merged.positions[3], [2.0, 0.0, 0.0]);
        let mut colored = triangle(4.0);
        colored.colors = vec![[1.0; 3]; 3];
        assert!(merge_geometries(&[&triangle(0.0), &colored]).is_none());
    }

    #[test]
    fn meshes_without_vertices_merge_harmlessly() {
        // A shape that triangulates to nothing has no normals, uvs or colors.
        let empty = Mesh::default();
        let mut colored = triangle(0.0);
        colored.colors = vec![[0.5; 3]; 3];
        let merged = merge_geometries(&[&empty, &colored, &empty]).expect("empty parts merge");
        assert_eq!(merged, colored);
        let only_empty = merge_geometries(&[&empty, &empty]).expect("nothing to merge");
        assert_eq!(only_empty.vertex_count(), 0);
    }

    #[test]
    fn merged_meshes_are_allocated_at_their_size() {
        // Merged scenery is large and kept for the page's lifetime: no growth slack.
        let mut part = triangle(0.0).to_non_indexed();
        part.indices = Some(vec![0, 1, 2]);
        part.set_attribute(Attribute::vertex("origin", 3, vec![0.5; 9]));
        let parts: Vec<Mesh> = (0..37).map(|_| part.clone()).collect();
        let merged = merge_geometries(&parts.iter().collect::<Vec<_>>()).expect("same layout");
        assert_eq!(merged.positions.capacity(), 37 * 3);
        assert_eq!(merged.normals.capacity(), 37 * 3);
        assert_eq!(merged.uvs.capacity(), 37 * 3);
        assert_eq!(merged.colors.capacity(), 0);
        assert_eq!(merged.indices.as_ref().map(Vec::capacity), Some(37 * 3));
        assert_eq!(merged.attributes[0].data.capacity(), 37 * 9);
    }
}
