//! The Cybertruck easter egg: neutral roaming targets on debug single-player pages, the
//! gold truck's blast, and the wheel a shell striking near it can knock off.

mod support;

use rapier3d::prelude::Vector;
use sloppy_core::models::CYBERTRUCK_WHEEL_RADIUS;
use sloppy_core::sim::cybertruck::{CYBERTRUCK_COUNT, CYBERTRUCK_RESPAWN_SECONDS, GOLD_EVERY};
use sloppy_core::sim::data::{STEP, vehicle};
use sloppy_core::sim::math::{Point3, Vec2, distance};
use sloppy_core::sim::physics::from_vector;
use sloppy_core::sim::tank_driving::drive_tank;
use sloppy_core::sim::{Shot, Simulation, SimulationSetup, VehicleCommand, VehicleKind, WreckPart};
use support::{clear_arena, damage_from, idle, place_tank, tank_xz};

const PLAYER: usize = 0;
const TRUCK: usize = 1;
const ENEMY: usize = 2;

fn with_trucks(seed: f64) -> Simulation {
    let mut s = Simulation::new(
        seed,
        SimulationSetup {
            cybertrucks: Some(true),
            ..SimulationSetup::default()
        },
    );
    s.start();
    s
}

/// The human, a truck of `kind` parked at the origin facing +Z and an enemy, on an
/// empty floor.
fn truck_range(kind: VehicleKind) -> Simulation {
    let mut s = with_trucks(77.0);
    let player = s.human_index().unwrap();
    let truck = s.tanks.iter().position(|t| t.kind == kind).unwrap();
    let enemy = s
        .tanks
        .iter()
        .position(|t| t.team != s.human_team && !t.kind.neutral())
        .unwrap();
    clear_arena(&mut s, &[player, truck, enemy]);
    place_tank(&mut s, PLAYER, 0.0, -40.0, Some(0.0));
    place_tank(&mut s, TRUCK, 0.0, 0.0, Some(0.0));
    place_tank(&mut s, ENEMY, 30.0, 30.0, Some(0.0));
    for tank in &mut s.tanks {
        tank.protection = 0.0;
    }
    s.world.step();
    s
}

fn truck_shots(s: &Simulation) -> usize {
    let trucks: Vec<u32> = s
        .tanks
        .iter()
        .filter(|t| t.kind.neutral())
        .map(|t| t.id)
        .collect();
    s.shots
        .iter()
        .filter(|shot| trucks.contains(&shot.owner))
        .count()
}

/// A player's standard shell crossing from -X toward local point `(x, z)` of the parked
/// truck, which faces +Z so its local axes are the world's.
fn shell_at(s: &mut Simulation, x: f64, z: f64) {
    let id = s.allocate_id();
    let player = &s.tanks[PLAYER];
    s.shots.push(Shot {
        id,
        x: x - 6.0,
        z,
        vx: 30.0,
        vz: 0.0,
        team: player.team,
        owner: player.id,
        owner_life: Some(player.life),
        damage: 1.0,
        life: 1.0,
        ..Shot::default()
    });
}

/// The loose wheel's centre and axle direction.
fn wheel_pose(s: &Simulation) -> (Point3, Vector) {
    let wheel = wheel_fragment(s).expect("a loose wheel");
    let body = &s.world.bodies[s.fragments[wheel].body];
    (
        from_vector(body.translation()),
        *body.rotation() * Vector::Y,
    )
}

fn wheel_fragment(s: &Simulation) -> Option<usize> {
    s.fragments
        .iter()
        .position(|f| f.part == Some(WreckPart::Wheel))
}

