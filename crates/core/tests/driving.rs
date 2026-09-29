//! Tank driving through the human command: braking before reversing, gradual hull turns,
//! arcing corners, release braking that keeps knockback, and normalized diagonal and boosted
//! speeds (the former `tests/driving.test.ts`).

mod support;

use std::f64::consts::PI;

use sloppy_core::sim::data::{MOVE_ACCELERATION, STEP, vehicle};
use sloppy_core::sim::math::{Vec2, angle_delta};
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::weapons::collect_pickup;
use sloppy_core::sim::{Pickup, PickupKind, Simulation, Team, VehicleCommand, VehicleKind};
use support::clear_arena;

/// An empty map with one human tank of `kind` at the origin, facing `heading`.
fn arena(kind: VehicleKind, heading: f64) -> (Simulation, usize) {
    let mut s = Simulation::with_seed(123.0);
    clear_arena(&mut s, &[]);
    let t = s.add_tank(Team::Blue, true, kind, 0);
    s.tanks[t].heading = heading;
    let body = s.tanks[t].body;
    s.world.bodies[body].set_translation(vector(0.0, 0.65, 0.0), true);
    s.tanks[t].previous = Vec2::new(0.0, 0.0);
    s.world.step();
    s.start();
    (s, t)
}

fn drive(s: &mut Simulation, angle: f64, steps: usize) {
    for _ in 0..steps {
        let command = VehicleCommand {
            move_x: angle.sin(),
            move_z: angle.cos(),
            aim: 0.7,
            ..VehicleCommand::idle()
        };
        s.step(command, false);
    }
}

fn idle(s: &mut Simulation) {
    s.step(VehicleCommand::idle(), false);
}

fn velocity(s: &Simulation, t: usize) -> Vec2 {
    s.body_linvel(s.tanks[t].body).planar()
}

fn position(s: &Simulation, t: usize) -> Vec2 {
    s.body_translation(s.tanks[t].body).planar()
}

fn planar_speed(s: &Simulation, t: usize) -> f64 {
    let v = velocity(s, t);
    v.x.hypot(v.z)
}

#[test]
fn opposite_input_brakes_then_reverses_without_turning_the_hull_on_any_chassis() {
    for kind in [
        VehicleKind::Scout,
        VehicleKind::Balanced,
        VehicleKind::Heavy,
    ] {
        for heading in [0.0, PI / 2.0, PI, -PI / 2.0] {
            let (mut s, t) = arena(kind, heading);
            let label = format!("{kind:?} heading {heading}");
            drive(&mut s, heading, 30);
            drive(&mut s, heading + PI, 1);
            let v = velocity(&s, t);
            assert!(
                v.x * heading.sin() + v.z * heading.cos() > 0.0,
                "{label}: velocity must brake before changing sign"
            );
            drive(&mut s, heading + PI, 29);
            assert!(
                angle_delta(heading, s.tanks[t].heading).abs() < 1e-8,
                "{label}"
            );
            let reverse = velocity(&s, t);
            let signed_speed = reverse.x * heading.sin() + reverse.z * heading.cos();
            assert!(
                signed_speed < -vehicle(kind).speed * 0.75,
                "{label}: {signed_speed}"
            );
            assert!(
                signed_speed > -vehicle(kind).speed * 0.85,
                "{label}: {signed_speed}"
            );
        }
    }
}

#[test]
fn perpendicular_input_gradually_turns_from_rest_without_strafing_then_reaches_full_speed() {
    for heading in [0.0, PI / 2.0, PI, -PI / 2.0] {
        for side in [-1.0, 1.0] {
            let (mut s, t) = arena(VehicleKind::Balanced, heading);
            let target = heading + (side * PI) / 2.0;
            let label = format!("heading {heading} side {side}");
            drive(&mut s, target, 1);
            assert!(
                angle_delta(heading, s.tanks[t].heading).abs() < 0.07,
                "{label}"
            );
            assert!(
                angle_delta(heading, s.tanks[t].heading) * side > 0.0,
                "{label}: broadside input favors forward steering"
            );
            assert!(planar_speed(&s, t) < 0.1, "{label}");
            drive(&mut s, target, 11);
            assert!(
                angle_delta(s.tanks[t].heading, target).abs() > 0.7,
                "{label}: still turning at 0.2 seconds"
            );
            let v = velocity(&s, t);
            let hull = s.tanks[t].heading;
            assert!(
                (v.x * hull.cos() - v.z * hull.sin()).abs() < 0.05,
                "{label}: drive follows the hull instead of the requested direction"
            );
            drive(&mut s, target, 18);
            assert!(
                angle_delta(s.tanks[t].heading, target).abs() < 1e-8,
                "{label}: aligned within half a second"
            );
            assert!(
                planar_speed(&s, t) > vehicle(VehicleKind::Balanced).speed * 0.98,
                "{label}"
            );
            assert_eq!(
                s.tanks[t].aim, 0.7,
                "{label}: turret aim remains independent of steering"
            );
        }
    }
}

