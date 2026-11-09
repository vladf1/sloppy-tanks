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
use super::senda_scenery::{is_yard_wall, yard_wall};
use super::quarry_surfaces::{
    RockSetting, RubbleStone, STONE_SEAT, sandstone_footing, sandstone_footing_height,
    sandstone_rock, sandstone_rubble,
};
use super::timber_model::{add_timber_parts, timber_part_model};
use super::tower_model::{tower, tower_debris};
use super::wheelie_bin::wheelie_bin;
use super::tree_models::{TreeDetail, TreeShape, tree_model};
use crate::scene::Node;
use crate::sim::math::Random;
use crate::sim::quarry_rock_shape::quarry_rock_variant;
use crate::sim::house_layout::HousePlan;
use crate::sim::render_state::RenderCover;
use crate::sim::arena::cover_turn;
use crate::sim::senda_layout::{
    is_trash_bin,
    SENDA_FENCE_COLOR, SENDA_WALL_COLOR, WALL_THICKNESS, WallFace,
};
use crate::sim::senda_yards::{YARD_VIEW_COLOR, YARD_WALL_COLOR};
use crate::sim::crate_structure::{CRATE_WALLS, CratePart, crate_structure};
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
    /// A house its map lays out; see [`HousePlan`].
    pub house_plan: Option<HousePlan>,
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
            house_plan: cover.house_plan,
            broken: cover.broken,
            cracked: cover.cracked,
        }
    }
}

/// `coverModel(c, detail)`: rebuild it when a structure's broken parts change.
pub fn cover_model(c: &CoverShape, detail: TreeDetail) -> Node {
    if c.kind == CoverKind::Tree {
        return tree_model(
            &TreeShape {
                x: c.x,
                z: c.z,
                w: c.w,
                d: c.d,
                h: c.h,
            },
            detail,
        );
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
        CoverKind::House => {
            structural_house(group, c.x, c.z, c.w, c.d, c.h, c.house_plan, &c.broken)
        }
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
        // The Neighborhood's wheelie bins, turned to face their street.
        CoverKind::Drum if is_trash_bin(c.color) => {
            let mut bin = wheelie_bin(c.color);
            bin.rotation = glam::DQuat::from_rotation_y(cover_turn(c.kind, c.house_plan));
            group.children.push(bin);
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
        CoverKind::Boundary if c.color == SENDA_WALL_COLOR || c.color == SENDA_FENCE_COLOR => {
            tract_wall(group, c);
            // A run down Senda Parkway's verge turns with its plan, as a lot's wall does.
            group.rotation = glam::DQuat::from_rotation_y(cover_turn(c.kind, c.house_plan));
        }
        CoverKind::Boundary if c.color == QUARRY_WALL_COLOR => {
            put(group, cut_stone_wall(c.w, c.h, c.d), 0.0, 0.0, 0.0);
        }
        CoverKind::Boundary if c.color == HARBOR_WALL_COLOR => {
            put(group, harbor_parapet(c.w, c.h, c.d), 0.0, c.h / 2.0, 0.0);
        }
        // Senda's lot walls and gates.
        CoverKind::Concrete if is_yard_wall(c.color) => {
            let turn = cover_turn(c.kind, c.house_plan);
            yard_wall(group, (c.w, c.d, c.h), c.color, turn);
        }
        // Harbor Havoc's precast barriers.
        CoverKind::Concrete => {
            put(group, concrete_barrier(c.w, c.h, c.d), 0.0, 0.0, 0.0);
        }
        CoverKind::Boundary => {
            put(group, concrete_wall(c.w, c.h, c.d), 0.0, c.h / 2.0, 0.0);
        }
    }
    node
}

/// The most (m) a tract wall's run goes between pilasters.
const TRACT_PILASTER_SPACING: f64 = 5.5;
/// A pilaster's side (m), as a lot wall's: the short piece at the far end of a run that
/// stands one there.
const TRACT_END_PIECE: f64 = 0.36;

/// Senda's edge, a run of the tract's own wall: capped split-face block, or along the
/// open ground a black view fence on a low block base, both as the lots' walls are built,
/// 0.2 m thick on the inside face of the run's cover (which keeps its depth outside it
/// for the collider alone), from end to end of the run, with a capped pilaster every few
/// metres and at both ends.
fn tract_wall(group: &mut Node, c: &CoverShape) {
    let Some(plan) = c.house_plan else {
        return;
    };
    let face = WallFace::of(plan);
    let long_x = face.along_x;
    let (length, thick) = if long_x { (c.w, c.d) } else { (c.d, c.w) };
    let (from, to) = (-length / 2.0 + face.reach[0], length / 2.0 - face.reach[1]);
    // On the inside face, and on past the run's ends as far as a wall this thick needs
    // to close an outer corner.
    let close = WALL_THICKNESS / thick;
    let (from, to) = (from - face.reach[0] * close, to + face.reach[1] * close);
    let across = face.inside * (thick - WALL_THICKNESS) / 2.0;
    let color = if c.color == SENDA_FENCE_COLOR {
        YARD_VIEW_COLOR
    } else {
        YARD_WALL_COLOR
    };
    let pieces = ((to - from) / TRACT_PILASTER_SPACING).ceil().max(1.0) as usize;
    let piece = (to - from) / pieces as f64;
    let mut place = |u: f64, length: f64| {
        let mut run = Node::default();
        let (w, d) = if long_x {
            run.position = DVec3::new(u, 0.0, across);
            (length, WALL_THICKNESS)
        } else {
            run.position = DVec3::new(across, 0.0, u);
            (WALL_THICKNESS, length)
        };
        yard_wall(&mut run, (w, d, c.h), color, 0.0);
        group.children.push(run);
    };
    for k in 0..pieces {
        place(from + piece * (k as f64 + 0.5), piece);
    }
    // A lot wall's run stands its pilaster at its start: along x its negative end, along
    // z (turned a quarter) its positive one; a short piece stands one at the other end.
    let end = if long_x {
        to - TRACT_END_PIECE / 2.0
    } else {
        from + TRACT_END_PIECE / 2.0
    };
    place(end, TRACT_END_PIECE);
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
        CoverKind::House => house_debris(c.x, c.z, c.w, c.d, c.h, c.house_plan, parts),
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
        _ => Node::default(),
    }
}
