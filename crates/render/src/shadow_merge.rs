//! Shadow-pass merging: the sun's depth pass needs no material, so every opaque
//! caster of a model (or of the static scenery) that no vertex effect moves can
//! draw from one merged, position-only mesh per cull side.
//!
//! Each merged vertex carries a *slot*: the index of its part's instance record
//! among the records written for that model instance (the parts' posed world
//! transforms), so joints still move, hide (a zero record) and instance: one
//! draw covers every instance of a model. Static scenery bakes its world
//! transforms in and uses the identity record (slot 0) per spatial cell.
//!
//! Alpha-tested cards (foliage) merge too, per material: their vertices keep
//! UVs and the merged draw binds the material to discard below its cutoff.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};
use sloppy_core::scene::{Material, Side};

use crate::camera::Sphere;
use crate::material::shadow_side;
use crate::model::{MeshData, PartMesh, PreparedModel, PreparedPart};

/// A merged shadow vertex (24 bytes): mesh-space position, its record slot and
/// UV (read only by alpha-tested groups).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct ShadowVertex {
    pub position: [f32; 3],
    pub slot: u32,
    pub uv: [f32; 2],
}

/// How a part's shadow may merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeKind {
    /// Keeps its own shadow draw.
    Separate,
    /// Depth only.
    Opaque,
    /// Alpha-tested against its material's map (grouped per material).
    Cutout,
}

/// One merged caster mesh for a cull side.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShadowGroup {
    /// The faces the shadow pass draws (`Material.shadow_side` semantics).
    pub side: Side,
    pub vertices: Vec<ShadowVertex>,
    pub indices: Vec<u32>,
    /// Scenery: world bounds of the group; models: unused (culled per instance).
    pub bounds: Sphere,
    /// The alpha-tested material all of the group's cards share.
    pub cutout: Option<Arc<Material>>,
}

/// A model's shadow merge.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShadowMerge {
    pub groups: Vec<ShadowGroup>,
    /// Movable models: the part indices whose records fill slots 0.., in order.
    /// Empty for scenery, whose vertices are already in world space.
    pub slots: Vec<usize>,
    /// Per part: whether it draws its shadow from a merged group.
    pub merged: Vec<bool>,
}

impl ShadowMerge {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

type Geometry<'a> = (Vec<Vec3>, Vec<Vec2>, std::borrow::Cow<'a, [u32]>);

/// Positions, UVs and triangle indices of a part's mesh.
fn part_geometry<'a>(part: &'a PreparedPart, meshes: &'a [MeshData]) -> Geometry<'a> {
    match &part.mesh {
        PartMesh::Shared(mesh) => {
            let positions = mesh.positions.iter().map(|p| Vec3::from(*p)).collect();
            let uvs = (0..mesh.positions.len())
                .map(|i| mesh.uvs.get(i).map_or(Vec2::ZERO, |uv| Vec2::from(*uv)))
                .collect();
            let indices = match &mesh.indices {
                Some(indices) => std::borrow::Cow::Borrowed(indices.as_slice()),
                None => std::borrow::Cow::Owned((0..mesh.positions.len() as u32).collect()),
            };
            (positions, uvs, indices)
        }
        PartMesh::Owned(index) => {
            let data = &meshes[*index];
            let positions = data
                .vertices
                .iter()
                .map(|v| Vec3::from(v.position))
                .collect();
            let uvs = data.vertices.iter().map(|v| Vec2::from(v.uv)).collect();
            (
                positions,
                uvs,
                std::borrow::Cow::Borrowed(data.indices.as_slice()),
            )
        }
    }
}

/// The cull side a caster's shadow uses.
pub fn caster_side(part: &PreparedPart) -> Side {
    part.material
        .shadow_side
        .unwrap_or_else(|| shadow_side(part.material.side))
}

