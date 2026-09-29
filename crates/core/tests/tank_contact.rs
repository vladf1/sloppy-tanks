//! Tank hull contact: boosted tanks and walls stop at the visible hull edges, and the
//! model-sized contact collider follows respawns and resets (the former
//! `tests/tank-contact.test.ts`). The TS measured the hulls from the rendered models; the
//! same measurements are the `tank_hull` constants here.

mod support;

use std::f64::consts::PI;

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::{MOVE_ACCELERATION, STEP, group, vehicle};
use sloppy_core::sim::math::Quat4;
use sloppy_core::sim::physics::{to_rotation, vector};
use sloppy_core::sim::simulation::packed_groups;
use sloppy_core::sim::tank_dimensions::tank_hull;
use sloppy_core::sim::{CoverKind, Simulation, Team, VehicleKind};
use support::clear_arena;

const TANKS: [VehicleKind; 3] = [
    VehicleKind::Scout,
    VehicleKind::Balanced,
    VehicleKind::Heavy,
];

/// World-space Z bounds of a hull whose model is turned `yaw` radians about Y, as
/// `Box3.setFromObject` measured them.
fn hull_z_bounds(kind: VehicleKind, yaw: f64) -> (f64, f64) {
    let hull = tank_hull(kind);
    let (sin, cos) = yaw.sin_cos();
    let center = -hull.center.x * sin + hull.center.z * cos;
    let half = (sin.abs() * hull.size.x + cos.abs() * hull.size.z) / 2.0;
    (center - half, center + half)
}

/// An empty map holding only the given human tanks, which become indices `0..`.
fn empty_arena(tanks: &[(Team, VehicleKind)]) -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    clear_arena(&mut s, &[]);
    for &(team, kind) in tanks {
        s.add_tank(team, true, kind, 0);
    }
    s
}

fn place_locked(s: &mut Simulation, index: usize, x: f64, z: f64, yaw: f64) {
    let body = &mut s.world.bodies[s.tanks[index].body];
    body.set_translation(vector(x, 0.65, z), true);
    body.set_rotation(to_rotation(Quat4::yaw(yaw)), true);
    body.lock_rotations(true, true);
}

/// Apply one tick of bounded drive impulse toward the planar velocity `(vx, vz)`.
fn push_toward(s: &mut Simulation, index: usize, vx: f64, vz: f64) {
    let body = &mut s.world.bodies[s.tanks[index].body];
    let v = body.linvel();
    let dx = vx - v.x as f64;
    let dz = vz - v.z as f64;
    let length = dx.hypot(dz);
    let amount = 1f64.min((MOVE_ACCELERATION * STEP) / if length == 0.0 { 1.0 } else { length });
    let mass = body.mass() as f64;
    body.apply_impulse(vector(dx * amount * mass, 0.0, dz * amount * mass), true);
}

/// Apply one tick of bounded drive impulse along Z toward `target` speed.
fn push_along_z(s: &mut Simulation, index: usize, target: f64) {
    let body = &mut s.world.bodies[s.tanks[index].body];
    let limit = MOVE_ACCELERATION * STEP;
    let change = (-limit).max(limit.min(target - body.linvel().z as f64));
    let mass = body.mass() as f64;
    body.apply_impulse(vector(0.0, 0.0, change * mass), true);
}

fn step_world(s: &mut Simulation) {
    s.world.integration_parameters.dt = STEP as f32;
    s.world.step();
}

#[test]
fn boosted_tanks_stop_at_visible_hull_edges_in_head_on_and_side_contacts_including_rotated_hulls() {
    for kind in TANKS {
        for axis_x in [true, false] {
            for angle in [0.0, PI / 3.0] {
                for team in [Team::Blue, Team::Red] {
                    let mut s = empty_arena(&[(Team::Blue, kind), (team, kind)]);
                    let size = tank_hull(kind).size;
                    let expected = if axis_x { size.x } else { size.z };
                    let (dir_x, dir_z) = if axis_x {
                        (angle.cos(), -angle.sin())
                    } else {
                        (angle.sin(), angle.cos())
                    };
                    for i in 0..2 {
                        let side = if i == 0 { -1.0 } else { 1.0 };
                        place_locked(&mut s, i, dir_x * side * 4.0, dir_z * side * 4.0, angle);
                        let mass = s.world.bodies[s.tanks[i].body].mass() as f64;
                        assert!((mass - vehicle(kind).mass).abs() < 0.001);
                    }
                    let mut minimum = f64::INFINITY;
                    for _ in 0..120 {
                        for i in 0..2 {
                            let speed = vehicle(kind).speed * 1.5 * if i == 0 { 1.0 } else { -1.0 };
                            push_toward(&mut s, i, dir_x * speed, dir_z * speed);
                        }
                        step_world(&mut s);
                        let p = s.body_translation(s.tanks[0].body);
                        let q = s.body_translation(s.tanks[1].body);
                        minimum = minimum.min((q.x - p.x) * dir_x + (q.z - p.z) * dir_z);
                    }
                    let axis = if axis_x { "x" } else { "z" };
                    assert!(
                        minimum >= expected - 0.01,
                        "{kind:?} {axis} {angle} {team:?}: {minimum} versus {expected}"
                    );
                }
            }
        }
    }
}

