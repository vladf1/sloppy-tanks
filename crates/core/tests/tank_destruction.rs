//! Tank destruction (the former `tests/tank-destruction.test.ts`): burnout selection, quiet
//! and dramatic wrecks, breakup flight and landing, the shared debris cap, and Humvee tumbles.

mod support;

use std::collections::BTreeSet;

use sloppy_core::sim::math::{Random, Vec2};
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::tank_destruction::{humvee_tumble, tank_burnout};
use sloppy_core::sim::wrecks::break_tank;
use sloppy_core::sim::{
    CoverKind, DamageCause, DeathStyle, FragmentShape, SimEventType, Simulation, VehicleKind,
    WreckPart,
};
use support::{clear_arena, place_tank};

fn part_name(part: Option<WreckPart>) -> &'static str {
    match part {
        Some(WreckPart::Intact) => "intact",
        Some(WreckPart::Hull) => "hull",
        Some(WreckPart::Turret) => "turret",
        Some(WreckPart::TurretBarrel) => "turret-barrel",
        Some(WreckPart::Barrel) => "barrel",
        None => "",
    }
}

#[test]
fn burnouts_stay_near_one_fifth_of_deaths_with_repeatable_selection() {
    let mut count = 0;
    for id in 1..=1000 {
        if tank_burnout(123.0, id, 1) {
            count += 1;
        }
        assert_eq!(tank_burnout(123.0, id, 1), tank_burnout(123.0, id, 1));
    }
    assert!(count > 160 && count < 240, "{count}");
}

#[test]
fn quiet_kills_keep_one_grounded_wreck_and_dramatic_kills_retain_launched_pieces() {
    for quiet in [true, false] {
        let mut sim = Simulation::with_seed(123.0);
        let human = sim.human_index().unwrap();
        sim.tanks[human].protection = 0.0;
        while tank_burnout(sim.seed, sim.tanks[human].id, sim.tanks[human].life + 1) != quiet {
            sim.seed += 1.0;
        }
        let origin = sim.body_translation(sim.tanks[human].body);
        let enemy = sim.tanks[human].team.opponent();
        sim.damage_tank(human, 9999.0, 999, enemy, None, None);
        let death = sim
            .events
            .iter()
            .find(|e| e.kind == SimEventType::Death)
            .unwrap();
        assert_eq!(death.death_style == Some(DeathStyle::Burnout), quiet);
        if quiet {
            assert_eq!(sim.fragments.len(), 1);
            let wreck = sim.fragments[0].clone();
            assert_eq!(wreck.part, Some(WreckPart::Intact));
            assert_eq!(wreck.created_at, Some(sim.elapsed));
            let lift = sim.body_linvel(wreck.body).y;
            assert!(lift > 6.0 && lift < 7.1, "{lift}");
            let spin = sim.world.bodies[wreck.body].angvel();
            assert!((spin.x as f64).hypot(spin.z as f64) > 0.5);
            let start_y = sim.body_translation(wreck.body).y;
            let mut peak_y = start_y;
            for _ in 0..120 {
                sim.world.step();
                peak_y = peak_y.max(sim.body_translation(wreck.body).y);
            }
            assert!(
                peak_y - start_y > 0.8 && peak_y - start_y < 1.2,
                "hop stays around one metre: {}",
                peak_y - start_y
            );
            let p = sim.body_translation(wreck.body);
            assert!(p.y < 1.5 && p.y > 0.0);
            assert!((p.x - origin.x).hypot(p.z - origin.z) < 2.0);
        } else {
            assert!(sim.fragments.len() >= 2);
            assert!(
                sim.fragments
                    .iter()
                    .any(|f| sim.body_linvel(f.body).y > 5.0)
            );
        }
        sim.reset(None);
        assert_eq!(sim.fragments.len(), 0);
    }
}

#[test]
fn barrel_detonation_events_carry_their_source_without_tagging_shell_blasts() {
    let mut sim = Simulation::with_seed(123.0);
    let barrel = sim
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Drum)
        .unwrap();
    let (id, team) = (sim.human().id, sim.human().team);
    sim.damage_cover(barrel, 9999.0, id, team, None, None);
    assert!(
        sim.events
            .iter()
            .any(|e| e.kind == SimEventType::Explosion && e.cover_kind == Some(CoverKind::Drum))
    );
    sim.events.clear();
    sim.explode(
        Vec2::new(50.0, 50.0),
        0.1,
        0.0,
        id,
        team,
        None,
        DamageCause::Explosion,
    );
    let explosion = sim
        .events
        .iter()
        .find(|e| e.kind == SimEventType::Explosion)
        .unwrap();
    assert_eq!(explosion.cover_kind, None);
}

