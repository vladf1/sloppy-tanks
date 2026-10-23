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
use crate::effects::EffectRegistry;
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

/// How a material's shadow can come from a merged caster: depth only, or an
/// alpha-tested card; never with an effect that moves vertices, dithers the
/// shadow or (for cards) could change the cut-out alpha, unless the effect
/// declares a still shadow (foliage sway).
pub fn shadow_merge_kind(
    effects: &EffectRegistry,
    material: &sloppy_core::scene::Material,
) -> MergeKind {
    let effect = match &material.effect {
        sloppy_core::scene::Effect::None => None,
        sloppy_core::scene::Effect::Custom { name, params } => {
            effects.id_for(name, params).and_then(|id| effects.get(id))
        }
    };
    let still = effect.is_some_and(|effect| effect.still_shadow);
    let moves = effect.is_some_and(|effect| {
        effect.has_vertex() || effect.has_world() || effect.has_clip() || effect.shadow_fade
    });
    if moves && !still {
        MergeKind::Separate
    } else if material.alpha_test > 0.0 {
        if effect.is_some() && !still {
            MergeKind::Separate
        } else {
            MergeKind::Cutout
        }
    } else {
        MergeKind::Opaque
    }
}

/// The fewest fixed scenery shadow triangles worth caching. A depth copy has a fixed
/// bandwidth cost; small static sets are cheaper to redraw. Small sets keep the
/// existing view culling and avoid copying unused depth.
pub const MIN_CACHED_SHADOW_TRIANGLES: u64 = 131_072;

/// Where a frame's merged shadow item draws: by pipeline, then by the caster's
/// vertex and index pages, so draws from one page go together, then by model and
/// group, so each group's items stay together and draw instanced. One integer
/// orders like that tuple: the renderer sorts a few hundred items a frame, and an
/// integer compares in a few instructions where the tuple took a chain of branches.
pub fn merged_draw_order(
    pipeline: u32,
    vertex_page: u16,
    index_page: u16,
    model: u32,
    group: u32,
) -> u128 {
    u128::from(pipeline) << 96
        | u128::from(vertex_page) << 80
        | u128::from(index_page) << 64
        | u128::from(model) << 32
        | u128::from(group)
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

/// Merge the eligible parts of a prepared model. `kind(part_index)` says how a
/// part may merge (see [`MergeKind`]; on movable models only non-instanced
/// parts merge). `scenery` bakes world
/// transforms (and InstancedMesh placements) into the vertices, grouped per
/// `cell_size` cell; otherwise vertices stay in mesh space with a slot per part.
/// Every group has triangles.
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
    // Every merged placement and its group, found first, so each group is allocated
    // once at its final size: grown part by part, the groups held up to twice their
    // vertices, and old and new buffers side by side, at the heap's peak.
    let mut placed: Vec<Placed> = Vec::new();
    let mut sizes: Vec<(usize, usize)> = Vec::new();
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
        let (vertex_count, index_count) = part_counts(part, &model.meshes);
        if index_count == 0 {
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
        let side = shadow_side(&part.material);
        for (instance, placement) in placements.into_iter().enumerate() {
            // Transformed on the fly, twice, rather than into a copy of the part.
            let moved = |p: Vec3| {
                if scenery {
                    placement.transform_point3(p)
                } else {
                    p
                }
            };
            let sphere = part_bounds(part, &model.meshes, moved);
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
                    sizes.push((0, 0));
                    merge.groups.push(ShadowGroup {
                        side,
                        cutout: cutout.clone(),
                        ..ShadowGroup::default()
                    });
                    merge.groups.len() - 1
                }
            };
            sizes[group].0 += vertex_count;
            sizes[group].1 += index_count;
            placed.push(Placed {
                part: index as u32,
                slot,
                group: group as u32,
                instance: instance as u32,
            });
            bounds[group] = Some(bounds[group].map_or(sphere, |b| b.union(&sphere)));
        }
    }
    for (group, &(vertices, indices)) in merge.groups.iter_mut().zip(&sizes) {
        group.vertices.reserve_exact(vertices);
        group.indices.reserve_exact(indices);
    }
    // A part's placements are consecutive: its geometry is gathered once.
    let mut geometry: Option<(u32, Geometry)> = None;
    for item in &placed {
        let part = &model.parts[item.part as usize];
        if geometry
            .as_ref()
            .is_none_or(|(index, _)| *index != item.part)
        {
            geometry = Some((item.part, part_geometry(part, &model.meshes)));
        }
        let (_, (positions, uvs, indices)) = geometry.as_ref().expect("gathered above");
        let placement = scenery.then(|| match &part.instances {
            Some(list) => part.local * list[item.instance as usize].matrix,
            None => part.local,
        });
        let target = &mut merge.groups[item.group as usize];
        let base = target.vertices.len() as u32;
        target
            .vertices
            .extend(positions.iter().zip(uvs).map(|(p, uv)| {
                ShadowVertex {
                    position: placement
                        .map_or(*p, |placement| placement.transform_point3(*p))
                        .to_array(),
                    slot: item.slot,
                    uv: uv.to_array(),
                }
            }));
        target.indices.extend(indices.iter().map(|i| base + i));
    }
    for (group, sphere) in merge.groups.iter_mut().zip(bounds) {
        group.bounds = sphere.unwrap_or_default();
    }
    merge
}

