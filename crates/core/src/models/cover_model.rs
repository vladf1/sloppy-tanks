//! Port of `cover-model.ts`: the model of every cover kind.
//!
//! `cover_model` returns the unbatched tree like the TypeScript: a group at the
//! cover's x/z whose direct mesh children presentation batches (`batch`) before
//! drawing. Trees are delegated to [`tree_model`]; a structure (a cottage, a
//! watchtower, a timber wall or a cargo crate) is drawn without its broken parts.

use std::f64::consts::PI;

use glam::DVec3;

use super::barrel_surfaces::{LID_DROP, explosive_barrel};
use super::building_kit::fieldstone_wall;
use super::concrete_surfaces::{concrete_barrier, concrete_wall};
use super::crate_model::{CrateShape, cargo_crate, crate_piece_model};
use super::harbor_models::{CargoShape, shipping_container};
use super::harbor_surfaces::harbor_parapet;
use super::house_parts_model::{house_debris, structural_house};
use super::model_primitives::{cylinder_part, put};
use super::quarry_barriers::{cut_stone_wall, dragon_tooth, steel_hedgehog};
use super::quarry_surfaces::{
    RockSetting, RubbleStone, STONE_SEAT, sandstone_footing, sandstone_footing_height,
    sandstone_rock, sandstone_rubble,
};
use super::timber_model::{add_timber_parts, timber_part_model};
use super::tower_model::{tower, tower_debris};
use super::tree_models::{
    TreeDetail, TreeShape, oak_pieces, standing_tree_model, tree_model, tree_pieces,
};
use crate::scene::Node;
use crate::sim::math::Random;
use crate::sim::quarry_rock_shape::quarry_rock_variant;
use crate::sim::render_state::RenderCover;
use crate::sim::crate_structure::{CRATE_WALLS, CratePart, crate_structure};
use crate::sim::oak_layout::OAK;
use crate::sim::tree_proportions::tree_proportions;
use crate::sim::tree_structure::{TreePart, tree_structure};
use crate::sim::structure::{PartSet, Role};
use crate::sim::timber_layout::{
    TimberJoin, TimberPart, TimberPartKind, TimberWall, post_partner, timber_parts, whole_post,
};
use crate::sim::types::CoverKind;

/// Pine Village's boundary colour (`sim::arena::authored_layout`): its walls are
/// built in fieldstone, other maps' boundaries in concrete.
const VILLAGE_WALL_COLOR: u32 = 0xa68c68;
/// Dusty Dig's boundary colour (`sim::quarry_layout`): its walls are cut sandstone blocks.
const QUARRY_WALL_COLOR: u32 = 0xd2bd99;
/// Harbor Havoc's boundary colour (`sim::harbor_layout`): its walls are the sea
/// wall's parapet.
const HARBOR_WALL_COLOR: u32 = 0x879698;

/// The cover fields the models read (`Pick<Cover, "kind" | "x" | ... >`).
#[derive(Clone, Debug, PartialEq)]
pub struct CoverShape {
    pub kind: CoverKind,
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub h: f64,
    pub color: u32,
    pub timber_join: Option<TimberJoin>,
    /// A structure's parts broken or fallen from it.
    pub broken: PartSet,
    /// A watchtower's parts that have been struck, standing or not (a cottage's look
    /// ignores it).
    pub cracked: PartSet,
}

impl From<&RenderCover> for CoverShape {
    fn from(cover: &RenderCover) -> Self {
        Self {
            kind: cover.kind,
            x: cover.x,
            z: cover.z,
            w: cover.w,
            d: cover.d,
            h: cover.h,
            color: cover.color,
            timber_join: cover.timber_join,
            broken: cover.broken,
            cracked: cover.cracked,
        }
    }
}