#[test]
fn a_moving_right_angle_turn_sheds_speed_and_traces_an_arc_instead_of_changing_direction_instantly()
{
    let (mut s, t) = arena(VehicleKind::Balanced, 0.0);
    drive(&mut s, 0.0, 30);
    let start = position(&s, t);
    drive(&mut s, PI / 2.0, 1);
    assert!(
        velocity(&s, t).z > 5.0,
        "retains momentum on the first turning tick"
    );
    assert!(
        velocity(&s, t).x < 0.1,
        "does not immediately accelerate sideways"
    );
    drive(&mut s, PI / 2.0, 9);
    assert!(planar_speed(&s, t) < vehicle(VehicleKind::Balanced).speed * 0.5);
    drive(&mut s, PI / 2.0, 20);
    let end = position(&s, t);
    assert!(
        end.x - start.x > 1.0 && end.z - start.z > 0.3,
        "{start:?} -> {end:?}"
    );
    assert!(
        end.z - start.z < 1.5,
        "turn remains compact enough for responsive controls"
    );
}

#[test]
fn a_diagonal_behind_the_hull_makes_a_short_reverse_turn_across_the_angle_wrap() {
    let (mut s, t) = arena(VehicleKind::Balanced, PI - 0.1);
    let start = s.tanks[t].heading;
    drive(&mut s, PI / 4.0, 30);
    assert!(angle_delta(start, s.tanks[t].heading).abs() < PI / 2.0);
    assert!(angle_delta(s.tanks[t].heading, PI * 1.25).abs() < 1e-8);
    let v = velocity(&s, t);
    assert!(v.x > 0.0 && v.z > 0.0);
}

#[test]
fn release_brakes_promptly_and_stops_turning_external_knockback_is_not_erased() {
    let (mut s, t) = arena(VehicleKind::Balanced, 0.0);
    drive(&mut s, PI / 2.0, 10);
    let heading = s.tanks[t].heading;
    for _ in 0..12 {
        idle(&mut s);
    }
    assert_eq!(s.tanks[t].heading, heading);
    assert!(planar_speed(&s, t) < 0.01);
    let body = &mut s.world.bodies[s.tanks[t].body];
    let mass = body.mass() as f64;
    body.apply_impulse(vector(mass * 20.0, 0.0, 0.0), true);
    idle(&mut s);
    assert!(
        velocity(&s, t).x > 15.0,
        "bounded braking preserves impact momentum"
    );
}

#[test]
fn diagonal_input_is_normalized_release_brakes_to_rest_and_a_speed_boost_reaches_1_5x() {
    let mut distances = Vec::new();
    for diagonal in [false, true] {
        // Compare travel after alignment.
        let (mut s, t) = arena(VehicleKind::Balanced, if diagonal { PI / 4.0 } else { 0.0 });
        let input = VehicleCommand {
            move_x: if diagonal { 1.0 } else { 0.0 },
            move_z: 1.0,
            ..VehicleCommand::idle()
        };
        for _ in 0..60 {
            s.step(input, false);
        }
        let p = position(&s, t);
        distances.push(p.x.hypot(p.z));
        let top_speed = vehicle(s.tanks[t].kind).speed;
        assert!(planar_speed(&s, t) > top_speed * 0.98);
        let braking_steps = (top_speed / (MOVE_ACCELERATION * STEP)).ceil() as usize;
        for _ in 0..braking_steps {
            idle(&mut s);
        }
        assert!(planar_speed(&s, t) < 0.01);
        let id = s.next_id;
        s.next_id += 1;
        let mut boost = Pickup {
            id,
            kind: PickupKind::Speed,
            x: 0.0,
            z: 0.0,
            available: true,
            cooldown: 0.0,
            cooldown_duration: 0.0,
        };
        collect_pickup(&mut s, t, &mut boost);
        let boost_steps = ((top_speed * 1.5) / (MOVE_ACCELERATION * STEP)).ceil() as usize + 2;
        for _ in 0..boost_steps {
            s.step(input, false);
        }
        let speed = planar_speed(&s, t);
        assert!(
            (speed / top_speed - 1.5).abs() < 0.03,
            "diagonal {diagonal}: {speed}"
        );
    }
    assert!((distances[0] - distances[1]).abs() < 0.02, "{distances:?}");
}
