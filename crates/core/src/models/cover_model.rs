//! Port of `cover-model.ts`: the model of every cover kind.
//!
//! `cover_model` returns the unbatched tree like the TypeScript: a group at the
//! cover's x/z whose direct mesh children presentation batches (`batch`) before
//! drawing. Trees are delegated to [`tree_model`]; timber walls and cargo crates
//! are rebuilt when their damage stage or hit count changes.

use std::f64::consts::PI;

use glam::DVec3;

use super::barrel_surfaces::{LID_DROP, explosive_barrel};
use super::building_kit::fieldstone_wall;
use super::concrete_surfaces::concrete_wall;
use super::crate_model::{CrateShape, cargo_crate};
use super::harbor_models::{CargoShape, shipping_container};
use super::harbor_surfaces::harbor_parapet;
use super::house_model::{house, house_heap};
use super::model_primitives::{box_part, cylinder_part, put};
use super::quarry_barriers::{bastion_wall, dragon_tooth, steel_hedgehog};
use super::quarry_surfaces::{
    RockSetting, RubbleStone, STONE_SEAT, sandstone_footing, sandstone_footing_height,
    sandstone_rock, sandstone_rubble,
};
use super::timber_model::add_timber_parts;
use super::tower_model::{rubble, tower};
use super::tree_models::{TreeDetail, TreeShape, tree_model};
use crate::scene::Node;
use crate::sim::house_layout::{house_damage_stage, is_house_heap};
use crate::sim::math::Random;
use crate::sim::quarry_rock_shape::quarry_rock_variant;
use crate::sim::render_state::RenderCover;
use crate::sim::timber_layout::{
    TimberHit, TimberJoin, TimberWall, timber_damage_stage, timber_parts,
};
use crate::sim::types::CoverKind;

/// Pine Village's boundary colour (`sim::arena::authored_layout`): its walls are
/// built in fieldstone, other maps' boundaries in concrete.
const VILLAGE_WALL_COLOR: u32 = 0xa68c68;
/// Harbor Havoc's boundary colour (`sim::harbor_layout`): its walls are the sea
/// wall's parapet.
const HARBOR_WALL_COLOR: u32 = 0x879698;
/// Dusty Dig's boundary colour (`sim::quarry_layout`): its walls are bastion cells.
const QUARRY_WALL_COLOR: u32 = 0xd2bd99;

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
    /// Chosen at collapse so rubble stays stable when its model is rebuilt.
    pub debris_seed: Option<f64>,
    pub timber_hits: Vec<TimberHit>,
    pub timber_join: Option<TimberJoin>,
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
            debris_seed: cover.debris_seed,
            timber_hits: cover.timber_hits.clone(),
            timber_join: cover.timber_join,
        }
    }
}

/// `coverDamageStage(c)`: cargo splits at any damage and breaks at 35% health;
/// timber uses [`timber_damage_stage`] and cottages [`house_damage_stage`]; other
/// kinds have one look.
pub fn cover_damage_stage(kind: CoverKind, hp: f64, max_hp: f64) -> u32 {
    match kind {
        CoverKind::Cargo => {
            if hp >= max_hp {
                0
            } else if hp > max_hp * 0.35 {
                1
            } else {
                2
            }
        }
        CoverKind::Timber => timber_damage_stage(hp, max_hp),
        CoverKind::House => house_damage_stage(hp, max_hp),
        _ => 0,
    }
}

/// `coverModel(c, detail, damageStage)`: rebuild it when [`cover_damage_stage`] or
/// the timber, crate or cottage hit count changes.
pub fn cover_model(c: &CoverShape, detail: TreeDetail, damage_stage: u32) -> Node {
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
        CoverKind::Cargo => cargo_crate(group, crate_shape(c), &c.timber_hits, damage_stage),
        // An intact cottage shows no marks, so shells change its look only at a
        // damage stage and it keeps its place in the static covers' draws until then.
        CoverKind::House => house(
            group,
            c.x,
            c.z,
            c.w,
            c.d,
            c.h,
            damage_stage,
            if damage_stage == 0 {
                &[]
            } else {
                &c.timber_hits
            },
        ),
        CoverKind::Timber => {
            let parts = timber_parts(
                &TimberWall {
                    x: c.x,
                    z: c.z,
                    w: c.w,
                    d: c.d,
                    h: c.h,
                    color: c.color,
                    hits: &c.timber_hits,
                    join: c.timber_join,
                },
                damage_stage,
            );
            add_timber_parts(group, &parts);
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
        CoverKind::Tower => tower(group, c.color),
        CoverKind::Rubble if is_house_heap(c.h) => {
            house_heap(group, c.x, c.z, c.w, c.d, c.debris_seed.unwrap_or(0.0));
        }
        CoverKind::Rubble => rubble(group, c.x, c.z, c.color, c.debris_seed),
        CoverKind::Tree => unreachable!("trees return early"),
        CoverKind::Boundary if c.color == VILLAGE_WALL_COLOR => {
            fieldstone_wall(group, c.w, c.h, c.d, c.x, c.z);
        }
        CoverKind::Boundary if c.color == HARBOR_WALL_COLOR => {
            put(group, harbor_parapet(c.w, c.h, c.d), 0.0, c.h / 2.0, 0.0);
        }
        CoverKind::Boundary if c.color == QUARRY_WALL_COLOR => {
            put(group, bastion_wall(c.w, c.h, c.d), 0.0, 0.0, 0.0);
        }
        CoverKind::Concrete | CoverKind::Boundary => {
            let block = if c.kind == CoverKind::Boundary {
                concrete_wall(c.w, c.h, c.d)
            } else {
                box_part(c.w, c.h, c.d, c.color, 0.16)
            };
            put(group, block, 0.0, c.h / 2.0, 0.0);
            if c.kind == CoverKind::Concrete {
                let along = c.w > c.d;
                let marks = ((if along { c.w } else { c.d }) / 1.1).floor() as u32;
                for i in 0..marks {
                    let i = f64::from(i);
                    let mark = box_part(
                        if along { 0.55 } else { 0.035 },
                        0.22,
                        if along { 0.035 } else { 0.55 },
                        0x499ac7,
                        0.01,
                    );
                    put(
                        group,
                        mark,
                        if along {
                            -c.w / 2.0 + 0.6 + i * 1.1
                        } else {
                            c.w / 2.0 + 0.02
                        },
                        c.h * 0.7,
                        if along {
                            c.d / 2.0 + 0.02
                        } else {
                            -c.d / 2.0 + 0.6 + i * 1.1
                        },
                    );
                }
            }
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