#[test]
fn model_sized_contact_collider_is_recreated_on_class_changing_respawn_and_cleaned_on_death() {
    let mut s = Simulation::with_seed(123.0);
    let t = s.human_index().expect("human");
    let count = s.world.colliders.len();
    let hull = &s.world.colliders[s.tanks[t].collider];
    let (restitution, friction) = (hull.restitution(), hull.friction());
    s.tanks[t].protection = 0.0;
    s.damage_tank(t, 1000.0, 999, Team::Red, None, None);
    s.human_kind = VehicleKind::Heavy;
    s.respawn(t, None);
    let body = &s.world.bodies[s.tanks[t].body];
    assert_eq!(body.colliders().len(), 2);
    let hull = &s.world.colliders[s.tanks[t].collider];
    assert_eq!(
        hull.restitution(),
        restitution,
        "respawn preserves hull bounce"
    );
    assert_eq!(hull.friction(), friction, "respawn preserves hull friction");
    let contact = &s.world.colliders[body.colliders()[1]];
    assert_eq!(
        packed_groups(contact.collision_groups()),
        group::TANK_CONTACT
    );
    let half_extents = contact
        .shape()
        .as_cuboid()
        .expect("cuboid contact hull")
        .half_extents;
    assert!((half_extents.z as f64 * 2.0 - tank_hull(VehicleKind::Heavy).size.z).abs() < 0.00001);
    assert!((body.mass() as f64 - vehicle(VehicleKind::Heavy).mass).abs() < 0.001);
    s.reset(None);
    assert_eq!(s.world.colliders.len(), count);
}

#[test]
fn different_chassis_meeting_at_right_angles_cannot_overlap_their_visible_hulls() {
    for a_kind in TANKS {
        for b_kind in TANKS {
            let mut s = empty_arena(&[(Team::Blue, a_kind), (Team::Red, b_kind)]);
            let mut expected = 0.0;
            for i in 0..2 {
                let yaw = (i as f64 * PI) / 2.0;
                let (min_z, max_z) = hull_z_bounds(s.tanks[i].kind, yaw);
                expected += if i == 0 { max_z } else { -min_z };
                place_locked(&mut s, i, 0.0, if i == 0 { -6.0 } else { 6.0 }, yaw);
            }
            let mut minimum = f64::INFINITY;
            for _ in 0..180 {
                for i in 0..2 {
                    let target =
                        vehicle(s.tanks[i].kind).speed * 1.5 * if i == 0 { 1.0 } else { -1.0 };
                    push_along_z(&mut s, i, target);
                }
                step_world(&mut s);
                minimum = minimum.min(
                    s.body_translation(s.tanks[1].body).z - s.body_translation(s.tanks[0].body).z,
                );
            }
            assert!(
                minimum >= expected - 0.015,
                "{a_kind:?}/{b_kind:?}: {minimum} versus {expected}"
            );
        }
    }
}

#[test]
fn long_hulls_stop_at_walls_using_their_visible_nose_and_tail() {
    for kind in TANKS {
        for side in [-1.0, 1.0] {
            let mut s = empty_arena(&[(Team::Blue, kind)]);
            let body = &mut s.world.bodies[s.tanks[0].body];
            body.set_translation(vector(0.0, 0.65, -side * 7.0), true);
            body.lock_rotations(true, true);
            s.add_cover(&CoverDef::new(
                CoverKind::Concrete,
                0.0,
                0.0,
                20.0,
                0.5,
                3.0,
                f64::INFINITY,
                0,
            ));
            let (min_z, max_z) = hull_z_bounds(kind, 0.0);
            let reach = if side == 1.0 { max_z } else { -min_z };
            for frame in 0..180 {
                push_along_z(&mut s, 0, vehicle(kind).speed * side * 1.5);
                step_world(&mut s);
                let z = s.body_translation(s.tanks[0].body).z;
                assert!(
                    z * side + reach <= -0.25 + 0.015,
                    "{kind:?}/{side} clipped wall at frame {frame}: {}",
                    z * side + reach
                );
            }
        }
    }
}

#[test]
fn big_rig_is_the_widest_chassis_while_tank_sizes_remain_comparable() {
    let widths = TANKS.map(|kind| tank_hull(kind).size.x);
    assert!(widths[2] > widths[1] && widths[2] > widths[0]);
    let widest = widths.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let narrowest = widths.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(widest / narrowest < 1.15);
    assert!(widths.iter().all(|&width| width > 1.8 && width < 2.2));
}