/// `coverModel(c, detail)`: rebuild it when a structure's broken parts change.
pub fn cover_model(c: &CoverShape, detail: TreeDetail) -> Node {
    if c.kind == CoverKind::Tree {
        let tree = TreeShape {
            x: c.x,
            z: c.z,
            w: c.w,
            d: c.d,
            h: c.h,
        };
        if c.broken.is_empty() || detail != TreeDetail::Full {
            return tree_model(&tree, detail);
        }
        // Limbs a shell snapped off fell away, both halves of them.
        let layout = tree_structure(c.x, c.z, c.w, c.d, c.h);
        let gone = |part: TreePart| {
            let whole = match part {
                TreePart::LimbPiece { limb, .. } => TreePart::Limb(limb),
                part => part,
            };
            c.broken
                .iter()
                .any(|index| layout.part(index).role == Role::Tree(whole))
        };
        return standing_tree_model(&tree, detail, &|part| !gone(part));
    }
    let mut node = Node::default();
    let group = &mut node;
    group.position = DVec3::new(c.x, 0.0, c.z);
    match c.kind {
        CoverKind::Teeth => dragon_tooth(group, c.w, c.h, c.d, c.x, c.z),
        CoverKind::Hedgehog => {
            steel_hedgehog(group);
            group.scale = DVec3::new(c.w / 2.9, c.h / 2.7, c.d / 3.2);
        }
        CoverKind::Rock => rock(group, c),
        CoverKind::Container => shipping_container(
            group,
            CargoShape {
                w: c.w,
                d: c.d,
                h: c.h,
                color: c.color,
            },
        ),
        CoverKind::Cargo => cargo_crate(group, crate_shape(c), &c.broken),
        // The parts of a cottage that still stand; an untouched one keeps its place
        // in the static covers' draws.
        CoverKind::House => structural_house(group, c.x, c.z, c.w, c.d, c.h, &c.broken),
        CoverKind::Timber => {
            let parts = timber_parts(&TimberWall {
                x: c.x,
                z: c.z,
                w: c.w,
                d: c.d,
                h: c.h,
                color: c.color,
                join: c.timber_join,
            });
            // The members that stand, numbered as the wall's structure numbers them: a
            // post over its stump draws whole, as it was built.
            let stands = |index: usize| !c.broken.contains(index);
            let standing: Vec<TimberPart> = parts
                .iter()
                .enumerate()
                .filter(|&(index, _)| stands(index))
                .filter_map(|(_, member)| {
                    let partner = post_partner(&parts, member).is_some_and(stands);
                    match member.kind {
                        TimberPartKind::Post if partner => Some(whole_post(member)),
                        TimberPartKind::Stump if partner => None,
                        _ => Some(*member),
                    }
                })
                .collect();
            add_timber_parts(group, &standing);
        }
        CoverKind::Drum => {
            put(group, explosive_barrel(), 0.0, 0.8, 0.0);
            // Screwed bung caps on the recessed lid, where the atlas paints them.
            let lid = 1.6 - LID_DROP;
            for (x, z, radius) in [(0.3, 0.0, 0.085), (-0.25, 0.16, 0.06)] {
                put(
                    group,
                    cylinder_part(radius, 0.045, 0x3d3a35, 10),
                    x,
                    lid + 0.022,
                    z,
                );
            }
        }
        // The parts of a watchtower that still stand, its footings for good; an
        // untouched one is the model every tower shares.
        CoverKind::Tower => tower(group, c.color, &c.broken, &c.cracked),
        CoverKind::Tree => unreachable!("trees return early"),
        CoverKind::Boundary if c.color == VILLAGE_WALL_COLOR => {
            fieldstone_wall(group, c.w, c.h, c.d, c.x, c.z);
        }
        CoverKind::Boundary if c.color == QUARRY_WALL_COLOR => {
            put(group, cut_stone_wall(c.w, c.h, c.d), 0.0, 0.0, 0.0);
        }
        CoverKind::Boundary if c.color == HARBOR_WALL_COLOR => {
            put(group, harbor_parapet(c.w, c.h, c.d), 0.0, c.h / 2.0, 0.0);
        }
        // Harbor Havoc's precast barriers (the only concrete cover).
        CoverKind::Concrete => {
            put(group, concrete_barrier(c.w, c.h, c.d), 0.0, 0.0, 0.0);
        }
        CoverKind::Boundary => {
            put(group, concrete_wall(c.w, c.h, c.d), 0.0, c.h / 2.0, 0.0);
        }
    }
    node
}

fn crate_shape(c: &CoverShape) -> CrateShape {
    CrateShape {
        x: c.x,
        z: c.z,
        w: c.w,
        d: c.d,
        h: c.h,
        color: c.color,
    }
}

