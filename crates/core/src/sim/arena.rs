//! Pine Village's authored cover, and the shared pickup and spawn layout every map uses.

use super::data::ARENA;
use super::math::{Vec2, js_round};
use super::timber_layout::{TIMBER_HEALTH, TimberJoin};
use super::types::{CoverKind, PickupKind, Team};

/// An authored cover placement; w/d/h are full dimensions in metres.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverDef {
    pub kind: CoverKind,
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub h: f64,
    /// Infinite for indestructible cover.
    pub hp: f64,
    pub color: u32,
    pub timber_join: Option<TimberJoin>,
    /// How many independently breakable bays a timber run splits into.
    pub timber_bays: Option<usize>,
    pub debris_seed: Option<f64>,
}

impl CoverDef {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        kind: CoverKind,
        x: f64,
        z: f64,
        w: f64,
        d: f64,
        h: f64,
        hp: f64,
        color: u32,
    ) -> Self {
        Self {
            kind,
            x,
            z,
            w,
            d,
            h,
            hp,
            color,
            timber_join: None,
            timber_bays: None,
            debris_seed: None,
        }
    }
}

fn authored_layout() -> Vec<CoverDef> {
    let mut result = Vec::new();
    let infinite = f64::INFINITY;
    let cover = CoverDef::new;
    for s in [-1.0, 1.0] {
        result.push(cover(
            CoverKind::Boundary,
            s * (ARENA + 0.5),
            0.0,
            1.0,
            ARENA * 2.0 + 2.0,
            2.2,
            infinite,
            0xa68c68,
        ));
        result.push(cover(
            CoverKind::Boundary,
            0.0,
            s * (ARENA + 0.5),
            ARENA * 2.0 + 2.0,
            1.0,
            2.2,
            infinite,
            0xa68c68,
        ));
        for z in [-45.0, -9.0, 9.0, 45.0] {
            result.push(cover(
                CoverKind::Tree,
                s * 36.0,
                z,
                2.6,
                2.6,
                5.8,
                80.0,
                0x169f65,
            ));
        }
        for z in [-39.0, -13.0, 13.0, 39.0] {
            result.push(cover(
                CoverKind::House,
                s * 45.0,
                z,
                5.0,
                6.0,
                4.6,
                infinite,
                0xb87b4c,
            ));
        }
        for z in [-46.0, 46.0] {
            result.push(cover(
                CoverKind::House,
                s * 17.0,
                z,
                7.0,
                5.0,
                5.2,
                infinite,
                0xc78b50,
            ));
        }
        for z in [-28.0, 28.0] {
            result.push(cover(
                CoverKind::Tree,
                s * 23.0,
                z,
                2.6,
                2.6,
                6.0,
                80.0,
                0x169f65,
            ));
            // Open cottage gardens provide flanking space; each timber bay breaks independently.
            let depth = 0.9;
            let back_x = 33.2;
            let front_x = 24.5;
            // A single square upright joins each corner. Runs stop at its faces.
            let side_end = back_x - depth / 2.0 - 0.04;
            for end in [-1.0, 1.0] {
                let mut run = cover(
                    CoverKind::Timber,
                    (s * (front_x + side_end)) / 2.0,
                    z + end * 5.0,
                    side_end - front_x,
                    depth,
                    2.8,
                    TIMBER_HEALTH,
                    0xb47a49,
                );
                run.timber_join = Some(TimberJoin {
                    open_min: s < 0.0,
                    open_max: s > 0.0,
                    post: false,
                });
                run.timber_bays = Some(2);
                result.push(run);
                let mut post = cover(
                    CoverKind::Timber,
                    s * back_x,
                    z + end * 5.0,
                    depth,
                    depth,
                    2.8,
                    TIMBER_HEALTH,
                    0x805336,
                );
                post.timber_join = Some(TimberJoin {
                    post: true,
                    ..TimberJoin::default()
                });
                result.push(post);
            }
            let mut back = cover(
                CoverKind::Timber,
                s * back_x,
                z,
                depth,
                10.0 - depth - 0.08,
                2.8,
                TIMBER_HEALTH,
                0xb47a49,
            );
            back.timber_join = Some(TimberJoin {
                open_min: true,
                open_max: true,
                post: false,
            });
            back.timber_bays = Some(3);
            result.push(back);
            result.push(cover(
                CoverKind::Drum,
                s * 25.0,
                z - s,
                1.2,
                1.2,
                1.7,
                30.0,
                0xff5b24,
            ));
        }
        for x in [-6.0, -2.0, 2.0, 6.0] {
            result.push(cover(
                CoverKind::Timber,
                x,
                s * 13.0,
                3.7,
                0.9,
                2.8,
                TIMBER_HEALTH,
                0xb47a49,
            ));
        }
        result.push(cover(
            CoverKind::Tower,
            s * 12.75,
            -s * 28.0,
            6.0,
            5.0,
            7.5,
            180.0,
            0xbd864a,
        ));
        for z in [-2.0, 2.0] {
            result.push(cover(
                CoverKind::Drum,
                s * 6.0,
                z,
                1.2,
                1.2,
                1.7,
                30.0,
                0xff5b24,
            ));
        }
        result.push(cover(
            CoverKind::House,
            s * 24.0,
            0.0,
            5.0,
            7.0,
            4.9,
            180.0,
            0xb87b4c,
        ));
    }
    result
}

