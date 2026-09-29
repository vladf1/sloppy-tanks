//! Port of `cover-model.ts` (with `tower-layout.ts`): the model of every cover kind.
//!
//! `cover_model` returns the unbatched tree like the TypeScript: a group at the
//! cover's x/z whose direct mesh children presentation batches (`batch`) before
//! drawing. Trees are delegated to [`tree_model`]; timber walls and cargo crates
//! are rebuilt when their damage stage (or timber hit count) changes.

use std::f64::consts::PI;

use glam::DVec3;

use super::barrel_surfaces::explosive_barrel;
use super::cottage_details::cottage_details;
use super::harbor_models::{CargoShape, CrateShape, cargo_stack, shipping_container};
use super::model_primitives::{DEFAULT_BOX_RADIUS, box_part, cylinder_part, put, rotated};
use super::pending_props::{
    RubbleStone, concrete_wall, sandstone_footing, sandstone_rock, sandstone_rubble, shingle_roof,
    siding_box, siding_gable,
};
use super::prop_support::Random;
use super::quarry_barriers::{dragon_tooth, steel_hedgehog};
use super::quarry_shapes::quarry_rock_variant;
use super::timber_model::{
    TimberHit, TimberJoin, TimberPart, TimberWall, add_timber_parts, timber_damage_stage,
    timber_parts,
};
use super::tree_models::{TreeDetail, TreeShape, tree_model};
use crate::geometry::math::{js_round, scale_hex_color};
use crate::scene::Node;

/// Cover kinds (TS `CoverKind` in `src/game/types.ts`). Defined here until the
/// simulation's equivalent is shared; de-duplicate at integration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CoverKind {
    Rock,
    Teeth,
    Hedgehog,
    Container,
    Cargo,
    House,
    Tree,
    Timber,
    Concrete,
    Drum,
    Tower,
    Rubble,
    Boundary,
}

impl CoverKind {
    pub const ALL: [CoverKind; 13] = [
        CoverKind::Rock,
        CoverKind::Teeth,
        CoverKind::Hedgehog,
        CoverKind::Container,
        CoverKind::Cargo,
        CoverKind::House,
        CoverKind::Tree,
        CoverKind::Timber,
        CoverKind::Concrete,
        CoverKind::Drum,
        CoverKind::Tower,
        CoverKind::Rubble,
        CoverKind::Boundary,
    ];

    /// The TypeScript identifier.
    pub fn name(self) -> &'static str {
        match self {
            CoverKind::Rock => "rock",
            CoverKind::Teeth => "teeth",
            CoverKind::Hedgehog => "hedgehog",
            CoverKind::Container => "container",
            CoverKind::Cargo => "cargo",
            CoverKind::House => "house",
            CoverKind::Tree => "tree",
            CoverKind::Timber => "timber",
            CoverKind::Concrete => "concrete",
            CoverKind::Drum => "drum",
            CoverKind::Tower => "tower",
            CoverKind::Rubble => "rubble",
            CoverKind::Boundary => "boundary",
        }
    }
}

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

