//! The Scrap Yard extra level: spawns and pickups fit the compact yard, the layout is dense
//! and half-turn symmetric, destroyed cover rebuilds in place with its identity, debris
//! lingers while the budget has room, and a seeded brawl stays bounded (the former
//! `tests/superstress-level.test.ts`).

use std::collections::HashSet;

use sloppy_core::sim::data::{ARENA, STEP};
use sloppy_core::sim::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::superstress_level::{
    REBUILD_SECONDS, SUPERSTRESS_MAP, SUPERSTRESS_SCALE, superstress_level, superstress_rules,
};
use sloppy_core::sim::{
    CoverKind, FragmentShape, SimEventType, Simulation, SimulationSetup, VehicleCommand,
};

const YARD: f64 = ARENA * SUPERSTRESS_SCALE;
/// Hull clearance a yard spawn or pickup keeps from every cover.
const CLEARANCE: f64 = 1.5;

fn superstress(seed: f64) -> Simulation {
    Simulation::new(
        seed,
        single_player_rules(superstress_level()).merged(SimulationSetup {
            round: Some(3),
            ..SimulationSetup::default()
        }),
    )
}

fn human(s: &Simulation) -> usize {
    s.human_index().expect("local play has a human")
}

#[test]
fn superstress_spawns_and_pickups_fit_the_compact_yard_with_hull_clearance_and_routes() {
    let mut sim = superstress(731.0);
    assert_eq!(sim.tanks.len(), 30);
    let mut points: Vec<Vec2> = sim.pickups.iter().map(|p| Vec2::new(p.x, p.z)).collect();
    points.extend(
        sim.tanks
            .iter()
            .map(|tank| sim.body_translation(tank.body).planar()),
    );
    for point in points {
        assert!(
            point.x.abs().max(point.z.abs()) < YARD - 2.0,
            "inside the yard"
        );
        assert_eq!(
            sim.nav.blocked[sim.nav.index(point)],
            0,
            "blocked {point:?}"
        );
        assert!(
            (point.x == 0.0 && point.z == 0.0) || !sim.nav.find(Vec2::ZERO, point).is_empty(),
            "unreachable {point:?}"
        );
        for cover in &sim.covers {
            assert!(
                (point.x - cover.x).abs() >= cover.w / 2.0 + CLEARANCE
                    || (point.z - cover.z).abs() >= cover.d / 2.0 + CLEARANCE,
                "no hull clearance at {point:?} beside {:?}",
                cover.kind
            );
        }
    }
}

/// `${kind}:${x.toFixed(3)}:${z.toFixed(3)}`; `+ 0.0` folds -0 into 0 like JS.
fn key(kind: CoverKind, x: f64, z: f64) -> String {
    format!("{kind:?}:{:.3}:{:.3}", x + 0.0, z + 0.0)
}

#[test]
fn the_yard_is_dense_half_turn_symmetric_and_has_only_cover_that_rebuilds_in_place() {
    let layout = (SUPERSTRESS_MAP.layout)();
    let destructible: Vec<_> = layout.iter().filter(|c| c.hp.is_finite()).collect();
    assert!(destructible.len() >= 100);
    assert_eq!(
        layout
            .iter()
            .filter(|c| c.kind == CoverKind::Boundary)
            .count(),
        4
    );
    // A collapsing tower adds separate rubble covers, which a rebuild could never reclaim.
    assert!(destructible.iter().all(|c| c.kind != CoverKind::Tower));
    let keys: HashSet<String> = layout.iter().map(|c| key(c.kind, c.x, c.z)).collect();
    assert_eq!(
        keys.len(),
        layout.len(),
        "every obstacle needs a unique location and kind"
    );
    for cover in &layout {
        assert!(
            keys.contains(&key(cover.kind, -cover.x, -cover.z)),
            "{:?} has a rotated twin",
            cover.kind
        );
        if cover.kind != CoverKind::Boundary {
            assert!(cover.x.abs() + cover.w / 2.0 <= YARD && cover.z.abs() + cover.d / 2.0 <= YARD);
        }
    }
}

fn destroy(sim: &mut Simulation, kind: CoverKind) -> usize {
    let cover = sim
        .covers
        .iter()
        .position(|c| c.kind == kind && c.alive)
        .unwrap();
    let h = human(sim);
    let (id, team) = (sim.tanks[h].id, sim.tanks[h].team);
    sim.damage_cover(cover, 9999.0, id, team, None, None);
    assert!(!sim.covers[cover].alive);
    cover
}

fn set_tank_translation(sim: &mut Simulation, tank: usize, x: f64, z: f64) {
    let body = sim.tanks[tank].body;
    sim.world.bodies[body].set_translation(vector(x, 0.65, z), true);
}