/// Shells at the truck's rear-left hub, the truck parked again and repaired each time,
/// until one knocks the wheel off. Returns the shots it took.
fn knock_off_rear_left_wheel(s: &mut Simulation) -> usize {
    let scale = vehicle(VehicleKind::Cybertruck).scale;
    for attempt in 1..=40 {
        place_tank(s, TRUCK, 0.0, 0.0, Some(0.0));
        s.tanks[TRUCK].hp = s.max_health(&s.tanks[TRUCK]);
        shell_at(s, -0.84 * scale, -1.8 * scale);
        for _ in 0..30 {
            idle(s);
            if s.shots.is_empty() {
                break;
            }
        }
        if s.tanks[TRUCK].careen.is_some() {
            return attempt;
        }
    }
    panic!("forty shells at a hub never knocked the wheel off");
}

#[test]
fn rounds_add_neutral_trucks_only_when_asked() {
    let plain = Simulation::with_seed(5.0);
    assert!(plain.tanks.iter().all(|t| !t.kind.neutral()));
    let s = with_trucks(5.0);
    let trucks: Vec<_> = s.tanks.iter().filter(|t| t.kind.neutral()).collect();
    assert_eq!(trucks.len(), CYBERTRUCK_COUNT);
    let gold = trucks
        .iter()
        .filter(|t| t.kind == VehicleKind::GoldCybertruck)
        .count();
    assert_eq!(gold, CYBERTRUCK_COUNT / GOLD_EVERY);
    // On the opposing side only so the player's shells strike them.
    assert!(trucks.iter().all(|t| t.team == s.human_team.opponent()));
}

#[test]
fn trucks_roam_without_firing_and_bots_ignore_them() {
    let mut s = with_trucks(11.0);
    let trucks: Vec<usize> = (0..s.tanks.len())
        .filter(|&i| s.tanks[i].kind.neutral())
        .collect();
    let start: Vec<Vec2> = trucks.iter().map(|&i| tank_xz(&s, i)).collect();
    let ids: Vec<u32> = trucks.iter().map(|&i| s.tanks[i].id).collect();
    for _ in 0..(20.0 / STEP) as usize {
        s.step(VehicleCommand::idle(), true);
        assert_eq!(truck_shots(&s), 0);
        for tank in s.tanks.iter().filter(|t| !t.kind.neutral()) {
            assert!(
                !ids.contains(&tank.brain.target),
                "{} hunts a truck",
                tank.name
            );
        }
    }
    for (&i, from) in trucks.iter().zip(start) {
        if s.tanks[i].alive {
            assert!(distance(tank_xz(&s, i), from) > 5.0, "truck {i} stayed put");
        }
    }
}

#[test]
fn trucks_turn_round_like_cars_instead_of_reversing_or_pivoting() {
    let mut s = truck_range(VehicleKind::Cybertruck);
    // Told to head back the way it faces, from rest.
    let command = VehicleCommand {
        move_z: -1.0,
        ..VehicleCommand::idle()
    };
    let mut travelled = 0.0;
    let mut turned_at = None;
    for tick in 0..(4.0 / STEP) as usize {
        let from = tank_xz(&s, TRUCK);
        let tank = &mut s.tanks[TRUCK];
        let body = &mut s.world.bodies[tank.body];
        drive_tank(tank, body, &command, STEP, 1.0);
        s.world.step();
        let to = tank_xz(&s, TRUCK);
        let heading = s.tanks[TRUCK].heading;
        let along = (to.x - from.x) * heading.sin() + (to.z - from.z) * heading.cos();
        assert!(along > -1e-3, "reversed at tick {tick}");
        travelled += distance(from, to);
        if turned_at.is_none() && heading.cos() < -0.95 {
            turned_at = Some((tick, travelled));
        }
    }
    let (tick, arc) = turned_at.expect("it turned round");
    // A tank pivots round in under a second; the truck drives an arc.
    assert!(tick as f64 * STEP > 1.0, "turned round in {tick} ticks");
    assert!(arc > 5.0, "turned round in {arc:.1} m");
}