/// A sandstone outcrop on its sand apron, with flat spalls at the foot that read as
/// fallen chips rather than additional obstacles.
fn rock(group: &mut Node, c: &CoverShape) {
    let variant = quarry_rock_variant(c.x, c.z);
    let outcrop = sandstone_rock(c.w, c.h, c.d, variant, RockSetting::Drift);
    put(group, outcrop, 0.0, 0.0, 0.0);
    put(group, sandstone_footing(c.w, c.d, variant), 0.0, 0.0, 0.0);
    let mut rubble = Random::new(f64::from(variant) + 902.0);
    let chips: Vec<RubbleStone> = (0..12)
        .map(|_| {
            let angle = rubble.range(0.0, PI * 2.0);
            let reach = rubble.range(0.56, 0.72);
            let size = rubble.range(0.14, 0.5);
            // Rest on the drift bank rather than under or above it.
            let x = angle.cos() * c.w * reach;
            let z = angle.sin() * c.d * reach;
            let h = rubble.range(0.06, 0.12);
            let d = size * rubble.range(0.6, 1.0);
            let shade = rubble.range(0.6, 0.88);
            RubbleStone {
                x,
                y: sandstone_footing_height(c.w, c.d, variant, x, z) + h * STONE_SEAT,
                z,
                w: size,
                h,
                d,
                rot_y: angle,
                shade,
            }
        })
        .collect();
    put(group, sandstone_rubble(&chips), 0.0, 0.0, 0.0);
}

/// The model of a structure's fallen `parts` (or the pieces a landing broke them into),
/// drawn as they stood about its ground centre, where their body started, batched.
pub fn structure_debris(c: &CoverShape, parts: &PartSet) -> Node {
    match c.kind {
        CoverKind::House => house_debris(c.x, c.z, c.w, c.d, c.h, parts),
        CoverKind::Tower => tower_debris(c.color, parts),
        CoverKind::Timber => {
            let members = timber_parts(&TimberWall {
                x: c.x,
                z: c.z,
                w: c.w,
                d: c.d,
                h: c.h,
                color: c.color,
                join: c.timber_join,
            });
            let mut group = Node::default();
            for member in parts.iter().filter_map(|index| members.get(index)) {
                // Knocked out of its slots or tipped over whole: its sawn ends as it stood.
                let mut node = timber_part_model(member);
                node.position = DVec3::new(member.x, member.y, member.z);
                node.rotation = glam::DQuat::from_rotation_y(member.yaw);
                group.children.push(node);
            }
            let mut root = Node::group("timber-debris");
            root.children.push(group);
            root
        }
        CoverKind::Cargo => {
            let layout = crate_structure(c.w, c.d, c.h);
            let mut group = Node::default();
            for index in parts.iter() {
                let Role::Crate(CratePart {
                    section: Some(section),
                    face,
                    board,
                }) = layout.part(index).role
                else {
                    continue;
                };
                // A wall's frame falling with its boards draws as the whole wall.
                if board.is_some() && face.is_some_and(|face| parts.contains(CRATE_WALLS[face])) {
                    continue;
                }
                let size = DVec3::from_array(section.size);
                let mut node = crate_piece_model(section.piece, size, c.color, index as u32);
                let [rx, ry, rz] = section.rotation;
                node.position = DVec3::from_array(section.center);
                node.rotation = glam::DQuat::from_euler(glam::EulerRot::XYZ, rx, ry, rz);
                group.children.push(node);
            }
            let mut root = Node::group("crate-debris");
            root.children.push(group);
            root
        }
        CoverKind::Tree => {
            let layout = tree_structure(c.x, c.z, c.w, c.d, c.h);
            let keep = |part: TreePart| {
                parts
                    .iter()
                    .any(|index| layout.part(index).role == Role::Tree(part))
            };
            let tree = TreeShape {
                x: c.x,
                z: c.z,
                w: c.w,
                d: c.d,
                h: c.h,
            };
            let mut root = Node::group("tree-debris");
            root.children.push(if tree_proportions(c.x, c.z, c.w, c.d, c.h).family == OAK {
                oak_pieces(&tree, &keep)
            } else {
                tree_pieces(&tree, &keep)
            });
            root
        }
        _ => Node::default(),
    }
}