#[test]
fn destroyed_cover_rises_with_its_identity_once_the_rebuild_delay_passes_and_it_is_clear() {
    let mut sim = superstress(731.0);
    let cover_count = sim.covers.len();
    let bodies = sim.world.bodies.len();
    // Push the drum off its spot first, so the rebuild has to return it to its origin.
    let drum = sim
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Drum)
        .unwrap();
    let origin = Vec2::new(sim.covers[drum].x, sim.covers[drum].z);
    let drum_body = sim.covers[drum].body;
    let drum_height = sim.covers[drum].h;
    sim.world.bodies[drum_body]
        .set_translation(vector(origin.x + 2.0, drum_height / 2.0, origin.z), true);
    let mut fallen = vec![
        destroy(&mut sim, CoverKind::Timber),
        destroy(&mut sim, CoverKind::Tree),
        destroy(&mut sim, CoverKind::Cargo),
    ];
    let h = human(&sim);
    let (id, team) = (sim.tanks[h].id, sim.tanks[h].team);
    sim.damage_cover(drum, 9999.0, id, team, None, None);
    fallen.push(drum);
    let fallen_ids: Vec<u32> = fallen.iter().map(|&c| sim.covers[c].id).collect();
    sim.events.clear();
    let near_tank = sim.tanks.iter().position(|t| !t.human).unwrap();
    set_tank_translation(&mut sim, near_tank, origin.x, origin.z + 1.0);
    let rng = sim.rng.state;
    superstress_rules(&mut sim);
    sim.elapsed += REBUILD_SECONDS - 0.5;
    superstress_rules(&mut sim);
    assert!(
        fallen.iter().all(|&c| !sim.covers[c].alive),
        "still waiting for the rebuild delay"
    );

    sim.elapsed += 0.5;
    superstress_rules(&mut sim);
    assert!(
        !sim.covers[drum].alive,
        "a tank on the footprint delays the rebuild"
    );
    for &c in fallen.iter().filter(|&&c| c != drum) {
        assert!(sim.covers[c].alive, "{:?} rebuilt", sim.covers[c].kind);
    }
    set_tank_translation(&mut sim, near_tank, 0.0, 0.0);
    superstress_rules(&mut sim);

    for (&c, &id) in fallen.iter().zip(&fallen_ids) {
        let cover = &sim.covers[c];
        assert!(cover.alive);
        assert_eq!(cover.hp, cover.max_hp);
        assert!(cover.timber_hits.is_empty());
        assert_eq!(cover.id, id, "{:?} keeps its record", cover.kind);
        assert_eq!(sim.cover_by_collider.get(&cover.collider), Some(&c));
        let at = Vec2::new(cover.x, cover.z);
        assert_eq!(
            sim.nav.blocked[sim.nav.index(at)],
            1,
            "{:?} blocks routes again",
            cover.kind
        );
        assert!(
            sim.events
                .iter()
                .any(|e| e.kind == SimEventType::Impact && e.id == Some(cover.id)),
            "{:?} announces its rebuild",
            cover.kind
        );
    }
    assert_eq!(Vec2::new(sim.covers[drum].x, sim.covers[drum].z), origin);
    let p = sim.body_translation(sim.covers[drum].body);
    assert!(
        (p.x - origin.x).hypot(p.z - origin.z) < 1e-6,
        "the drum returns to its origin"
    );
    assert_eq!(sim.movable_covers.iter().filter(|&&c| c == drum).count(), 1);
    // Destruction debris is the only growth: no stumps or dead bodies are left behind.
    assert_eq!(sim.covers.len(), cover_count);
    assert_eq!(sim.world.bodies.len(), bodies + sim.fragments.len());
    assert_eq!(
        sim.rng.state, rng,
        "rules and rebuilds draw no gameplay randomness"
    );
}

#[test]
fn debris_lingers_while_the_fragment_budget_has_room_then_resumes_the_normal_fade() {
    let mut sim = superstress(731.0);
    sim.fragment(0.0, 0.0, 0xffffff, 0.5, FragmentShape::Shard, 1.0);
    let piece = sim.fragments[0].id;
    let piece_index = |sim: &Simulation| sim.fragments.iter().position(|f| f.id == piece).unwrap();
    let i = piece_index(&sim);
    sim.fragments[i].life = DEBRIS_CLEANUP_SECONDS + 0.1;
    superstress_rules(&mut sim);
    let i = piece_index(&sim);
    assert!(
        sim.fragments[i].life > DEBRIS_CLEANUP_SECONDS + 1.0,
        "settling debris is held"
    );

    while (sim.fragments.len() as f64) < sim.max_fragments as f64 * 0.8 {
        sim.fragment(0.0, 0.0, 0xffffff, 0.5, FragmentShape::Shard, 1.0);
    }
    let i = piece_index(&sim);
    sim.fragments[i].life = DEBRIS_CLEANUP_SECONDS + 0.1;
    superstress_rules(&mut sim);
    let i = piece_index(&sim);
    assert_eq!(
        sim.fragments[i].life,
        DEBRIS_CLEANUP_SECONDS + 0.1,
        "a full budget lets debris fade"
    );
}

#[test]
fn a_seeded_superstress_brawl_keeps_rebuilding_its_cover_and_stays_inside_its_bounds() {
    let mut sim = superstress(4242.0);
    let cover_count = sim.covers.len();
    let mut restored = HashSet::new();
    sim.start();
    // Two rebuild delays, so every cover felled in the first six seconds has had time to
    // return; a shorter window hinges on whether one opening shot sets off a drum chain.
    let steps = (2.0 * REBUILD_SECONDS) / STEP;
    let mut i = 0;
    while (i as f64) < steps {
        sim.step(VehicleCommand::idle(), true);
        for event in sim.events.drain(..) {
            if event.kind == SimEventType::Impact
                && event.cover_kind.is_some()
                && let Some(id) = event.id
            {
                restored.insert(id);
            }
        }
        assert!(sim.fragments.len() <= sim.max_fragments);
        i += 1;
    }
    assert!(sim.destroyed > 15, "only {} covers fell", sim.destroyed);
    assert!(restored.len() > 5, "only {} covers rebuilt", restored.len());
    assert_eq!(sim.covers.len(), cover_count);
    for tank in sim.tanks.iter().filter(|t| t.alive) {
        let p = sim.body_translation(tank.body);
        assert!(
            p.x.abs().max(p.z.abs()) < YARD,
            "tanks stay inside the fence"
        );
    }
}
