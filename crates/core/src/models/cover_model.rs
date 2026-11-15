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
use super::imported_house::standing_entry;
use super::model_primitives::{box_part, cylinder_part, put};
use super::quarry_barriers::{cut_stone_wall, dragon_tooth, steel_hedgehog};
use super::senda_scenery::{
    MOW_STRIP, boundary_wall, is_yard_wall, patio_cover, street_lamp, yard_wall,
};
use super::quarry_surfaces::{
    RockSetting, RubbleStone, STONE_SEAT, sandstone_footing, sandstone_footing_height,
    sandstone_rock, sandstone_rubble,
};
use super::timber_model::{add_timber_parts, timber_part_model};
use super::tower_model::{tower, tower_debris};
use super::tree_models::{
    TreeDetail, TreeLook, TreeShape, grown_pieces, standing_tree_model, tree_pieces,
};
use super::wheelie_bin::wheelie_bin;
use crate::scene::Node;
use crate::sim::math::Random;
use crate::sim::quarry_rock_shape::quarry_rock_variant;
use crate::sim::house_layout::{HousePlan, SENDA_STYLES};
use crate::sim::senda_entries::is_entry;
use crate::sim::render_state::RenderCover;
use crate::sim::arena::cover_turn;
use crate::sim::senda_layout::{
    is_senda_paved, is_street_tree, is_trash_bin, street_lamp_look,
    SENDA_FENCE_COLOR, SENDA_SCALE, SENDA_WALL_COLOR, WALL_THICKNESS, WallFace,
};
use crate::sim::senda_yards::{PatioCover, YARD_VIEW_COLOR, YARD_WALL_COLOR};
use crate::sim::crate_structure::{CRATE_WALLS, CratePart, crate_structure};
use crate::sim::grown_layout::grows;
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

