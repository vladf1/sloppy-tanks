//! The Stress Grid extra level: 30 reachable spawns with hull clearance, a dense but bounded
//! destruction workload that keeps the authored maps' destructibility rules, and endless
//! rules that survive respawns and resets (the former `tests/stress-test-level.test.ts`).

use crate::support;

use std::collections::HashSet;

use sloppy_core::sim::data::{STEP, vehicle, weapon};
use sloppy_core::sim::house_layout::HOUSE_HEALTH;
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::maps::MAPS;
use sloppy_core::sim::math::{Point3, Vec2};
use sloppy_core::sim::stress_test_level::{
    STRESS_PLAYER_HEALTH_MULTIPLIER, STRESS_TANK_COUNT, STRESS_TEST_MAP, stress_test_level,
};
use sloppy_core::sim::{
    CoverKind, MatchPhase, Simulation, SimulationSetup, Team, VehicleCommand, VehicleKind, Weapon,
};
use support::{assert_spawns_and_pickups_clear, damage_from, fell_cottage};

fn stress_simulation(extra: SimulationSetup) -> Simulation {
    Simulation::new(
        731.0,
        single_player_rules(stress_test_level()).merged(SimulationSetup {
            round: Some(3),
            ..extra
        }),
    )
}

#[test]
fn stress_pickups_and_all_30_spawns_have_hull_clearance_and_navigable_routes() {
    let mut sim = stress_simulation(SimulationSetup::default());
    assert_eq!(sim.tanks.len(), STRESS_TANK_COUNT);
    assert_spawns_and_pickups_clear(&mut sim);
}

#[test]
fn stress_grid_is_a_bounded_dense_destruction_workload() {
    let layout = (STRESS_TEST_MAP.layout)();
    let destructible = layout.iter().filter(|c| c.hp.is_finite()).count();
    let cottages = layout
        .iter()
        .filter(|c| c.kind == CoverKind::House && c.hp == HOUSE_HEALTH)
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
    assert_eq!(destructible, 83);
    assert_eq!(cottages, 8, "cottages fall like the village's");
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
        let h = sim.human_index().unwrap();
        assert_eq!(sim.tanks[h].kind, VehicleKind::Heavy);
        let hp = vehicle(VehicleKind::Heavy).health * STRESS_PLAYER_HEALTH_MULTIPLIER;
        assert_eq!(sim.tanks[h].hp, hp);
        sim.start();
        sim.match_state.scores = [100, 100];
        sim.match_state.time = STEP;
        let human_team = sim.tanks[h].team;
        let enemy = sim.tanks.iter().position(|t| t.team != human_team).unwrap();
        sim.tanks[enemy].protection = 0.0;
        damage_from(&mut sim, enemy, 9999.0, h, None, None);
        sim.step(VehicleCommand::idle(), false);
        assert_eq!(sim.match_state.scores[human_team.index()], 101);
        assert_eq!(sim.match_state.phase, MatchPhase::Playing);
        assert_eq!(sim.match_state.winner, None);
        sim.tanks[h].protection = 0.0;
        let lethal = sim.max_health(&sim.tanks[h]) * 2.0;
        damage_from(&mut sim, h, lethal, enemy, None, None);
        sim.respawn(h, None);
        assert_eq!(sim.tanks[h].hp, hp);
        sim.reset(None);
        assert_eq!(sim.world.bodies.len(), initial_bodies);
        assert_eq!(sim.fragments.len(), 0);
        assert_eq!(sim.match_state.scores, [0, 0]);
    }
}

/// The grid's cottages break where they are shot and fall when their footing is gone,
/// all eight at once within the grid's fragment budget, opening their footprints to
/// routes; a reset stands them back up with no body left over.
#[test]
fn stress_cottages_fall_within_the_fragment_budget() {
    let mut sim = stress_simulation(SimulationSetup::default());
    let bodies = sim.world.bodies.len();
    let covers = sim.covers.len();
    let houses: Vec<usize> = (0..sim.covers.len())
        .filter(|&i| sim.covers[i].kind == CoverKind::House)
        .collect();
    assert_eq!(houses.len(), 8);
    let (id, team) = (sim.human().id, sim.human().team);
    let shell = weapon(Weapon::Standard).damage;
    let house = houses[0];
    let c = &sim.covers[house];
    let impact = Point3::new(c.x + c.w / 2.0, 1.0, c.z);
    sim.damage_cover(house, shell, id, team, None, Some(impact));
    assert!(sim.covers[house].alive);
    assert!(
        !sim.covers[house]
            .structure
            .as_ref()
            .unwrap()
            .gone
            .is_empty(),
        "the shell breaks what it reaches"
    );
    for &house in &houses {
        fell_cottage(&mut sim, house);
    }
    assert!(sim.fragments.len() <= sim.max_fragments);
    assert!(sim.fragments.iter().any(|f| f.structure.is_some()));
    assert_eq!(sim.covers.len(), covers, "no heap: the ruins are the parts");
    for &house in &houses {
        let c = &sim.covers[house];
        assert!(!c.alive, "{} {} is levelled", c.x, c.z);
        assert!(
            !sim.nav.is_blocked(support::open_ruin_ground(&sim, house)),
            "its footprint opens"
        );
    }

    sim.reset(None);
    assert_eq!(sim.world.bodies.len(), bodies);
    assert_eq!(sim.covers.len(), covers);
    for &house in &houses {
        let c = &sim.covers[house];
        assert!(c.alive && c.hp == HOUSE_HEALTH);
        assert!(c.structure.as_ref().unwrap().gone.is_empty());
        assert!(sim.nav.is_blocked(Vec2::new(c.x, c.z)));
    }
}
