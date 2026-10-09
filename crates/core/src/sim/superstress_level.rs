//! The Scrap Yard: the Stress Grid's 30 tanks packed into a compact yard with over 100
//! destructibles (watchtowers among them), cover that rebuilds in place and debris that
//! lingers until the budget needs room. Its cottages stand for good, like the village's.
//! Level behaviour lives here and reaches the game only through `Simulation::after_step`,
//! `restore_cover` and the map's scale.

use super::arena::{BOUNDARY_THICKNESS, CoverDef};
use super::data::ARENA;
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::map_options::MapId;
use super::maps::{ArenaMap, GroundKind};
use super::simulation::{Simulation, SimulationSetup};
use super::stress_test_level::{
    STRESS_AMMO_CRATE_MULTIPLIER, STRESS_PLAYER_HEALTH_MULTIPLIER, STRESS_POWER_UP_MULTIPLIER,
    STRESS_TANK_COUNT,
};
use super::timber_layout::TIMBER_HEALTH;
use super::types::{CoverKind, SimEvent, SimEventType};

/// Two-thirds of the standard arena's width: the same 30 tanks fight at 2.4x the density.
pub const SUPERSTRESS_SCALE: f64 = 0.65;
const YARD: f64 = ARENA * SUPERSTRESS_SCALE;
/// Three times the normal debris budget; FRAGMENT_CAPACITY bounds what presentation draws.
pub const SUPERSTRESS_MAX_FRAGMENTS: usize = 240;
/// Destroyed cover rises again this long after it falls, once no tank stands in the way.
pub const REBUILD_SECONDS: f64 = 6.0;
/// Hull half-length plus margin that must be clear around a footprint before it rebuilds.
const REBUILD_CLEARANCE: f64 = 2.0;
/// Settled debris stays while the yard is below this share of its fragment budget. The base
/// cleanup fades the most distant pieces above 80%, so held debris retires gracefully.
const DEBRIS_ROOM_FRACTION: f64 = 0.75;
/// Remaining life held on lingering debris; within the base cleanup's fade candidates.
const DEBRIS_HOLD_SECONDS: f64 = 2.0;

const TIMBER_BAY: f64 = 3.7;
const TIMBER_DEPTH: f64 = 0.9;

#[derive(Clone, Copy)]
enum Piece {
    Cargo,
    Drum,
    Tree,
    House,
    Tower,
}

fn sized(kind: Piece, x: f64, z: f64) -> CoverDef {
    match kind {
        Piece::Cargo => CoverDef::new(CoverKind::Cargo, x, z, 2.8, 2.8, 2.4, 80.0, 0xb47a49),
        Piece::Drum => CoverDef::new(CoverKind::Drum, x, z, 1.2, 1.2, 1.7, 30.0, 0xff5b24),
        Piece::Tree => CoverDef::new(CoverKind::Tree, x, z, 2.6, 2.6, 5.8, 80.0, 0x218f55),
        Piece::House => CoverDef::new(
            CoverKind::House,
            x,
            z,
            5.0,
            6.0,
            4.6,
            f64::INFINITY,
            0xb87b4c,
        ),
        Piece::Tower => CoverDef::new(CoverKind::Tower, x, z, 6.0, 5.0, 7.5, 180.0, 0xbd864a),
    }
}

struct Yard {
    covers: Vec<CoverDef>,
}

/// Placements use standard-arena coordinates, like the shared spawns and pickups, so the
/// whole yard follows SUPERSTRESS_SCALE. Object sizes and cluster spacing stay in metres.
fn at(standard: f64) -> f64 {
    standard * SUPERSTRESS_SCALE
}

impl Yard {
    /// Every placement has a twin rotated half a turn, so both teams meet the same yard.
    fn pair(&mut self, cover: CoverDef) {
        let twin = (cover.x != 0.0 || cover.z != 0.0).then(|| CoverDef {
            x: -cover.x,
            z: -cover.z,
            ..cover.clone()
        });
        self.covers.push(cover);
        if let Some(twin) = twin {
            self.covers.push(twin);
        }
    }

    fn place(&mut self, kind: Piece, x: f64, z: f64) {
        self.pair(sized(kind, x, z));
    }

    fn put(&mut self, kind: Piece, x: f64, z: f64) {
        self.place(kind, at(x), at(z));
    }