/// One merged caster placement: its part, slot and group, and for scenery which of
/// the part's instances it is (its world placement is found again when filled).
struct Placed {
    part: u32,
    slot: u32,
    group: u32,
    instance: u32,
}

/// The vertex and triangle-index counts [`part_geometry`] gathers for a part.
fn part_counts(part: &PreparedPart, meshes: &[MeshData]) -> (usize, usize) {
    match &part.mesh {
        PartMesh::Shared(mesh) => {
            let vertices = mesh.positions.len();
            (vertices, mesh.indices.as_ref().map_or(vertices, Vec::len))
        }
        PartMesh::Owned(index) => (meshes[*index].vertices.len(), meshes[*index].indices.len()),
    }
}

/// The bounds of a part's positions moved by `moved`, read in place.
fn part_bounds(part: &PreparedPart, meshes: &[MeshData], moved: impl Fn(Vec3) -> Vec3) -> Sphere {
    match &part.mesh {
        PartMesh::Shared(mesh) => {
            Sphere::from_points(mesh.positions.iter().map(|p| moved(Vec3::from(*p))))
        }
        PartMesh::Owned(index) => {
            let vertices = meshes[*index].vertices.iter();
            Sphere::from_points(vertices.map(|v| moved(Vec3::from(v.position))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::MaterialInterner;
    use crate::model::{prepare_model, prepare_scenery};
    use glam::DVec3;
    use sloppy_core::geometry::box_geometry;
    use sloppy_core::scene::{Material, Node};
    use std::sync::Arc;

    #[test]
    fn merged_draw_order_compares_like_its_fields() {
        let mut random = crate::effects::random::CosmeticRandom::seeded(5);
        let mut pick = |n: f64| (random.next_f64() * n) as u32;
        let mut items: Vec<(u32, u16, u16, u32, u32)> = (0..300)
            .map(|_| {
                (
                    pick(6.0),
                    pick(4.0) as u16,
                    pick(3.0) as u16,
                    pick(40.0),
                    pick(5.0),
                )
            })
            .collect();
        // The extremes of every field too.
        items.push((5, u16::MAX, u16::MAX, u32::MAX, u32::MAX));
        items.push((0, 0, 0, 0, 0));
        items.push((1, 0, u16::MAX, 0, u32::MAX));
        let order =
            |&(pipeline, vertex_page, index_page, model, group): &(u32, u16, u16, u32, u32)| {
                merged_draw_order(pipeline, vertex_page, index_page, model, group)
            };
        for a in &items {
            for b in &items {
                assert_eq!(order(a).cmp(&order(b)), a.cmp(b), "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn fixed_casters_exclude_animated_or_alpha_changing_effects() {
        use crate::effects::registry::{PULSE, WAVE};
        use sloppy_core::scene::Effect;
        let mut effects = EffectRegistry::default();
        effects.register(WAVE);
        effects.register(PULSE);
        let mut material = Material::default();
        assert_eq!(shadow_merge_kind(&effects, &material), MergeKind::Opaque);
        material.alpha_test = 0.5;
        assert_eq!(shadow_merge_kind(&effects, &material), MergeKind::Cutout);
        material.effect = Effect::Custom {
            name: PULSE.name,
            params: vec![0.0; 16],
        };
        assert_eq!(shadow_merge_kind(&effects, &material), MergeKind::Separate);
        material.alpha_test = 0.0;
        material.effect = Effect::Custom {
            name: WAVE.name,
            params: vec![0.0; 16],
        };
        assert_eq!(shadow_merge_kind(&effects, &material), MergeKind::Separate);
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
        let model = prepare_model(&root, &mut interner);
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
        let scenery = prepare_scenery(&root, &mut interner);
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

    /// Merged geometry is allocated once at its final size: a merge that grew part by
    /// part left up to twice its vertices allocated at the heap's peak.
    #[test]
    fn merges_allocate_their_final_size() {
        let cube = Arc::new(box_geometry(1.0, 1.0, 1.0));
        let mut root = Node::group("yard");
        for i in 0..37 {
            root.children.push(caster(
                &cube,
                Material::standard(0x888888, 0.0, 0.9),
                f64::from(i) * 2.0,
            ));
        }
        let mut interner = MaterialInterner::default();
        let model = prepare_model(&root, &mut interner);
        let merged = &model.meshes[0];
        assert_eq!(merged.vertices.len(), 37 * cube.positions.len());
        assert_eq!(merged.vertices.capacity(), merged.vertices.len());
        assert_eq!(merged.indices.capacity(), merged.indices.len());
        for scenery in [false, true] {
            let prepared = if scenery {
                prepare_scenery(&root, &mut interner)
            } else {
                prepare_model(&root, &mut interner)
            };
            let merge = merge_shadows(&prepared, |_| MergeKind::Opaque, scenery, 60.0);
            for group in &merge.groups {
                assert_eq!(group.vertices.capacity(), group.vertices.len());
                assert_eq!(group.indices.capacity(), group.indices.len());
            }
        }
    }
}