/// Merge the eligible parts of a prepared model. `kind(part_index)` says how a
/// part may merge (see [`MergeKind`]; on movable models only non-instanced
/// parts merge). `scenery` bakes world
/// transforms (and InstancedMesh placements) into the vertices, grouped per
/// `cell_size` cell; otherwise vertices stay in mesh space with a slot per part.
pub fn merge_shadows(
    model: &PreparedModel,
    kind: impl Fn(usize) -> MergeKind,
    scenery: bool,
    cell_size: f32,
) -> ShadowMerge {
    let mut merge = ShadowMerge {
        merged: vec![false; model.parts.len()],
        ..ShadowMerge::default()
    };
    // Groups keyed by (side, cell, cutout material), in first-use order.
    let mut keys: Vec<(Side, (i32, i32), usize)> = Vec::new();
    let mut bounds: Vec<Option<Sphere>> = Vec::new();
    for (index, part) in model.parts.iter().enumerate() {
        let merge_kind = if part.cast_shadow {
            kind(index)
        } else {
            MergeKind::Separate
        };
        if merge_kind == MergeKind::Separate {
            continue;
        }
        let cutout = (merge_kind == MergeKind::Cutout).then(|| part.material.clone());
        let material_key = cutout.as_ref().map_or(0, |m| Arc::as_ptr(m) as usize);
        if !scenery && part.instances.is_some() {
            continue;
        }
        let (positions, uvs, indices) = part_geometry(part, &model.meshes);
        if indices.is_empty() {
            continue;
        }
        let placements: Vec<Mat4> = if scenery {
            match &part.instances {
                Some(list) => list.iter().map(|item| part.local * item.matrix).collect(),
                None => vec![part.local],
            }
        } else {
            vec![Mat4::IDENTITY]
        };
        let slot = if scenery {
            0
        } else {
            merge.slots.push(index);
            merge.slots.len() as u32 - 1
        };
        merge.merged[index] = true;
        let side = caster_side(part);
        for placement in placements {
            // Transformed on the fly, twice, rather than into a copy of the part.
            let moved = |p: &Vec3| {
                if scenery {
                    placement.transform_point3(*p)
                } else {
                    *p
                }
            };
            let sphere = Sphere::from_points(positions.iter().map(moved));
            let cell = if scenery && cell_size > 0.0 {
                (
                    (sphere.center.x / cell_size).floor() as i32,
                    (sphere.center.z / cell_size).floor() as i32,
                )
            } else {
                (0, 0)
            };
            let key = (side, cell, material_key);
            let group = match keys.iter().position(|k| *k == key) {
                Some(group) => group,
                None => {
                    keys.push(key);
                    bounds.push(None);
                    merge.groups.push(ShadowGroup {
                        side,
                        cutout: cutout.clone(),
                        ..ShadowGroup::default()
                    });
                    merge.groups.len() - 1
                }
            };
            let target = &mut merge.groups[group];
            let base = target.vertices.len() as u32;
            target.vertices.reserve(positions.len());
            target.indices.reserve(indices.len());
            target
                .vertices
                .extend(
                    positions
                        .iter()
                        .map(moved)
                        .zip(&uvs)
                        .map(|(p, uv)| ShadowVertex {
                            position: p.to_array(),
                            slot,
                            uv: uv.to_array(),
                        }),
                );
            target.indices.extend(indices.iter().map(|i| base + i));
            bounds[group] = Some(bounds[group].map_or(sphere, |b| b.union(&sphere)));
        }
    }
    for (group, sphere) in merge.groups.iter_mut().zip(bounds) {
        group.bounds = sphere.unwrap_or_default();
    }
    merge
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::MaterialInterner;
    use crate::model::{SceneryOptions, prepare_model, prepare_scenery};
    use glam::DVec3;
    use sloppy_core::geometry::box_geometry;
    use sloppy_core::scene::{Material, Node};
    use std::sync::Arc;

    fn no_attributes(_: &Material) -> &'static [&'static str] {
        &[]
    }

    fn caster(mesh: &Arc<sloppy_core::geometry::Mesh>, material: Material, x: f64) -> Node {
        let mut node = Node::mesh(mesh.clone(), Arc::new(material));
        node.position = DVec3::new(x, 0.0, 0.0);
        if let Some(drawable) = &mut node.drawable {
            drawable.cast_shadow = true;
        }
        node
    }

    #[test]
    fn a_models_casters_merge_across_materials_with_a_slot_per_part() {
        let cube = Arc::new(box_geometry(1.0, 1.0, 1.0));
        let mut root = Node::group("tank");
        root.children
            .push(caster(&cube, Material::standard(0xff0000, 0.0, 0.5), 0.0));
        let mut turret = Node::group("turret");
        turret
            .children
            .push(caster(&cube, Material::standard(0x00ff00, 0.9, 0.2), 0.0));
        turret.children.push(caster(
            &cube,
            Material {
                alpha_test: 0.5,
                ..Material::default()
            },
            1.0,
        ));
        turret.children.push(caster(
            &cube,
            Material {
                side: Side::Double,
                ..Material::default()
            },
            2.0,
        ));
        root.children.push(turret);
        let mut interner = MaterialInterner::default();
        let model = prepare_model(&root, &mut interner, &no_attributes);
        let kind = |index: usize| {
            if model.parts[index].material.alpha_test == 0.0 {
                MergeKind::Opaque
            } else {
                MergeKind::Separate
            }
        };
        let merge = merge_shadows(&model, kind, false, 0.0);
        // Front-sided casters share one group (their back faces); double-sided
        // ones get their own; the alpha-tested card keeps its own shadow draw.
        assert_eq!(merge.groups.len(), 2);
        assert_eq!(merge.slots.len(), model.parts.len() - 1);
        assert_eq!(merge.merged.iter().filter(|m| !**m).count(), 1);
        // As a cutout it merges into a group of its own material.
        let cutouts = merge_shadows(&model, |_| MergeKind::Cutout, false, 0.0);
        assert!(cutouts.groups.iter().all(|g| g.cutout.is_some()));
        assert_eq!(cutouts.slots.len(), model.parts.len());
        let back = merge.groups.iter().find(|g| g.side == Side::Back).unwrap();
        let slots: std::collections::BTreeSet<u32> = back.vertices.iter().map(|v| v.slot).collect();
        assert!(!slots.is_empty());
        assert_eq!(back.indices.len() % 3, 0);
    }

    #[test]
    fn scenery_bakes_world_space_per_cell() {
        let cube = Arc::new(box_geometry(1.0, 1.0, 1.0));
        let mut root = Node::group("yard");
        for x in [0.0, 5.0, 200.0] {
            root.children
                .push(caster(&cube, Material::standard(0x888888, 0.0, 0.9), x));
        }
        let mut interner = MaterialInterner::default();
        let scenery = prepare_scenery(
            &root,
            &mut interner,
            &no_attributes,
            SceneryOptions::default(),
        );
        let merge = merge_shadows(&scenery, |_| MergeKind::Opaque, true, 60.0);
        assert_eq!(merge.groups.len(), 2);
        assert!(merge.slots.is_empty());
        let far = merge
            .groups
            .iter()
            .find(|g| g.bounds.center.x > 100.0)
            .unwrap();
        assert!(
            far.vertices
                .iter()
                .all(|v| v.slot == 0 && v.position[0] > 199.0)
        );
    }
}