    /// A centred run of independently breakable timber bays.
    fn timber_run(&mut self, x: f64, z: f64, bays: usize, along_x: bool) {
        for i in 0..bays {
            let offset = (i as f64 - (bays as f64 - 1.0) / 2.0) * TIMBER_BAY;
            self.pair(CoverDef::new(
                CoverKind::Timber,
                if along_x { x + offset } else { x },
                if along_x { z } else { z + offset },
                if along_x { TIMBER_BAY } else { TIMBER_DEPTH },
                if along_x { TIMBER_DEPTH } else { TIMBER_BAY },
                2.8,
                TIMBER_HEALTH,
                0xa66f46,
            ));
        }
    }

    fn fence(&mut self, x: f64, z: f64, bays: usize, along_x: bool) {
        self.timber_run(at(x), at(z), bays, along_x);
    }

    fn crate_block(&mut self, x: f64, z: f64) {
        for dx in [-1.0, 1.0] {
            for dz in [-1.0, 1.0] {
                self.place(
                    Piece::Cargo,
                    at(x) + (dx * 2.8) / 2.0,
                    at(z) + (dz * 2.8) / 2.0,
                );
            }
        }
    }

    fn drum_trio(&mut self, x: f64, z: f64) {
        self.place(Piece::Drum, at(x) - 0.65, at(z) - 0.4);
        self.place(Piece::Drum, at(x) + 0.65, at(z) - 0.4);
        self.place(Piece::Drum, at(x), at(z) + 0.7);
    }
}

fn superstress_layout() -> Vec<CoverDef> {
    let mut yard = Yard { covers: Vec::new() };
    // A hard square fence keeps every body and chain reaction inside the yard.
    let wall = |x, z, w, d| {
        CoverDef::new(
            CoverKind::Boundary,
            x,
            z,
            w,
            d,
            2.2,
            f64::INFINITY,
            0x7b7162,
        )
    };
    let (centre, length) = (
        YARD + BOUNDARY_THICKNESS / 2.0,
        YARD * 2.0 + BOUNDARY_THICKNESS * 2.0,
    );
    yard.pair(wall(centre, 0.0, BOUNDARY_THICKNESS, length));
    yard.pair(wall(0.0, centre, length, BOUNDARY_THICKNESS));

    // The laser pickup sits in a powder-keg plaza inside a ring of timber with open corners.
    yard.put(Piece::Drum, 10.0, 0.0);
    yard.put(Piece::Drum, 0.0, 10.0);
    yard.fence(0.0, 17.0, 2, true);
    yard.fence(17.0, 0.0, 2, false);

    // Crate stacks, drum trios and lone trees chain into each other across both diagonals.
    yard.crate_block(25.0, 29.0);
    yard.crate_block(-25.0, 22.0);
    yard.drum_trio(11.0, 28.0);
    yard.drum_trio(-11.0, 26.0);
    yard.drum_trio(-15.0, 9.0);
    yard.drum_trio(-27.0, 35.0);
    yard.put(Piece::Tree, 25.0, 10.0);
    yard.put(Piece::Tree, -25.0, 8.0);
    yard.put(Piece::Drum, 31.0, 13.0);
    yard.put(Piece::Drum, -31.0, 11.0);
    yard.put(Piece::Cargo, 16.0, 9.0);
    yard.put(Piece::Cargo, 20.0, 41.6);

    // Watchtowers overlook the plaza from beyond its fences and rebuild back over their
    // rubble; a permanent cottage stands by each team's spawn.
    yard.put(Piece::Tower, 0.0, 26.5);
    yard.put(Piece::House, -38.0, 33.8);

    // Timber alleys guard the rapid-fire and repair pickups beside each end wall.
    yard.fence(8.0, 47.4, 2, false);
    yard.fence(-8.0, 47.4, 2, false);
    yard.put(Piece::Cargo, 18.0, 52.0);
    yard.put(Piece::Cargo, -18.0, 52.0);
    yard.put(Piece::Cargo, 12.6, 56.8);
    yard.put(Piece::Cargo, -12.6, 56.8);

    // Groves shade the end walls; stumps keep blocking tanks after the crowns fall.
    for x in [-33.0, -24.0, 24.0, 33.0] {
        yard.put(Piece::Tree, x, 56.6);
    }

    // Timber stubs divide each team's spawn lanes into garages with a drum in each corner.
    for z in [11.5, 34.5] {
        for side in [-1.0, 1.0] {
            yard.timber_run(-YARD + TIMBER_BAY / 2.0, at(side * z), 1, true);
            yard.place(Piece::Drum, -YARD + 0.8, at(side * z) + side * 1.3);
        }
    }
    yard.covers
}