#[test]
fn tank_breakup_varies_assemblies_travels_widely_and_lands_after_flight() {
    let mut variants = BTreeSet::new();
    let mut axes = BTreeSet::new();
    let mut high_launches = 0;
    let mut highest = 0f64;
    // Seeds chosen to cover a high turret launch, both assemblies and all three tumble axes.
    let seeds = [3.0, 7.0, 8.0];
    for seed in seeds {
        let mut s = Simulation::with_seed(123.0);
        let human = s.human_index().unwrap();
        clear_arena(&mut s, &[human]);
        s.start();
        s.tanks[0].protection = 0.0;
        place_tank(&mut s, 0, 0.0, 0.0, None);
        s.world.step();
        s.rng = Random::new(seed);
        let enemy = s.tanks[0].team.opponent();
        s.damage_tank(0, 1000.0, 999, enemy, None, None);
        let pieces = s.fragments.clone();
        let mut names: Vec<&str> = pieces.iter().map(|f| part_name(f.part)).collect();
        names.sort();
        assert!(names.contains(&"hull"));
        assert!(
            names.contains(&"turret-barrel")
                || (names.contains(&"turret") && names.contains(&"barrel"))
        );
        variants.insert(names.join("/"));
        assert!(pieces.len() <= 3);
        if s.body_linvel(pieces[1].body).y.powi(2) / 44.0 >= 20.0 {
            high_launches += 1;
        }
        assert!(
            s.body_linvel(pieces[0].body).y.powi(2) / 44.0 <= 8.01,
            "hulls keep normal arcs"
        );
        for piece in &pieces {
            let v = s.world.bodies[piece.body].angvel();
            let components = [("x", v.x as f64), ("y", v.y as f64), ("z", v.z as f64)];
            let speed =
                (components[0].1.powi(2) + components[1].1.powi(2) + components[2].1.powi(2))
                    .sqrt();
            assert!((6.99..=14.01).contains(&speed), "spin {speed}");
            // The first of equally large components wins, like the stable sort it replaces.
            let mut dominant = components[0];
            for component in &components[1..] {
                if component.1.abs() > dominant.1.abs() {
                    dominant = *component;
                }
            }
            axes.insert(dominant.0);
        }
        let targets: Vec<Vec2> = pieces[..2]
            .iter()
            .map(|piece| {
                let p = s.body_translation(piece.body);
                let v = s.body_linvel(piece.body);
                let flight = (v.y + (v.y.powi(2) + 44.0 * 0f64.max(p.y - 0.3)).sqrt()) / 22.0;
                Vec2::new(p.x + v.x * flight, p.z + v.z * flight)
            })
            .collect();
        assert!(
            (targets[0].x - targets[1].x).hypot(targets[0].z - targets[1].z) > 12.0,
            "launch aims hull and turret several tank lengths apart; later contacts may deflect them"
        );
        let longest = pieces
            .iter()
            .map(|f| f.life - 3.2)
            .fold(f64::NEG_INFINITY, f64::max);
        let landing_steps = (longest * 60.0).ceil() as usize + 60;
        for _ in 0..landing_steps {
            s.world.step();
            for piece in &pieces {
                highest = highest.max(s.body_translation(piece.body).y);
            }
        }
        assert!(
            pieces.iter().all(|f| s.body_translation(f.body).y < 2.0),
            "parts land after their ballistic flight, including high launches"
        );
    }
    assert_eq!(variants.len(), 2);
    assert!(highest > 20.0, "some turrets take spectacular high arcs");
    assert!(
        high_launches > 0 && high_launches < seeds.len(),
        "high launches are occasional"
    );
    assert_eq!(axes.len(), 3, "tumbling varies across all three axes");
}

#[test]
fn deaths_during_a_full_fragment_burst_remain_within_the_shared_debris_and_wreck_cap() {
    let mut s = Simulation::with_seed(123.0);
    s.start();
    for tank in &mut s.tanks {
        tank.protection = 0.0;
    }
    for _ in 0..150 {
        s.fragment(0.0, 0.0, 0, 0.5, FragmentShape::Shard, 1.0);
    }
    assert_eq!(s.fragments.len(), s.max_fragments);
    for i in 0..s.tanks.len() {
        let enemy = s.tanks[i].team.opponent();
        s.damage_tank(i, 1000.0, 999, enemy, None, None);
        assert!(s.fragments.len() <= s.max_fragments);
    }
}