/// The tower footprint shared by the intact model, surviving foundations and
/// collision (`TOWER_BASE` in `tower-layout.ts`). Shared with the simulation;
/// de-duplicate at integration.
pub mod tower_base {
    pub const OFFSET: f64 = 2.55;
    pub const WIDTH: f64 = 1.3;
    pub const DEPTH: f64 = 3.0;
    pub const HEIGHT: f64 = 0.85;
    pub const RUBBLE_HEIGHT: f64 = 1.25;
    pub const POST_Z: f64 = 1.05;
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
    pub tree: Option<(usize, u32)>,
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

const TOWER_POST: u32 = 0x887454;
const TOWER_BRACE: u32 = 0x96734c;
const TOWER_DECK: u32 = 0x887d59;
const TOWER_ROOF: u32 = 0x197451;

fn tower_foundation(group: &mut Node, x: f64) {
    put(
        group,
        concrete_wall(tower_base::WIDTH, tower_base::HEIGHT, tower_base::DEPTH),
        x,
        tower_base::HEIGHT / 2.0,
        0.0,
    );
}

/// Turn the long axis of the boards upright for continuous vertical wood grain.
fn tower_post(height: f64) -> Node {
    rotated(
        siding_box(height, 0.35, 0.35, TOWER_POST),
        0.0,
        0.0,
        PI / 2.0,
    )
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
        CoverKind::House => house(group, c),
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
                    hits: c.timber_hits.clone(),
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
        CoverKind::Tower => tower(group, c),
        CoverKind::Rubble => rubble(group, c),
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

/// A clapboard cottage with framed windows, shutters, door, gable roof, chimney and
/// window boxes.
fn house(group: &mut Node, c: &CoverShape) {
    let (w, d, h) = (c.w, c.d, c.h);
    let wall = h * 0.68;
    let mut add = |node: Node, x: f64, y: f64, z: f64| put(group, node, x, y, z);
    add(
        box_part(w + 0.2, 0.22, d + 0.2, 0xa1977c, 0.0),
        0.0,
        0.11,
        0.0,
    );
    add(siding_box(w, wall, d, c.color), 0.0, wall / 2.0, 0.0);
    // Pale corner boards and a stone sill frame the clapboard walls.
    for x in [-1.0, 1.0] {
        for z in [-1.0, 1.0] {
            add(
                box_part(0.14, wall, 0.14, 0xd4be95, 0.0),
                (x * w) / 2.0,
                wall / 2.0,
                (z * d) / 2.0,
            );
        }
    }
    for side in [-1.0, 1.0] {
        add(
            box_part(w + 0.16, 0.16, 0.12, 0x856447, 0.0),
            0.0,
            0.28,
            (side * d) / 2.0,
        );
        add(
            box_part(0.12, 0.16, d + 0.16, 0x856447, 0.0),
            (side * w) / 2.0,
            0.28,
            0.0,
        );
    }
    for side in [-1.0, 1.0] {
        for x in [-w * 0.29, w * 0.29] {
            let face = side * (d / 2.0 + 0.025);
            add(
                box_part(1.24, 1.16, 0.1, 0xe5cea1, 0.0),
                x,
                wall * 0.59,
                face,
            );
            add(
                box_part(1.36, 0.1, 0.25, 0xc8b087, 0.0),
                x,
                wall * 0.59 - 0.6,
                side * (d / 2.0 + 0.09),
            );
            for shutter in [-1.0, 1.0] {
                add(
                    box_part(0.22, 1.05, 0.1, 0x4d6650, 0.0),
                    x + shutter * 0.75,
                    wall * 0.59,
                    side * (d / 2.0 + 0.06),
                );
                for y in [-0.3, 0.0, 0.3] {
                    add(
                        box_part(0.24, 0.035, 0.11, 0x334a3c, 0.0),
                        x + shutter * 0.75,
                        wall * 0.59 + y,
                        side * (d / 2.0 + 0.08),
                    );
                }
            }
            add(
                box_part(1.05, 0.97, 0.07, 0xffd94e, 0.0),
                x,
                wall * 0.59,
                side * (d / 2.0 + 0.045),
            );
            add(
                box_part(0.075, 0.97, 0.085, 0x875534, 0.0),
                x,
                wall * 0.59,
                side * (d / 2.0 + 0.09),
            );
            add(
                box_part(1.05, 0.075, 0.085, 0x875534, 0.0),
                x,
                wall * 0.59,
                side * (d / 2.0 + 0.09),
            );
        }
        add(
            box_part(0.07, 1.05, 1.1, 0xffd94e, 0.0),
            side * (w / 2.0 + 0.05),
            wall * 0.58,
            0.0,
        );
    }
    for side in [-1.0, 1.0] {
        add(
            box_part(0.08, 1.22, 1.28, 0xe5cea1, 0.0),
            side * (w / 2.0 + 0.01),
            wall * 0.58,
            0.0,
        );
        add(
            box_part(0.1, 1.05, 0.07, 0x875534, 0.0),
            side * (w / 2.0 + 0.09),
            wall * 0.58,
            0.0,
        );
        add(
            box_part(0.1, 0.07, 1.1, 0x875534, 0.0),
            side * (w / 2.0 + 0.09),
            wall * 0.58,
            0.0,
        );
        add(
            box_part(0.25, 0.1, 1.36, 0xc8b087, 0.0),
            side * (w / 2.0 + 0.07),
            wall * 0.58 - 0.65,
            0.0,
        );
    }
    add(
        box_part(1.03, 1.72, 0.11, 0xe5cea1, 0.0),
        0.0,
        0.88,
        d / 2.0 + 0.015,
    );
    add(
        box_part(1.2, 0.18, 0.62, 0x9a9585, 0.0),
        0.0,
        0.14,
        d / 2.0 + 0.2,
    );
    add(
        box_part(0.82, 1.55, 0.1, 0x64452f, 0.0),
        0.0,
        0.85,
        d / 2.0 + 0.06,
    );
    add(
        box_part(0.1, 0.1, 0.12, 0xffd24a, 0.0),
        0.24,
        0.83,
        d / 2.0 + 0.12,
    );
    for y in [0.5, 1.15] {
        add(
            box_part(0.6, 0.42, 0.035, 0x805b3d, 0.0),
            0.0,
            y,
            d / 2.0 + 0.12,
        );
    }
    // Gentle paint weathering varies per cottage without splitting material batches.
    let roof_base = if c.z.abs() > 35.0 { 0xcc493c } else { 0x167857 };
    let roof_color = scale_hex_color(
        roof_base,
        0.9 + 0.12 * (0.5 + 0.5 * (c.x * 3.7 + c.z * 1.9).sin()),
    );
    add(
        siding_gable(w + 0.6, h - wall, d + 0.6, roof_color),
        0.0,
        wall,
        0.0,
    );
    add(
        shingle_roof(w + 0.6, h - wall, d + 0.6, roof_color),
        0.0,
        wall,
        0.0,
    );
    for side in [-1.0, 1.0] {
        add(
            box_part(0.16, 0.15, d + 0.7, 0xe0c79d, 0.0),
            (side * (w + 0.6)) / 2.0,
            wall,
            0.0,
        );
    }
    let ridge_end = (d + 0.6) / 2.0;
    let mut z = -(d + 0.6) / 2.0;
    while z < ridge_end {
        add(
            box_part(0.22, 0.11, 0.46f64.min(ridge_end - z), 0x334a40, 0.0),
            0.0,
            h + 0.04,
            z + 0.23,
        );
        z += 0.48;
    }
    add(
        box_part(0.74, 0.14, 0.74, 0x705a4d, 0.0),
        -w * 0.25,
        h + 0.19,
        -d * 0.2,
    );
    add(
        box_part(0.43, 0.015, 0.43, 0x302c29, 0.0),
        -w * 0.25,
        h + 0.27,
        -d * 0.2,
    );
    let mut y = h - 0.75;
    while y < h + 0.12 {
        add(
            box_part(0.59, 0.026, 0.59, 0xd3b095, 0.0),
            -w * 0.25,
            y,
            -d * 0.2,
        );
        y += 0.22;
    }
    add(
        box_part(0.58, 1.0, 0.58, 0xbc5c3e, 0.0),
        -w * 0.25,
        h - 0.36,
        -d * 0.2,
    );
    cottage_details(group, w, d, h, c.x, c.z);
}

/// A braced timber lookout on two concrete foundations with a ladder.
fn tower(group: &mut Node, c: &CoverShape) {
    for side in [-1.0, 1.0] {
        let x = side * tower_base::OFFSET;
        tower_foundation(group, x);
        for z in [-tower_base::POST_Z, tower_base::POST_Z] {
            put(group, tower_post(4.3), x, tower_base::HEIGHT + 2.15, z);
        }
        // Cross bracing terminates at the same posts that survive the collapse.
        for direction in [-1.0, 1.0] {
            let brace = rotated(
                siding_box(0.18, 4.35, 0.18, TOWER_BRACE),
                direction * (2.0 * tower_base::POST_Z).atan2(3.8),
                0.0,
                0.0,
            );
            put(group, brace, x, 2.85, 0.0);
        }
    }
    put(group, siding_box(6.0, 0.35, 5.0, TOWER_DECK), 0.0, 5.0, 0.0);
    put(group, siding_box(5.7, 2.15, 4.7, c.color), 0.0, 6.15, 0.0);
    for z in [-2.4, 2.4] {
        put(
            group,
            box_part(4.0, 0.65, 0.08, 0x164e79, DEFAULT_BOX_RADIUS),
            0.0,
            6.4,
            z,
        );
    }
    put(
        group,
        siding_gable(6.5, 1.2, 5.5, TOWER_ROOF),
        0.0,
        7.25,
        0.0,
    );
    put(
        group,
        shingle_roof(6.5, 1.2, 5.5, TOWER_ROOF),
        0.0,
        7.25,
        0.0,
    );
    for x in [2.2, 3.1] {
        put(group, tower_post(4.9), x, 2.45, 2.15);
    }
    for i in 0..9 {
        put(
            group,
            siding_box(0.9, 0.08, 0.18, 0xe2cc93),
            2.65,
            0.4 + f64::from(i) * 0.55,
            2.15,
        );
    }
}

/// One surviving tower foundation with cut posts and a scatter of boards, stable per
/// collapse through `debris_seed`.
fn rubble(group: &mut Node, c: &CoverShape) {
    tower_foundation(group, 0.0);
    let mut rng = Random::new(
        c.debris_seed
            .unwrap_or_else(|| js_round(c.x * 73_856_093.0 + c.z * 19_349_663.0)),
    );
    fn choose<T: Copy>(rng: &mut Random, values: &[T]) -> T {
        values[(rng.next() * values.len() as f64).floor() as usize]
    }
    for z in [-tower_base::POST_Z, tower_base::POST_Z] {
        // Cut posts keep their original position, section and grain direction.
        let height = choose(&mut rng, &[0.12, 0.2, 0.28, 0.34]);
        put(
            group,
            tower_post(height),
            0.0,
            tower_base::HEIGHT + height / 2.0,
            z,
        );
        if rng.next() < 0.7 {
            let rz = rng.range(-0.4, 0.4);
            let splinter = rotated(siding_box(0.09, 0.12, 0.16, 0xc5a073), 0.0, 0.0, rz);
            let x = rng.range(-0.1, 0.1);
            put(group, splinter, x, tower_base::HEIGHT + height - 0.01, z);
        }
    }
    // Discrete sizes reuse cached geometry; each foundation gets its own scatter.
    let count = choose(&mut rng, &[2, 3, 4]);
    for i in 0..count {
        let width = choose(&mut rng, &[0.16, 0.3, 0.55]);
        let length = choose(&mut rng, &[0.7, 1.1, 1.5]);
        let yaw = rng.range(-0.55, 0.55);
        let color = choose(&mut rng, &[c.color, TOWER_DECK, TOWER_BRACE]);
        let board = rotated(siding_box(width, 0.09, length, color), 0.0, yaw, 0.0);
        // Keep the pile inside its foundation, preserving the opened center route.
        let room_x = ((tower_base::WIDTH - width * yaw.cos() - length * yaw.sin().abs()) / 2.0
            - 0.02)
            .max(0.0);
        let room_z =
            (tower_base::DEPTH - length * yaw.cos() - width * yaw.sin().abs()) / 2.0 - 0.02;
        let x = rng.range(-room_x, room_x);
        let z = rng.range(-room_z, room_z);
        put(
            group,
            board,
            x,
            tower_base::HEIGHT + 0.045 + f64::from(i) * 0.055,
            z,
        );
    }
}