/// How a tree cover of `color` is coloured: the Neighborhood's parkway trees as its
/// sycamores, every other as its family.
fn tree_look(color: u32) -> TreeLook {
    if is_street_tree(color) {
        TreeLook::StreetSycamore
    } else {
        TreeLook::Family
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
        let look = tree_look(c.color);
        if c.broken.is_empty() || detail != TreeDetail::Full {
            return standing_tree_model(&tree, detail, look, &|_| true);
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
        return standing_tree_model(&tree, detail, look, &|part| !gone(part));
    }
    // Senda's walls, lamps, bins, loggias and entries are drawn to the level's design and
    // stand scaled about their ground centre, as the level does (a house scales its own).
    let senda = drawn_to_senda_design(c);
    let mut node = Node {
        position: DVec3::new(c.x, 0.0, c.z),
        scale: DVec3::splat(if senda { SENDA_SCALE } else { 1.0 }),
        ..Node::default()
    };
    let design;
    let c = if senda {
        design = CoverShape {
            x: c.x / SENDA_SCALE,
            z: c.z / SENDA_SCALE,
            w: c.w / SENDA_SCALE,
            d: c.d / SENDA_SCALE,
            h: c.h / SENDA_SCALE,
            ..c.clone()
        };
        &design
    } else {
        c
    };
    let group = &mut node;
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
        // The Neighborhood's wheelie bins: their bodies start turned to face their
        // street (`cover_turn`), and a drum is drawn at its body's pose.
        CoverKind::Drum if is_trash_bin(c.color) => group.children.push(wheelie_bin(c.color)),
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
        // Senda's street lamps inside the level.
        CoverKind::Concrete if let Some(tall) = street_lamp_look(c.color) => {
            street_lamp(group, tall);
        }
        // The loggias and pergolas over Senda's patios, turned square to their lots.
        CoverKind::Concrete if let Some(look) = PatioCover::of_color(c.color) => {
            let style = c.house_plan.map_or(0, |plan| plan.style);
            patio_cover(group, look, (c.w, c.d), style);
            group.rotation = glam::DQuat::from_rotation_y(cover_turn(c.kind, c.house_plan));
        }
        // The offices and entries before Senda's homes, turned square to their houses.
        CoverKind::Concrete if is_entry(c.color) => {
            senda_entry(group, c);
            group.rotation = glam::DQuat::from_rotation_y(cover_turn(c.kind, c.house_plan));
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

/// Whether a cover is one of Senda's built to its level's design and drawn scaled
/// ([`SENDA_SCALE`]): its tract wall, street lamps, wheelie bins, loggias and pergolas,
/// entries and lot walls. Its trees stand at their world size, as their structures do,
/// and its houses scale their own models.
fn drawn_to_senda_design(c: &CoverShape) -> bool {
    match c.kind {
        CoverKind::Drum => is_trash_bin(c.color),
        CoverKind::Boundary => c.color == SENDA_WALL_COLOR || c.color == SENDA_FENCE_COLOR,
        CoverKind::Concrete => {
            street_lamp_look(c.color).is_some()
                || PatioCover::of_color(c.color).is_some()
                || is_entry(c.color)
                || is_yard_wall(c.color)
        }
        _ => false,
    }
}

/// How thick (m) the roof slab is over an entry drawn before its house's model is in.
const ENTRY_STAND_IN_ROOF: f64 = 0.15;

/// An office and entry before a Senda home, in its cover's frame: drawn from its house's
/// model, or, until that model is installed, a stucco block under a slab of its tile.
fn senda_entry(group: &mut Node, c: &CoverShape) {
    let Some(plan) = c.house_plan else {
        return;
    };
    if let Some(entry) = standing_entry(plan, c.x, c.z, (c.w, c.d, c.h)) {
        group.children.push(entry);
        return;
    }
    let style = SENDA_STYLES[usize::from(plan.style) % SENDA_STYLES.len()];
    let walls = c.h - ENTRY_STAND_IN_ROOF;
    put(group, box_part(c.w, walls, c.d, style.siding, 0.0), 0.0, walls / 2.0, 0.0);
    put(
        group,
        box_part(c.w, ENTRY_STAND_IN_ROOF, c.d, style.roof, 0.0),
        0.0,
        c.h - ENTRY_STAND_IN_ROOF / 2.0,
        0.0,
    );
}

/// The most (m) a tract wall's run goes between pilasters.
const TRACT_PILASTER_SPACING: f64 = 5.5;
/// A pilaster's side (m), as a lot wall's: the short piece at the far end of a run that
/// stands one there.
const TRACT_END_PIECE: f64 = 0.36;

/// Where the level's wall closes Senda Parkway, it stands as a line of precast concrete
/// T-wall panels rather than a garden wall across the lanes: each panel's width (m) and the
/// gap between panels, its stem's thickness (the drawn wall's) and its footing's height and
/// depth (m, reaching outward under the cover's collider), in two weathered tones.
const ROAD_PANEL: (f64, f64) = (1.2, 0.03);
const ROAD_PANEL_FOOTING: (f64, f64) = (0.3, 0.7);
const ROAD_PANEL_TONES: [u32; 2] = [0xaba89f, 0x9a978e];

/// Senda's edge, a run of the tract's own wall: capped split-face block, or along the
/// open ground a black view fence on a low block base with a concrete mow strip inside
/// it, both as the lots' walls are built (with lighter coursing), 0.2 m thick on the
/// inside face of the run's cover (which keeps its depth outside it for the collider
/// alone), from end to end of the run, with a capped pilaster every few metres and at
/// both ends; where the run crosses Senda Parkway's paving, a line of road-closure
/// T-walls instead.
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
    // A part `w` along the run by `d` across it, centred `u` along and `v` across.
    let place_part = |group: &mut Node, part: Node, (u, v): (f64, f64), y: f64| {
        let (x, z) = if long_x { (u, v) } else { (v, u) };
        let mut part = part;
        if !long_x {
            part.rotation = glam::DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
        }
        put(group, part, x, y, z);
    };
    if is_senda_paved(c.x, c.z) {
        let (panel, gap) = ROAD_PANEL;
        let (footing, deep) = ROAD_PANEL_FOOTING;
        let count = ((to - from) / panel).round().max(1.0) as usize;
        let pitch = (to - from) / count as f64;
        // The footing's inner edge flush with the stem's inside face.
        let out = across - face.inside * (deep - WALL_THICKNESS) / 2.0;
        for k in 0..count {
            let u = from + pitch * (k as f64 + 0.5);
            let tone = ROAD_PANEL_TONES[k % ROAD_PANEL_TONES.len()];
            let stem = box_part(pitch - gap, c.h - footing, WALL_THICKNESS, tone, 0.0);
            place_part(group, stem, (u, across), footing + (c.h - footing) / 2.0);
            let foot = box_part(pitch - gap, footing, deep, tone, 0.0);
            place_part(group, foot, (u, out), footing / 2.0);
        }
        return;
    }
    let color = if c.color == SENDA_FENCE_COLOR {
        YARD_VIEW_COLOR
    } else {
        YARD_WALL_COLOR
    };
    if c.color == SENDA_FENCE_COLOR {
        let (width, top, paint) = MOW_STRIP;
        let mut strip = box_part(to - from, top, width, paint, 0.0);
        if let Some(drawable) = &mut strip.drawable {
            drawable.cast_shadow = false;
        }
        let v = across + face.inside * (WALL_THICKNESS + width) / 2.0;
        place_part(group, strip, ((from + to) / 2.0, v), top / 2.0);
    }
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
        boundary_wall(&mut run, (w, d, c.h), color);
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
        CoverKind::Tree => tree_debris(c, parts, 0),
        _ => Node::default(),
    }
}

/// A cover tree's fallen `parts` ([`structure_debris`]), without the boughs it shed at
/// damage stages up to `shed` ([`super::tree_branch_stage`]) before it fell: those fell
/// on their own.
pub fn tree_debris(c: &CoverShape, parts: &PartSet, shed: u32) -> Node {
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
    root.children.push(if grows(tree_proportions(c.x, c.z, c.w, c.d, c.h).family) {
        grown_pieces(&tree, tree_look(c.color), &keep, shed)
    } else {
        tree_pieces(&tree, &keep, shed)
    });
    root
}
