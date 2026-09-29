//! The Stress Grid: an intentional workload of 30 tanks and 75 destructibles for finding
//! body, navigation, destruction and resource-growth regressions. Do not weaken it.

use super::arena::CoverDef;
use super::data::ARENA;
use super::map_options::MapId;
use super::maps::{ArenaMap, GroundKind};
use super::simulation::SimulationSetup;
use super::timber_layout::TIMBER_HEALTH;
use super::types::CoverKind;

pub const STRESS_TANK_COUNT: usize = 30;
pub const STRESS_PLAYER_HEALTH_MULTIPLIER: f64 = 10_000.0;
pub const STRESS_POWER_UP_MULTIPLIER: f64 = 10.0;
pub const STRESS_AMMO_CRATE_MULTIPLIER: f64 = 10.0;

fn stress_test_layout() -> Vec<CoverDef> {
    let mut covers = Vec::new();
    let infinite = f64::INFINITY;
    let mut add =
        |kind, x, z, w, d, h, hp, color| covers.push(CoverDef::new(kind, x, z, w, d, h, hp, color));

    // A hard square perimeter keeps every body and chain reaction inside the test yard.
    for side in [-1.0, 1.0] {
        add(
            CoverKind::Boundary,
            side * (ARENA + 0.5),
            0.0,
            1.0,
            ARENA * 2.0 + 2.0,
            2.2,
            infinite,
            0x7b7162,
        );
        add(
            CoverKind::Boundary,
            0.0,
            side * (ARENA + 0.5),
            ARENA * 2.0 + 2.0,
            1.0,
            2.2,
            infinite,
            0x7b7162,
        );
    }

    // Dense symmetric quadrants exercise draw calls, pathfinding, collisions and every major
    // destruction path while leaving the centre cross and team spawn strips driveable.
    let coordinates: [f64; 8] = [-42.0, -34.0, -26.0, -18.0, 18.0, 26.0, 34.0, 42.0];
    for (xi, &cx) in coordinates.iter().enumerate() {
        for (zi, &cz) in coordinates.iter().enumerate() {
            let (mut x, mut z) = (cx, cz);
            // Leave hull clearance around the shared rocket and ricochet pickup routes.
            if x.abs() == 18.0 && z.abs() == 18.0 {
                x = x.signum() * 20.0;
            } else if x.abs() == 18.0 && z.abs() == 34.0 {
                z = z.signum() * 32.0;
            }
            let pattern = (xi * 3 + zi * 5) % 7;
            if x.abs() == 26.0 && z.abs() == 26.0 {
                add(CoverKind::Tower, x, z, 6.0, 5.0, 7.5, 180.0, 0xbd864a);
            } else {
                match pattern {
                    0 => add(CoverKind::Cargo, x, z, 2.8, 2.8, 2.4, 80.0, 0xb47a49),
                    1 => {
                        let (w, d) = if xi % 2 != 0 { (3.7, 0.9) } else { (0.9, 3.7) };
                        add(CoverKind::Timber, x, z, w, d, 2.8, TIMBER_HEALTH, 0xa66f46);
                    }
                    2 => add(CoverKind::Tree, x, z, 2.6, 2.6, 5.8, 80.0, 0x169f65),
                    3 => add(CoverKind::Concrete, x, z, 3.2, 1.1, 2.2, infinite, 0xb9b3a5),
                    4 => add(CoverKind::Drum, x, z, 1.2, 1.2, 1.7, 30.0, 0xff5b24),
                    5 => add(CoverKind::Teeth, x, z, 2.4, 2.4, 2.5, infinite, 0xc8c2b5),
                    _ => add(CoverKind::Hedgehog, x, z, 2.9, 3.2, 2.7, infinite, 0x5d6870),
                }
            }
        }
    }

    // Breakable barricades create four temporary gates around the open centre.
    for side in [-1.0, 1.0] {
        for offset in [-12.0, -6.0, 0.0, 6.0, 12.0] {
            add(
                CoverKind::Timber,
                offset,
                side * 22.0,
                4.2,
                0.9,
                2.8,
                TIMBER_HEALTH,
                0xb47a49,
            );
            add(
                CoverKind::Timber,
                side * 22.0,
                offset,
                0.9,
                4.2,
                2.8,
                TIMBER_HEALTH,
                0xb47a49,
            );
        }
    }

    // Permanent buildings create hard sight-line breaks without sealing the broad central lanes.
    for side in [-1.0, 1.0] {
        for offset in [-10.0, 10.0] {
            add(
                CoverKind::House,
                offset,
                side * 35.0,
                5.5,
                6.5,
                5.0,
                infinite,
                0xb87b4c,
            );
            add(
                CoverKind::House,
                side * 35.0,
                offset,
                6.5,
                5.5,
                5.0,
                infinite,
                0xc78b50,
            );
        }
    }

    // Tree groves sit between the permanent houses, outer obstacle grid and spawn approaches.
    for side in [-1.0, 1.0] {
        for offset in [-10.0, 10.0] {
            add(
                CoverKind::Tree,
                offset,
                side * 45.0,
                2.8,
                2.8,
                6.2,
                80.0,
                0x169f65,
            );
            add(
                CoverKind::Tree,
                offset,
                side * 27.0,
                2.6,
                2.6,
                5.8,
                80.0,
                0x218f55,
            );
            add(
                CoverKind::Tree,
                side * 45.0,
                offset,
                2.8,
                2.8,
                6.2,
                80.0,
                0x169f65,
            );
            add(
                CoverKind::Tree,
                side * 27.0,
                offset,
                2.6,
                2.6,
                5.8,
                80.0,
                0x218f55,
            );
        }
    }

    // A final square belt alternates permanent concrete with heavy movable obstacles. It stays
    // between the combat field and deployment pads so the expanded roster can still spawn cleanly.
    let outer_offsets = [-36.0, -12.0, 12.0, 36.0];
    for side in [-1.0, 1.0] {
        for (i, &offset) in outer_offsets.iter().enumerate() {
            let (kind, w, d, h, color) = match i % 3 {
                0 => (CoverKind::Concrete, 3.2, 1.1, 2.2, 0xb9b3a5),
                1 => (CoverKind::Teeth, 2.4, 2.4, 2.5, 0xc8c2b5),
                _ => (CoverKind::Hedgehog, 2.9, 3.2, 2.7, 0x5d6870),
            };
            add(kind, offset, side * 50.0, w, d, h, infinite, color);
            add(kind, side * 50.0, offset, d, w, h, infinite, color);
        }
    }

    // Extra permanent teeth guard the north and south verges without blocking spawn pads.
    for side in [-1.0, 1.0] {
        for x in [-42.0, -28.0, -14.0, 14.0, 28.0, 42.0] {
            add(
                CoverKind::Teeth,
                x,
                side * 54.0,
                2.4,
                2.4,
                2.5,
                infinite,
                0xc8c2b5,
            );
        }
    }
    covers
}

pub static STRESS_TEST_MAP: ArenaMap = ArenaMap {
    id: MapId::StressTest,
    name: "Stress Grid",
    description: "30 tanks · 75 destructibles · permanent buildings and barriers",
    theme: None,
    // Reuse only existing ground materials, without Pine Village's surrounding scenery.
    floor: Some(GroundKind::DryGrass),
    outer_floor: Some(GroundKind::PackedDirt),
    outer_floor_extent: Some(ARENA * 2.0 + 20.0),
    scale: None,
    layout: stress_test_layout,
};

/// The grid and its 30-tank roster, shared by single player and multiplayer rooms. Players
/// get a near-invulnerable hull, and pickups boost every tank.
pub fn stress_test_level() -> SimulationSetup {
    SimulationSetup {
        custom_map: Some(Some(&STRESS_TEST_MAP)),
        round_count: Some(STRESS_TANK_COUNT),
        human_health_multiplier: Some(STRESS_PLAYER_HEALTH_MULTIPLIER),
        power_up_duration_multiplier: Some(STRESS_POWER_UP_MULTIPLIER),
        ammo_crate_multiplier: Some(STRESS_AMMO_CRATE_MULTIPLIER),
        ..SimulationSetup::default()
    }
}