#[test]
fn a_destroyed_truck_scores_nothing_and_returns() {
    let mut s = truck_range(VehicleKind::Cybertruck);
    let score = s.match_state.clone();
    damage_from(&mut s, TRUCK, 500.0, PLAYER, None, None);
    assert!(!s.tanks[TRUCK].alive);
    let player = &s.tanks[PLAYER];
    assert_eq!((player.kills, player.xp), (0, 0.0));
    assert_eq!(s.match_state, score);
    for _ in 0..((CYBERTRUCK_RESPAWN_SECONDS + 0.5) / STEP) as usize {
        idle(&mut s);
    }
    assert!(s.tanks[TRUCK].alive);
}

#[test]
fn a_truck_returns_while_the_player_is_dead() {
    let mut s = truck_range(VehicleKind::Cybertruck);
    damage_from(&mut s, TRUCK, 500.0, PLAYER, None, None);
    damage_from(&mut s, PLAYER, 500.0, ENEMY, None, None);
    assert!(!s.tanks[PLAYER].alive);
    s.tanks[TRUCK].respawn = STEP;
    idle(&mut s);
    assert!(s.tanks[TRUCK].alive);
}

#[test]
fn a_gold_truck_goes_up_like_a_fuel_drum() {
    let mut s = truck_range(VehicleKind::GoldCybertruck);
    place_tank(&mut s, ENEMY, 3.5, 0.0, Some(0.0));
    let before = s.tanks[ENEMY].hp;
    damage_from(&mut s, TRUCK, 500.0, PLAYER, None, None);
    assert!(
        s.tanks[ENEMY].hp < before,
        "the blast spared the enemy beside it"
    );
}

#[test]
fn a_shell_near_a_wheel_can_knock_it_off_and_the_truck_crashes() {
    let mut s = truck_range(VehicleKind::Cybertruck);
    // Shells far from every hub never cost a wheel.
    for _ in 0..12 {
        place_tank(&mut s, TRUCK, 0.0, 0.0, Some(0.0));
        s.tanks[TRUCK].hp = s.max_health(&s.tanks[TRUCK]);
        shell_at(&mut s, -0.9, 0.0);
        for _ in 0..30 {
            idle(&mut s);
        }
        assert!(s.tanks[TRUCK].careen.is_none());
    }
    let shots = knock_off_rear_left_wheel(&mut s);
    let careen = s.tanks[TRUCK].careen.unwrap();
    assert_eq!(careen.wheel, 0, "the rear-left hub");
    assert_eq!(careen.owner, s.tanks[PLAYER].id);
    assert!(shots <= 12, "a near shot took {shots} tries");
    let wheel = wheel_fragment(&s).expect("a loose wheel");
    let wheel_body = s.fragments[wheel].body;
    let launched = s.body_translation(wheel_body);
    // Careening drags the truck round toward the bare (-X, left) corner, then it crashes.
    let heading = s.tanks[TRUCK].heading;
    let mut ticks = 0;
    while s.tanks[TRUCK].alive {
        idle(&mut s);
        ticks += 1;
        assert!(ticks < 120, "the truck never crashed");
    }
    assert!(s.tanks[TRUCK].heading < heading - 0.3, "it did not veer");
    let wreck = s
        .fragments
        .iter()
        .find(|f| f.wreck == Some(VehicleKind::Cybertruck) && f.part == Some(WreckPart::Intact))
        .expect("a wreck");
    assert_eq!(wreck.lost_wheel, Some(0));
    // The loose wheel rolls away upright on its rim...
    let radius = CYBERTRUCK_WHEEL_RADIUS * vehicle(VehicleKind::Cybertruck).scale;
    for _ in 0..60 {
        idle(&mut s);
    }
    let (at, axle) = wheel_pose(&s);
    assert!(distance(at.planar(), launched.planar()) > 3.0, "{at:?}");
    assert!(
        (at.y - radius).abs() < 0.03 && axle.y.abs() < 0.2,
        "{at:?} {axle:?}"
    );
    // ...and once it slows down, wobbles over and lies flat.
    for _ in 0..(7.0 / STEP) as usize {
        idle(&mut s);
    }
    let (_, axle) = wheel_pose(&s);
    assert!(axle.y.abs() > 0.95, "still upright: {axle:?}");
}