#[test]
fn humvee_deaths_alternate_between_quiet_burnouts_and_bounded_whole_vehicle_tumbles() {
    for quiet in [false, true] {
        let mut simulation = Simulation::with_seed(123.0);
        let hunter = simulation
            .tanks
            .iter()
            .position(|tank| tank.kind == VehicleKind::Humvee)
            .expect("the default roster has a Humvee");
        let hunter_team = simulation.tanks[hunter].team;
        let attacker = simulation
            .tanks
            .iter()
            .position(|tank| tank.team != hunter_team)
            .unwrap();
        simulation.tanks[hunter].protection = 0.0;
        while tank_burnout(
            simulation.seed,
            simulation.tanks[hunter].id,
            simulation.tanks[hunter].life + 1,
        ) != quiet
        {
            simulation.seed += 1.0;
        }
        let bodies = simulation.world.bodies.len();
        let (attacker_id, attacker_team) = (
            simulation.tanks[attacker].id,
            simulation.tanks[attacker].team,
        );
        let hunter_id = simulation.tanks[hunter].id;
        simulation.damage_tank(hunter, 1000.0, attacker_id, attacker_team, None, None);
        let death = simulation
            .events
            .iter()
            .find(|event| event.kind == SimEventType::Death && event.id == Some(hunter_id))
            .expect("death event");
        assert_eq!(death.death_style == Some(DeathStyle::Burnout), quiet);
        let pieces: Vec<_> = simulation
            .fragments
            .iter()
            .filter(|fragment| fragment.wreck == Some(VehicleKind::Humvee))
            .collect();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].part, Some(WreckPart::Intact));
        let lift = simulation.body_linvel(pieces[0].body).y;
        let w = simulation.world.bodies[pieces[0].body].angvel();
        let spin = ((w.x as f64).powi(2) + (w.y as f64).powi(2) + (w.z as f64).powi(2)).sqrt();
        if quiet {
            assert!(lift > 6.0 && lift < 7.1, "{lift}");
            assert!(spin < 1.0, "{spin}");
        } else {
            assert!(lift > 4.0 && lift < 11.0, "{lift}");
            assert!(spin > 3.0 && spin < 7.0, "{spin}");
        }
        assert_eq!(simulation.world.bodies.len(), bodies);
    }
}

#[test]
fn humvee_low_rolls_settle_on_a_side_at_either_heading_without_consuming_combat_rng() {
    for heading in [0.0, std::f64::consts::FRAC_PI_2] {
        let mut simulation = Simulation::with_seed(2.0);
        let mut control = Simulation::with_seed(2.0);
        let hunter = simulation
            .tanks
            .iter()
            .position(|tank| tank.kind == VehicleKind::Humvee)
            .expect("the seed-2 roster has a Humvee");
        let covers: Vec<_> = simulation.covers.iter().map(|cover| cover.body).collect();
        for body in covers {
            simulation.remove_body(body);
        }
        let others: Vec<_> = simulation
            .tanks
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != hunter)
            .map(|(_, tank)| tank.body)
            .collect();
        for body in others {
            simulation.remove_body(body);
        }
        simulation.tanks[hunter].heading = heading;
        let body = simulation.tanks[hunter].body;
        simulation.world.bodies[body].set_translation(vector(0.0, 0.65, 0.0), true);
        simulation.world.bodies[body].set_linvel(vector(0.0, 0.0, 0.0), true);
        break_tank(&mut simulation, hunter, false);
        assert_eq!(simulation.rng.next(), control.rng.next());
        let wreck = simulation
            .fragments
            .iter()
            .find(|fragment| fragment.wreck == Some(VehicleKind::Humvee))
            .unwrap()
            .body;
        for _ in 0..300 {
            simulation.world.step();
        }
        let q = simulation.body_rotation(wreck);
        let side_up = 2.0 * (q.x * q.y + q.w * q.z);
        assert!(side_up.abs() > 0.95, "expected side landing, got {side_up}");
    }
}

#[test]
fn humvee_tumble_selection_varies_axes_and_direction_reproducibly() {
    let motions: Vec<_> = (0..64)
        .map(|seed| humvee_tumble(seed as f64, 9, 1))
        .collect();
    let heights: BTreeSet<u64> = motions
        .iter()
        .map(|motion| motion.height.to_bits())
        .collect();
    assert_eq!(heights.len(), 4);
    assert!(motions.iter().any(|motion| motion.roll > 0.0));
    assert!(motions.iter().any(|motion| motion.roll < 0.0));
    assert_eq!(humvee_tumble(12.0, 9, 1), humvee_tumble(12.0, 9, 1));
}