pub static SUPERSTRESS_MAP: ArenaMap = ArenaMap {
    id: MapId::Superstress,
    name: "Scrap Yard",
    description: "Compact yard · 30 tanks · cover rebuilds and debris lingers",
    theme: None,
    floor: Some(GroundKind::PackedDirt),
    outer_floor: Some(GroundKind::DryGrass),
    outer_floor_extent: Some(ARENA * 2.0 + 20.0),
    scale: Some(SUPERSTRESS_SCALE),
    layout: superstress_layout,
};

fn footprint_occupied(simulation: &Simulation, cover_index: usize) -> bool {
    let cover = &simulation.covers[cover_index];
    let (x, z, w, d) = match cover.motion {
        Some(motion) => (motion.origin_x, motion.origin_z, motion.w, motion.d),
        None => (cover.x, cover.z, cover.w, cover.d),
    };
    simulation.tanks.iter().any(|tank| {
        if !tank.alive {
            return false;
        }
        let p = simulation.body_translation(tank.body);
        (p.x - x).abs() < w / 2.0 + REBUILD_CLEARANCE
            && (p.z - z).abs() < d / 2.0 + REBUILD_CLEARANCE
    })
}

/// Bring each destroyed cover back after REBUILD_SECONDS, waiting while a tank is in the way.
/// The fall time lives on the cover record, so a reset's fresh covers start with no rebuild
/// pending.
fn rebuild_cover(simulation: &mut Simulation) {
    for i in 0..simulation.covers.len() {
        let cover = &simulation.covers[i];
        if cover.alive || !cover.destructible {
            continue;
        }
        let Some(fallen) = cover.fallen_at else {
            simulation.covers[i].fallen_at = Some(simulation.elapsed);
            continue;
        };
        if simulation.elapsed - fallen < REBUILD_SECONDS || footprint_occupied(simulation, i) {
            continue;
        }
        simulation.covers[i].fallen_at = None;
        simulation.restore_cover(i);
        let cover = &simulation.covers[i];
        let mut rebuilt = SimEvent::at(SimEventType::Impact, cover.x, cover.z);
        rebuilt.id = Some(cover.id);
        rebuilt.cover_kind = Some(cover.kind);
        rebuilt.color = Some(cover.color);
        rebuilt.height = Some(cover.h);
        simulation.events.push(rebuilt);
    }
}

/// Hold debris just before its fade while the budget has room. When destruction fills it,
/// the base cleanup fades the most distant settled pieces first.
fn linger_debris(simulation: &mut Simulation) {
    if simulation.fragments.len() as f64 >= simulation.max_fragments as f64 * DEBRIS_ROOM_FRACTION {
        return;
    }
    let hold = DEBRIS_CLEANUP_SECONDS + DEBRIS_HOLD_SECONDS;
    let elapsed = simulation.elapsed;
    for fragment in &mut simulation.fragments {
        if fragment.life > DEBRIS_CLEANUP_SECONDS && fragment.life < hold {
            fragment.life = hold;
            // A later blast relaunches debris only until this deadline; keep it ahead of the hold.
            if let Some(expires_at) = fragment.expires_at {
                fragment.expires_at = Some(expires_at.max(elapsed + hold));
            }
        }
    }
}

/// Level rules; they draw no gameplay randomness, so seeded matches stay reproducible.
pub fn superstress_rules(simulation: &mut Simulation) {
    rebuild_cover(simulation);
    linger_debris(simulation);
}

/// The yard, its rules and its 30-tank roster, shared by single player and multiplayer
/// rooms. Players get the stress test's near-invulnerable hull; pickups boost every tank.
pub fn superstress_level() -> SimulationSetup {
    SimulationSetup {
        custom_map: Some(Some(&SUPERSTRESS_MAP)),
        round_count: Some(STRESS_TANK_COUNT),
        max_fragments: Some(SUPERSTRESS_MAX_FRAGMENTS),
        human_health_multiplier: Some(STRESS_PLAYER_HEALTH_MULTIPLIER),
        power_up_duration_multiplier: Some(STRESS_POWER_UP_MULTIPLIER),
        ammo_crate_multiplier: Some(STRESS_AMMO_CRATE_MULTIPLIER),
        after_step: Some(Some(superstress_rules)),
        ..SimulationSetup::default()
    }
}