/// Each timber bay can break independently.
fn segment_walls(layout: Vec<CoverDef>) -> Vec<CoverDef> {
    let mut result = Vec::with_capacity(layout.len());
    for cover in layout {
        if cover.kind != CoverKind::Timber || cover.timber_join.is_some_and(|join| join.post) {
            result.push(cover);
            continue;
        }
        let along = cover.w > cover.d;
        let length = cover.w.max(cover.d);
        let count = cover
            .timber_bays
            .unwrap_or_else(|| js_round(length / 3.7).max(1.0) as usize);
        let span = length / count as f64;
        for i in 0..count {
            let offset = (i as f64 - (count as f64 - 1.0) / 2.0) * span;
            result.push(CoverDef {
                x: cover.x + if along { offset } else { 0.0 },
                z: cover.z + if along { 0.0 } else { offset },
                w: if along { span } else { cover.w },
                d: if along { cover.d } else { span },
                timber_join: cover.timber_join.map(|join| TimberJoin {
                    open_min: i == 0 && join.open_min,
                    open_max: i == count - 1 && join.open_max,
                    post: false,
                }),
                ..cover.clone()
            });
        }
    }
    result
}

pub fn arena_layout() -> Vec<CoverDef> {
    segment_walls(authored_layout())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickupPlacement {
    pub kind: PickupKind,
    pub x: f64,
    pub z: f64,
}

const fn placement(kind: PickupKind, x: f64, z: f64) -> PickupPlacement {
    PickupPlacement { kind, x, z }
}

pub const PICKUP_LAYOUT: [PickupPlacement; 19] = [
    // One rare, contested pickup at the rotationally symmetric center.
    placement(PickupKind::Laser, 0.0, 0.0),
    placement(PickupKind::Rapid, 0.0, -36.0),
    placement(PickupKind::Rapid, 0.0, 36.0),
    // Four distinct route pairs, mirrored by 180 degrees for equal team access.
    placement(PickupKind::Spread, -38.0, -22.0),
    placement(PickupKind::Spread, 38.0, 22.0),
    placement(PickupKind::Rocket, -16.0, -18.0),
    placement(PickupKind::Rocket, 16.0, 18.0),
    placement(PickupKind::Ricochet, -18.0, 36.0),
    placement(PickupKind::Ricochet, 18.0, -36.0),
    placement(PickupKind::Piercing, -38.0, 22.0),
    placement(PickupKind::Piercing, 38.0, -22.0),
    placement(PickupKind::Repair, -38.0, 0.0),
    placement(PickupKind::Repair, 38.0, 0.0),
    placement(PickupKind::Repair, 0.0, -52.0),
    placement(PickupKind::Repair, 0.0, 52.0),
    placement(PickupKind::Shield, -29.0, 46.0),
    placement(PickupKind::Shield, 29.0, -46.0),
    placement(PickupKind::Speed, -29.0, -46.0),
    placement(PickupKind::Speed, 29.0, 46.0),
];

/// The five team spawn lanes, scaled about the arena centre for compact maps.
pub fn spawn_positions(team: Team, scale: f64) -> [Vec2; 5] {
    [-46.0, -23.0, 0.0, 23.0, 46.0].map(|z| Vec2 {
        x: (if team == Team::Blue { -53.0 } else { 53.0 }) * scale,
        z: (if team == Team::Blue { z } else { -z }) * scale,
    })
}
