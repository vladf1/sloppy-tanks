//! Port of `cover-model.ts`: the model of every cover kind.
//!
//! `cover_model` returns the unbatched tree like the TypeScript: a group at the
//! cover's x/z whose direct mesh children presentation batches (`batch`) before
//! drawing. Trees are delegated to [`tree_model`]; timber walls and cargo crates
//! are rebuilt when their damage stage (or timber hit count) changes.

use std::f64::consts::PI;

use glam::DVec3;

use super::barrel_surfaces::explosive_barrel;
use super::concrete_surfaces::concrete_wall;
use super::harbor_models::{CargoShape, CrateShape, cargo_stack, shipping_container};
use super::house_model::house;
use super::model_primitives::{box_part, cylinder_part, put};
use super::quarry_barriers::{dragon_tooth, steel_hedgehog};
use super::quarry_surfaces::{RubbleStone, sandstone_footing, sandstone_rock, sandstone_rubble};
use super::timber_model::add_timber_parts;
use super::tower_model::{rubble, tower};
use super::tree_models::{TreeDetail, TreeShape, tree_model};
use crate::scene::Node;
use crate::sim::math::Random;
use crate::sim::quarry_rock_shape::quarry_rock_variant;
use crate::sim::render_state::RenderCover;
use crate::sim::timber_layout::{
    TimberHit, TimberJoin, TimberPart, TimberWall, timber_damage_stage, timber_parts,
};
use crate::sim::types::CoverKind;

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

/// A built cover and the values the TypeScript kept in `userData`.
#[derive(Clone, Debug)]
pub struct CoverModel {
    pub node: Node,
    /// The damage stage the model shows (TS `userData.damageStage`, set for timber
    /// and cargo; 0 otherwise). Rebuild when [`cover_damage_stage`] changes.
    pub damage_stage: u32,
    /// Timber hit marks drawn (TS `userData.timberHitCount`); rebuild when it changes.
    pub timber_hit_count: usize,
    /// The timber members drawn (TS `userData.timberParts`), unlengthened.
    pub timber_parts: Vec<TimberPart>,
    /// The tree family and seed for tree covers (TS `userData.family`/`seed`).
    pub tree: Option<(u32, u32)>,
}

/// `coverDamageStage(c)`: cargo splits at any damage and breaks at 35% health;
/// timber uses [`timber_damage_stage`]; other kinds have one look.
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
        _ => 0,
    }
}

/// `coverModel(c, detail, damageStage)`.
pub fn cover_model(c: &CoverShape, detail: TreeDetail, damage_stage: u32) -> CoverModel {
    let mut model = CoverModel {
        node: Node::default(),
        damage_stage: 0,
        timber_hit_count: 0,
        timber_parts: Vec::new(),
        tree: None,
    };
    if c.kind == CoverKind::Tree {
        let tree = tree_model(
            &TreeShape {
                x: c.x,
                z: c.z,
                w: c.w,
                d: c.d,
                h: c.h,
            },
            detail,
        );
        model.node = tree.node;
        model.tree = Some((tree.family, tree.seed));
        return model;
    }
    let group = &mut model.node;
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
        CoverKind::Cargo => {
            model.damage_stage = damage_stage;
            cargo_stack(group, crate_shape(c), damage_stage);
        }
        CoverKind::House => house(group, c.x, c.z, c.w, c.d, c.h, c.color),
        CoverKind::Timber => {
            model.damage_stage = damage_stage;
            model.timber_hit_count = c.timber_hits.len();
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
            model.timber_parts = parts;
        }
        CoverKind::Drum => {
            put(group, explosive_barrel(), 0.0, 0.8, 0.0);
            for y in [0.22, 1.35] {
                put(group, cylinder_part(0.63, 0.1, 0x574e3e, 12), 0.0, y, 0.0);
            }
            put(
                group,
                cylinder_part(0.15, 0.05, 0x343c31, 12),
                0.25,
                1.63,
                0.0,
            );
        }
        CoverKind::Tower => tower(group, c.color),
        CoverKind::Rubble => rubble(group, c.x, c.z, c.color, c.debris_seed),
        CoverKind::Tree => unreachable!("trees return early"),
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
    model
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
    put(group, sandstone_rock(c.w, c.h, c.d, variant), 0.0, 0.0, 0.0);
    put(group, sandstone_footing(c.w, c.d, variant), 0.0, 0.0, 0.0);
    let mut rubble = Random::new(f64::from(variant) + 902.0);
    let chips: Vec<RubbleStone> = (0..12)
        .map(|_| {
            let angle = rubble.range(0.0, PI * 2.0);
            let reach = rubble.range(0.56, 0.72);
            let size = rubble.range(0.14, 0.5);
            // Rest on the drift bank rather than under it.
            let x = angle.cos() * c.w * reach;
            let z = angle.sin() * c.d * reach;
            let h = rubble.range(0.06, 0.12);
            let d = size * rubble.range(0.6, 1.0);
            let shade = rubble.range(0.6, 0.88);
            RubbleStone {
                x,
                y: 0.07,
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
