//! Dusty Dig: a broad east/west crossing, sheltered outer loops and two crate-plugged rock cuts.

use super::arena::{CoverDef, boundary_walls};
use super::data::ARENA;
use super::quarry_barrier_shapes::DRAGON_TOOTH_SCALE;
use super::types::CoverKind;

const STONE: u32 = 0xd2bd99;

fn stone(kind: CoverKind, x: f64, z: f64, w: f64, d: f64, h: f64) -> CoverDef {
    CoverDef::new(kind, x, z, w, d, h, f64::INFINITY, STONE)
}

pub fn quarry_layout() -> Vec<CoverDef> {
    let mut covers = Vec::new();
    for side in [-1.0, 1.0] {
        covers.extend(boundary_walls(side, ARENA, 1.2, STONE));
        // Offset islands break cross-map fire without enclosing the central pickup.
        covers.push(stone(
            CoverKind::Rock,
            side * 25.0,
            side * 12.0,
            14.0,
            8.0,
            4.6,
        ));
        covers.push(stone(
            CoverKind::Rock,
            side * 25.0,
            -side * 12.0,
            14.0,
            8.0,
            3.8,
        ));
        covers.push(stone(
            CoverKind::Rock,
            side * 4.0,
            side * 28.0,
            16.0,
            9.0,
            4.8,
        ));
        covers.push(stone(
            CoverKind::Rock,
            side * 40.5,
            side * 37.0,
            10.0,
            12.0,
            4.2,
        ));
        covers.push(stone(
            CoverKind::Rock,
            side * 24.5,
            side * 37.0,
            10.0,
            12.0,
            3.6,
        ));
        covers.push(stone(
            CoverKind::Rock,
            -side * 10.0,
            side * 47.0,
            13.0,
            7.0,
            3.4,
        ));
        // Four pallet-sized supply crates form an orderly storage bay in each cut.
        // Their individual colliders leave visible seams and open progressively under fire.
        for x in [31.2, 33.8] {
            for z in [35.4, 38.6] {
                covers.push(CoverDef::new(
                    CoverKind::Cargo,
                    side * x,
                    side * z,
                    2.4,
                    2.8,
                    2.1,
                    55.0,
                    0xa18e6f,
                ));
            }
        }
        // Small, separated cover islands leave the direct route wide enough to dodge.
        covers.push(CoverDef::new(
            CoverKind::Cargo,
            side * 9.0,
            -side * 7.0,
            3.0,
            3.0,
            2.6,
            90.0,
            0xa18e6f,
        ));
        covers.push(CoverDef::new(
            CoverKind::Drum,
            side * 15.0,
            side * 27.0,
            1.2,
            1.2,
            1.7,
            30.0,
            0xff5b24,
        ));
        // Crane-set, staggered ranks follow the verge with uneven gaps and offsets.
        // Mirror the same irregular belt for fair approaches; keep the haul road open.
        for [x, z, width, height] in [
            [41.7, -15.3, 1.9, 1.9],
            [42.25, -12.45, 2.0, 2.05],
            [41.85, -9.35, 1.9, 1.8],
            [42.5, -6.7, 1.9, 1.9],
            [45.25, -13.85, 2.0, 2.05],
            [44.7, -10.8, 1.9, 1.8],
            [45.4, -7.9, 1.9, 1.9],
            [45.05, -4.75, 2.0, 2.05],
        ] {
            covers.push(stone(
                CoverKind::Teeth,
                side * x,
                side * z,
                width * DRAGON_TOOTH_SCALE,
                width * DRAGON_TOOTH_SCALE,
                height * DRAGON_TOOTH_SCALE,
            ));
        }
        // A close-set steel line ties into each midfield rock shoulder.
        for i in 0..4 {
            covers.push(stone(
                CoverKind::Hedgehog,
                side * (15.8 + i as f64 * 2.4),
                side * 22.0,
                2.32,
                2.56,
                2.16,
            ));
        }
    }
    covers
}
