//! The Stress Grid extra level: 30 reachable spawns with hull clearance, a dense but bounded
//! destruction workload that keeps the authored maps' destructibility rules, and endless
//! rules that survive respawns and resets (the former `tests/stress-test-level.test.ts`).

use std::collections::HashSet;

use sloppy_core::sim::data::{STEP, vehicle};
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::maps::{GroundKind, MAPS};
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::stress_test_level::{
    STRESS_PLAYER_HEALTH_MULTIPLIER, STRESS_TANK_COUNT, STRESS_TEST_MAP, stress_test_level,
};
use sloppy_core::sim::{
    CoverKind, MatchPhase, Simulation, SimulationSetup, Team, VehicleCommand, VehicleKind,
};

/// Hull clearance a stress spawn or pickup keeps from every cover.
const CLEARANCE: f64 = 1.5;

fn stress_simulation(extra: SimulationSetup) -> Simulation {
    Simulation::new(
        731.0,
        single_player_rules(stress_test_level()).merged(SimulationSetup {
            round: Some(3),
            ..extra
        }),
    )
}

fn human(s: &Simulation) -> usize {
    s.human_index().expect("local play has a human")
}

#[test]
fn stress_pickups_and_all_30_spawns_have_hull_clearance_and_navigable_routes() {
    let mut sim = stress_simulation(SimulationSetup::default());
    assert_eq!(sim.tanks.len(), STRESS_TANK_COUNT);
    let mut points: Vec<Vec2> = sim.pickups.iter().map(|p| Vec2::new(p.x, p.z)).collect();
    points.extend(
        sim.tanks
            .iter()
            .map(|tank| sim.body_translation(tank.body).planar()),
    );
    for point in points {
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

#[test]
fn stress_grid_is_a_bounded_dense_destruction_workload() {
    let layout = (STRESS_TEST_MAP.layout)();
    let destructible = layout.iter().filter(|c| c.hp.is_finite()).count();
    let permanent_houses = layout
        .iter()
        .filter(|c| c.kind == CoverKind::House && !c.hp.is_finite())
        .count();
    let dragon_teeth: Vec<_> = layout
        .iter()
        .filter(|c| c.kind == CoverKind::Teeth)
        .collect();
    let drums: Vec<_> = layout
        .iter()
        .filter(|c| c.kind == CoverKind::Drum)
        .collect();
    // `+ 0.0` folds -0 into 0, as JS number-to-string does.
    let keys: Vec<String> = layout
        .iter()
        .map(|c| format!("{:?}:{}:{}", c.kind, c.x + 0.0, c.z + 0.0))
        .collect();

    assert_eq!(
        layout
            .iter()
            .filter(|c| c.kind == CoverKind::Boundary)
            .count(),
        4
    );
    assert_eq!(destructible, 75);
    assert_eq!(permanent_houses, 8);
    assert!(dragon_teeth.len() >= 24);
    assert!(dragon_teeth.iter().all(|c| !c.hp.is_finite()));
    assert!(!drums.is_empty());
    assert!(drums.iter().all(|c| c.x.hypot(c.z) > 15.0));
    assert!(layout.iter().filter(|c| c.kind == CoverKind::Tree).count() >= 20);
    assert!(layout.iter().any(|c| c.kind == CoverKind::Hedgehog));
    assert_eq!(
        keys.iter().collect::<HashSet<_>>().len(),
        keys.len(),
        "every stress obstacle needs a unique location and kind"
    );
    assert_eq!(STRESS_TEST_MAP.floor, Some(GroundKind::DryGrass));
    assert_eq!(STRESS_TEST_MAP.outer_floor, Some(GroundKind::PackedDirt));
    assert_eq!(STRESS_TEST_MAP.outer_floor_extent, Some(140.0));
    assert_eq!(STRESS_TEST_MAP.theme, None);
}

#[test]
fn stress_objects_keep_the_authored_maps_destructibility_rules() {
    let authored: Vec<_> = MAPS.iter().flat_map(|map| (map.layout)()).collect();
    let always_permanent: HashSet<CoverKind> = authored
        .iter()
        .filter(|cover| {
            !cover.hp.is_finite()
                && !authored
                    .iter()
                    .any(|candidate| candidate.kind == cover.kind && candidate.hp.is_finite())
        })
        .map(|cover| cover.kind)
        .collect();
    for cover in (STRESS_TEST_MAP.layout)() {
        if always_permanent.contains(&cover.kind) {
            assert_eq!(
                cover.hp,
                f64::INFINITY,
                "{:?} cannot become destructible in stress mode",
                cover.kind
            );
        }
    }
}

#[test]
fn stress_configuration_survives_respawns_and_resets_and_never_ends_at_the_normal_limits() {
    // The player keeps the tank chosen in Battle Setup.
    let mut sim = stress_simulation(SimulationSetup {
        human_kind: Some(VehicleKind::Heavy),
        ..SimulationSetup::default()
    });
    let initial_bodies = sim.world.bodies.len();
    for _ in 0..2 {
        assert_eq!(sim.map_name(), "STRESS GRID");
        assert_eq!(
            sim.tanks.iter().filter(|t| t.team == Team::Blue).count(),
            15
        );
        assert_eq!(sim.tanks.iter().filter(|t| t.team == Team::Red).count(), 15);
        let h = human(&sim);
        assert_eq!(sim.tanks[h].kind, VehicleKind::Heavy);
        let hp = vehicle(VehicleKind::Heavy).health * STRESS_PLAYER_HEALTH_MULTIPLIER;
        assert_eq!(sim.tanks[h].hp, hp);
        sim.start();
        sim.match_state.scores = [100, 100];
        sim.match_state.time = STEP;
        let human_team = sim.tanks[h].team;
        let enemy = sim.tanks.iter().position(|t| t.team != human_team).unwrap();
        sim.tanks[enemy].protection = 0.0;
        let human_id = sim.tanks[h].id;
        sim.damage_tank(enemy, 9999.0, human_id, human_team, None, None);
        sim.step(VehicleCommand::idle(), false);
        assert_eq!(sim.match_state.scores[human_team.index()], 101);
        assert_eq!(sim.match_state.phase, MatchPhase::Playing);
        assert_eq!(sim.match_state.winner, None);
        sim.tanks[h].protection = 0.0;
        let lethal = sim.max_health(&sim.tanks[h]) * 2.0;
        let (enemy_id, enemy_team) = (sim.tanks[enemy].id, sim.tanks[enemy].team);
        sim.damage_tank(h, lethal, enemy_id, enemy_team, None, None);
        sim.respawn(h, None);
        assert_eq!(sim.tanks[h].hp, hp);
        sim.reset(None);
        assert_eq!(sim.world.bodies.len(), initial_bodies);
        assert_eq!(sim.fragments.len(), 0);
        assert_eq!(sim.match_state.scores, [0, 0]);
    }
}
