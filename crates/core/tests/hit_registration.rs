//! Hit registration: widened hull hit volumes and their reach bound, cover blocking, swept
//! hits on moving tanks, damage-event credit, muzzle placement, barrels poking into cover,
//! death causes and impact directions, shell interception ordering, and rocket acceleration.
//! Ported from `tests/hit-registration.test.ts`.
//!
//! The TypeScript test measured the rendered Three.js tank models for hull bounds and muzzle
//! positions. The core crate has no models; these tests use the recorded model measurement
//! in `tank_dimensions` (the values the TypeScript measurement produced) as that reference.

mod support;

use rapier3d::parry::query::{Ray, RayCast};
use rapier3d::parry::shape::Cuboid;
use rapier3d::prelude::Pose;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::combat_rules::MINE;
use sloppy_core::sim::data::{STEP, vehicle, weapon};
use sloppy_core::sim::hitboxes::{SHELL_HIT_RADIUS, ShotProbe, tank_hit_time};
use sloppy_core::sim::math::{Random, Vec2};
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::tank_dimensions::{tank_hull, tank_muzzle};
use sloppy_core::sim::weapons::{fire_weapon, interception_time, step_mines, step_projectiles};
use sloppy_core::sim::{
    CoverKind, DamageCause, DamageSource, Mine, Shot, SimEventType, Simulation, Tank, Team,
    VehicleKind, Weapon,
};
use support::{clear_arena, place_tank};

fn set_translation(s: &mut Simulation, index: usize, x: f64, z: f64) {
    let body = s.tanks[index].body;
    s.world.bodies[body].set_translation(vector(x, 0.65, z), true);
}

/// One red enemy target of `kind` at the origin (index 0), facing `heading`.
fn fixture(kind: VehicleKind, heading: f64) -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    let target = s
        .tanks
        .iter()
        .position(|t| !t.human && t.kind == kind)
        .unwrap();
    clear_arena(&mut s, &[target]);
    s.tanks[0].team = Team::Red;
    s.tanks[0].protection = 0.0;
    place_tank(&mut s, 0, 0.0, 0.0, Some(heading));
    s.world.step();
    s.start();
    s
}

/// The first `count` roster tanks in a column along +z; the first one is the human.
fn column(count: usize) -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    let keep: Vec<usize> = (0..count).collect();
    clear_arena(&mut s, &keep);
    for i in 0..s.tanks.len() {
        s.tanks[i].human = i == 0;
        s.tanks[i].protection = 0.0;
        place_tank(&mut s, i, 0.0, i as f64 * 12.0, None);
    }
    s.human_team = Team::Blue;
    s.world.step();
    s.start();
    s
}

const PLAYER: usize = 0;
const ENEMY: usize = 1;
const ALLY: usize = 2;

/// The human, one enemy and one ally, 14 m apart along +z.
fn squad() -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    let player = s.human_index().unwrap();
    let team = s.tanks[player].team;
    let enemy = s.tanks.iter().position(|t| t.team != team).unwrap();
    let ally = s
        .tanks
        .iter()
        .position(|t| !t.human && t.team == team)
        .unwrap();
    clear_arena(&mut s, &[player, enemy, ally]);
    for i in 0..s.tanks.len() {
        let z = i as f64 * 14.0;
        s.tanks[i].protection = 0.0;
        set_translation(&mut s, i, 0.0, z);
        s.tanks[i].previous = Vec2::new(0.0, z);
    }
    s.world.step();
    s.start();
    s
}

/// A standard 40-damage shell owned by the first tank of `team`, or 999 when there is none.
fn shot(s: &mut Simulation, x: f64, z: f64, vx: f64, vz: f64, team: Team) -> Shot {
    let id = s.next_id;
    s.next_id += 1;
    Shot {
        id,
        x,
        z,
        vx,
        vz,
        team,
        owner: s
            .tanks
            .iter()
            .find(|t| t.team == team)
            .map_or(999, |t| t.id),
        damage: 40.0,
        bounces: 0,
        life: 3.5,
        piercing: 0,
        weapon: Weapon::Standard,
        ..Shot::default()
    }
}

/// A standard shell fired by an absent blue shooter.
fn shell(s: &mut Simulation, x: f64, z: f64, vx: f64, vz: f64) {
    let mut p = shot(s, x, z, vx, vz, Team::Blue);
    p.owner = 999;
    p.life = 2.0;
    s.shots.push(p);
}

fn incoming(s: &mut Simulation, fired: Weapon, owner: u32, team: Team) -> Shot {
    let mut p = shot(s, -5.0, 0.0, 30.0, 0.0, team);
    p.weapon = fired;
    p.owner = owner;
    p.life = 3.0;
    p.piercing = if fired == Weapon::Piercing { 1 } else { 0 };
    p
}

fn count(s: &Simulation, kind: SimEventType) -> usize {
    s.events.iter().filter(|e| e.kind == kind).count()
}

fn shot_by_id(s: &Simulation, id: u32) -> &Shot {
    s.shots
        .iter()
        .find(|shot| shot.id == id)
        .expect("shot still in flight")
}

/// The recorded rendered-hull bounds in the tank frame: (min x, max x, min z, max z).
fn hull_bounds(kind: VehicleKind) -> (f64, f64, f64, f64) {
    let hull = tank_hull(kind);
    (
        hull.center.x - hull.size.x / 2.0,
        hull.center.x + hull.size.x / 2.0,
        hull.center.z - hull.size.z / 2.0,
        hull.center.z + hull.size.z / 2.0,
    )
}

#[test]
fn hit_boundaries_match_hull_bounds_plus_shell_radius_on_every_side_of_rotated_hulls() {
    for kind in [
        VehicleKind::Scout,
        VehicleKind::Balanced,
        VehicleKind::Heavy,
    ] {
        let (min_x, max_x, min_z, max_z) = hull_bounds(kind);
        for angle in [0.0, std::f64::consts::PI / 3.0] {
            let mut s = fixture(kind, angle);
            let (sin, cos) = angle.sin_cos();
            // Shots are authored in hull space, then rotated with the tank.
            let local_shell = |s: &mut Simulation, x: f64, z: f64, vx: f64, vz: f64| {
                shell(
                    s,
                    cos * x + sin * z,
                    -sin * x + cos * z,
                    cos * vx + sin * vz,
                    -sin * vx + cos * vz,
                )
            };
            for axis in ["x", "z"] {
                for side in [-1.0, 1.0] {
                    let bound = match (axis, side < 0.0) {
                        ("x", true) => min_x,
                        ("x", false) => max_x,
                        (_, true) => min_z,
                        (_, false) => max_z,
                    };
                    let edge = bound + side * SHELL_HIT_RADIUS;
                    for delta in [-0.001, 0.001] {
                        let offset = edge + side * delta;
                        if axis == "x" {
                            local_shell(&mut s, offset, -5.0, 0.0, 600.0);
                        } else {
                            local_shell(&mut s, -5.0, offset, 600.0, 0.0);
                        }
                        let probe = ShotProbe::from(&s.shots.pop().unwrap());
                        let hit = tank_hit_time(&s, &probe, &s.tanks[0], STEP, 0.0, 0.0, None);
                        assert_eq!(
                            hit.is_some(),
                            delta < 0.0,
                            "{kind:?} {angle} {axis} {side} {delta}"
                        );
                    }
                }
            }
            // A real shell grazing the outer tread, beyond the old narrower physics box, deals damage.
            local_shell(&mut s, max_x + SHELL_HIT_RADIUS - 0.01, -5.0, 0.0, 600.0);
            step_projectiles(&mut s, STEP, false);
            assert_eq!(
                s.tanks[0].hp,
                vehicle(kind).health - 40.0,
                "{kind:?} at {angle}"
            );
            assert_eq!(s.shots.len(), 0);
            let physical = s.world.colliders[s.tanks[0].collider]
                .shape()
                .as_cuboid()
                .expect("the hull collider is a box")
                .half_extents;
            assert!((physical.x as f64 - (max_x - min_x) / 2.0).abs() < 1e-6);
        }
    }
}

/// `tank_hit_time`'s Rapier query without its reach bound, as the reference the bound must match.
fn unbounded_hit_time(
    s: &Simulation,
    shot: &ShotProbe,
    tank: &Tank,
    limit: f64,
    elapsed: f64,
    frame_delta: f64,
) -> Option<f64> {
    let hull = tank_hull(tank.kind);
    let (size, center) = (hull.size, hull.center);
    let hit_box = Cuboid::new(vector(
        size.x / 2.0 + SHELL_HIT_RADIUS,
        0.9,
        size.z / 2.0 + SHELL_HIT_RADIUS,
    ));
    let end = s.body_translation(tank.body);
    let vx = if frame_delta > 0.0 {
        (end.x - tank.previous.x) / frame_delta
    } else {
        0.0
    };
    let vz = if frame_delta > 0.0 {
        (end.z - tank.previous.z) / frame_delta
    } else {
        0.0
    };
    let rotation = *s.world.bodies[tank.body].rotation();
    let (ry, rw) = (rotation.y as f64, rotation.w as f64);
    let cos = 1.0 - 2.0 * ry * ry;
    let sin = 2.0 * rw * ry;
    let position = vector(
        end.x - vx * (frame_delta - elapsed) + center.x * cos + center.z * sin,
        end.y,
        end.z - vz * (frame_delta - elapsed) - center.x * sin + center.z * cos,
    );
    let ray = Ray::new(
        vector(shot.x, shot.y.unwrap_or(1.0), shot.z),
        vector(shot.vx - vx, 0.0, shot.vz - vz),
    );
    let time = hit_box.cast_ray(
        &Pose::from_parts(position, rotation),
        &ray,
        limit as f32,
        true,
    )? as f64;
    (time >= 0.0 && time <= limit).then_some(time)
}

/// JavaScript `Math.sign` for finite values.
fn js_sign(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value.signum() }
}

#[test]
fn the_hull_reach_bound_skips_only_lanes_that_rapier_would_also_miss() {
    let mut s = Simulation::with_seed(123.0);
    let mut random = Random::new(7.0);
    let mut hits = 0;
    let mut misses = 0;
    for kind in VehicleKind::ALL {
        let target = s.tanks.iter().position(|tank| tank.kind == kind).unwrap();
        for sample in 0..400 {
            let x = random.range(-20.0, 20.0);
            let z = random.range(-20.0, 20.0);
            let heading = random.range(-4.0, 4.0);
            place_tank(&mut s, target, x, z, Some(heading));
            // Half the samples move the hull during the step, as projectile sweeps see it.
            let moving = sample % 2 == 1;
            let frame_delta = if moving { STEP } else { 0.0 };
            let elapsed = if moving { random.range(0.0, STEP) } else { 0.0 };
            let position = s.body_translation(s.tanks[target].body);
            if moving {
                let px = position.x - random.range(-0.3, 0.3);
                let pz = position.z - random.range(-0.3, 0.3);
                s.tanks[target].previous = Vec2::new(px, pz);
            }
            let speed = if sample % 4 < 2 {
                1.0
            } else {
                weapon(Weapon::Standard).speed
            };
            let limit = if speed == 1.0 {
                random.range(0.0, 12.0)
            } else {
                random.range(0.0, 0.4)
            };
            let (x, z, aim);
            if sample % 3 == 0 && !moving {
                // Graze just inside a corner, square to the body's radius: the farthest a hit can be.
                let hull = tank_hull(kind);
                let local_x = hull.center.x
                    + js_sign(random.range(-1.0, 1.0)) * (hull.size.x / 2.0 + SHELL_HIT_RADIUS);
                let local_z = hull.center.z
                    + js_sign(random.range(-1.0, 1.0)) * (hull.size.z / 2.0 + SHELL_HIT_RADIUS);
                let heading = s.tanks[target].heading;
                let corner_x =
                    position.x + 0.999 * (local_x * heading.cos() + local_z * heading.sin());
                let corner_z =
                    position.z + 0.999 * (-local_x * heading.sin() + local_z * heading.cos());
                aim = (corner_x - position.x).atan2(corner_z - position.z)
                    + std::f64::consts::FRAC_PI_2;
                let lead = random.range(0.01, 0.99) * limit * speed;
                x = corner_x - aim.sin() * lead;
                z = corner_z - aim.cos() * lead;
            } else {
                // Lanes start around the hull and aim near it, so many graze or narrowly miss.
                let from = random.range(0.0, std::f64::consts::PI * 2.0);
                let distance = random.range(0.0, 9.0);
                x = position.x + from.sin() * distance;
                z = position.z + from.cos() * distance;
                aim = (position.x - x).atan2(position.z - z) + random.range(-0.7, 0.7);
            }
            let probe = ShotProbe {
                x,
                y: None,
                z,
                vx: aim.sin() * speed,
                vz: aim.cos() * speed,
                ignored: None,
            };
            let tank = &s.tanks[target];
            let expected = unbounded_hit_time(&s, &probe, tank, limit, elapsed, frame_delta);
            assert_eq!(
                tank_hit_time(&s, &probe, tank, limit, elapsed, frame_delta, None),
                expected,
                "{kind:?} sample {sample}"
            );
            if expected.is_none() {
                misses += 1;
            } else {
                hits += 1;
            }
        }
    }
    assert!(hits > 300 && misses > 300, "{hits} hits, {misses} misses");
}

#[test]
fn cover_still_blocks_shots_at_the_widened_hull_and_protected_targets_do_not_lose_health() {
    let mut s = fixture(VehicleKind::Balanced, 0.0);
    s.add_cover(&CoverDef::new(
        CoverKind::Concrete,
        0.0,
        -3.8,
        5.0,
        0.25,
        3.0,
        f64::INFINITY,
        0,
    ));
    s.world.step();
    shell(
        &mut s,
        1.1 * vehicle(VehicleKind::Balanced).scale,
        -5.0,
        0.0,
        600.0,
    );
    step_projectiles(&mut s, STEP, false);
    assert_eq!(s.tanks[0].hp, 100.0);
    assert_eq!(s.shots.len(), 0);
    clear_arena(&mut s, &[0]);
    s.tanks[0].protection = 1.0;
    shell(&mut s, 0.0, -5.0, 0.0, 600.0);
    step_projectiles(&mut s, STEP, false);
    assert_eq!(s.tanks[0].hp, 100.0);
    s.tanks[0].protection = 0.0;
    s.tanks[0].shield = 10.0;
    s.tanks[0].shield_points = 40.0;
    shell(&mut s, 0.0, -5.0, 0.0, 600.0);
    step_projectiles(&mut s, STEP, false);
    assert_eq!(s.tanks[0].hp, 100.0);
    assert_eq!(s.tanks[0].shield_points, 0.0);
}

#[test]
fn moving_tanks_are_hit_at_the_crossing_time_not_just_their_end_of_tick_location() {
    for (start, end, expected) in [(-5.0, 5.0, 60.0), (-12.0, 0.0, 100.0)] {
        let mut s = fixture(VehicleKind::Balanced, 0.0);
        set_translation(&mut s, 0, 0.0, end);
        s.tanks[0].previous = Vec2::new(0.0, start);
        shell(&mut s, -5.0, 0.0, 600.0, 0.0);
        step_projectiles(&mut s, STEP, true);
        assert_eq!(s.tanks[0].hp, expected, "travel {start} to {end}");
    }
}

#[test]
fn damage_events_credit_the_owner_on_surviving_and_lethal_hits_excluding_protected_hits() {
    let mut s = fixture(VehicleKind::Balanced, 0.0);
    s.events.clear();
    let target_team = s.tanks[0].team;
    s.tanks[0].protection = 1.0;
    s.damage_tank(0, 40.0, 999, Team::Blue, None, None);
    s.tanks[0].protection = 0.0;
    s.tanks[0].shield = 10.0;
    s.tanks[0].shield_points = 40.0;
    s.damage_tank(0, 40.0, 999, Team::Blue, None, None);
    s.damage_tank(0, 40.0, 999, target_team, None, None);
    assert_eq!(s.events.len(), 0);
    s.damage_tank(0, 40.0, 999, Team::Blue, None, None);
    let last = s.events.last().unwrap();
    assert_eq!(last.kind, SimEventType::Hurt);
    assert_eq!(last.owner, Some(999));
    assert_eq!(last.team, Some(target_team));
    s.damage_tank(0, 100.0, 999, Team::Blue, None, None);
    let death = s
        .events
        .iter()
        .find(|e| e.kind == SimEventType::Death)
        .unwrap();
    assert_eq!(death.owner, Some(999));
    assert_eq!(death.id, Some(s.tanks[0].id));
}

#[test]
fn shells_and_spread_pellets_emerge_from_the_muzzle_for_all_chassis_and_aim_directions() {
    for kind in [
        VehicleKind::Scout,
        VehicleKind::Balanced,
        VehicleKind::Heavy,
    ] {
        for angle in [0.0, 1.2] {
            let mut s = Simulation::with_seed(123.0);
            clear_arena(&mut s, &[]);
            let t = s.add_tank(Team::Blue, true, kind, 0);
            place_tank(&mut s, t, 0.0, 0.0, None);
            s.tanks[t].aim = angle;
            s.tanks[t].ammo.spread = 18.0;
            s.tanks[t].selected_ammo = Weapon::Spread;
            // The model stands with its ground frame 0.25 m up and its turret turned to `angle`.
            let muzzle = tank_muzzle(kind);
            let (sin, cos) = angle.sin_cos();
            let expected = (
                muzzle.x * cos + muzzle.z * sin,
                0.25 + muzzle.y,
                -muzzle.x * sin + muzzle.z * cos,
            );
            fire_weapon(&mut s, t);
            assert_eq!(s.shots.len(), 3);
            for p in &s.shots {
                assert!(
                    (p.x - expected.0).abs() < 1e-5,
                    "{kind:?} {angle} x {}",
                    p.x
                );
                assert!(
                    (p.z - expected.2).abs() < 1e-5,
                    "{kind:?} {angle} z {}",
                    p.z
                );
                assert!(
                    (p.y.unwrap() - expected.1).abs() < 1e-5,
                    "{kind:?} {angle} y {:?}",
                    p.y
                );
            }
        }
    }
}

#[test]
fn a_protruding_barrel_cannot_spawn_shots_beyond_nearby_cover_or_an_enemy() {
    for obstruction in ["cover", "enemy"] {
        let mut s = Simulation::with_seed(123.0);
        clear_arena(&mut s, &[]);
        let t = s.add_tank(Team::Blue, true, VehicleKind::Balanced, 0);
        place_tank(&mut s, t, 0.0, 0.0, None);
        s.tanks[t].aim = 0.0;
        if obstruction == "cover" {
            s.add_cover(&CoverDef::new(
                CoverKind::Timber,
                0.0,
                1.0,
                3.0,
                0.2,
                2.0,
                100.0,
                0,
            ));
        } else {
            let target = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
            place_tank(&mut s, target, 0.0, 2.0, None);
            s.tanks[target].protection = 0.0;
        }
        s.world.step();
        set_translation(&mut s, t, 0.0, 0.0);
        if obstruction == "enemy" {
            set_translation(&mut s, 1, 0.0, 2.0);
        }
        // Rapier JS needed propagateModifiedBodyPositionsToColliders here; the muzzle trace
        // reads cover (fixed) colliders and tank bodies, which already see the new poses.
        fire_weapon(&mut s, t);
        step_projectiles(&mut s, STEP, false);
        let hp = if obstruction == "cover" {
            s.covers[0].hp
        } else {
            s.tanks[1].hp
        };
        assert_eq!(hp, 60.0, "{obstruction}");
    }
}

#[test]
fn every_weapon_hit_records_actual_impact_direction_and_death_cause() {
    for fired in [
        Weapon::Standard,
        Weapon::Spread,
        Weapon::Rocket,
        Weapon::Ricochet,
        Weapon::Piercing,
    ] {
        let mut s = squad();
        s.tanks[PLAYER].hp = 1.0;
        let (enemy_id, enemy_team) = (s.tanks[ENEMY].id, s.tanks[ENEMY].team);
        let p = incoming(&mut s, fired, enemy_id, enemy_team);
        s.shots = vec![p];
        step_projectiles(&mut s, 0.3, false);
        let player_id = s.tanks[PLAYER].id;
        let death = s
            .events
            .iter()
            .find(|e| e.kind == SimEventType::Death && e.id == Some(player_id))
            .unwrap_or_else(|| panic!("{fired:?}: no death"));
        let source = death.damage_source.unwrap();
        assert_eq!(source.cause, DamageCause::from(fired));
        assert!(
            source.origin.x < death.x,
            "{fired:?}: hit from left despite attacker standing below"
        );
        assert_eq!(death.owner, Some(enemy_id));
    }
}

#[test]
fn mines_barrels_and_shell_collisions_preserve_distinct_death_causes() {
    for cause in [
        DamageCause::Mine,
        DamageCause::Drum,
        DamageCause::Interception,
    ] {
        let mut s = squad();
        s.tanks[PLAYER].hp = 1.0;
        let (enemy_id, enemy_team) = (s.tanks[ENEMY].id, s.tanks[ENEMY].team);
        let (ally_id, ally_team) = (s.tanks[ALLY].id, s.tanks[ALLY].team);
        match cause {
            DamageCause::Mine => {
                let id = s.next_id;
                s.next_id += 1;
                s.mines.push(Mine {
                    id,
                    x: 1.0,
                    z: 0.0,
                    owner: enemy_id,
                    owner_life: None,
                    team: enemy_team,
                    arm: 0.0,
                    life: 10.0,
                    damage: Some(MINE.damage),
                });
                step_mines(&mut s, 1.0 / 60.0);
            }
            DamageCause::Drum => {
                let drum = s.add_cover(&CoverDef::new(
                    CoverKind::Drum,
                    2.0,
                    0.0,
                    1.0,
                    1.0,
                    2.0,
                    1.0,
                    0,
                ));
                s.damage_cover(drum, 2.0, enemy_id, enemy_team, None, None);
            }
            _ => {
                let mut a = incoming(&mut s, Weapon::Standard, enemy_id, enemy_team);
                (a.x, a.z, a.vx) = (-1.0, 2.8, 10.0);
                let mut b = incoming(&mut s, Weapon::Standard, ally_id, ally_team);
                (b.x, b.z, b.vx) = (1.0, 2.8, -10.0);
                s.shots = vec![a, b];
                step_projectiles(&mut s, 0.2, false);
            }
        }
        let player_id = s.tanks[PLAYER].id;
        let death = s
            .events
            .iter()
            .find(|e| e.kind == SimEventType::Death && e.id == Some(player_id))
            .unwrap_or_else(|| panic!("{cause:?}: no death"));
        assert_eq!(death.damage_source.map(|d| d.cause), Some(cause));
        assert_eq!(death.owner, Some(enemy_id), "{cause:?}");
    }
}

#[test]
fn protected_and_fully_shielded_hits_do_not_emit_hull_damage_direction() {
    let mut s = squad();
    let (enemy_id, enemy_team) = (s.tanks[ENEMY].id, s.tanks[ENEMY].team);
    let source = Some(DamageSource {
        cause: DamageCause::Standard,
        origin: Vec2::new(-1.0, 0.0),
    });
    s.tanks[PLAYER].protection = 2.0;
    s.damage_tank(PLAYER, 40.0, enemy_id, enemy_team, None, source);
    assert_eq!(s.events.len(), 0);
    s.tanks[PLAYER].protection = 0.0;
    s.tanks[PLAYER].shield = 10.0;
    s.tanks[PLAYER].shield_points = 100.0;
    s.damage_tank(PLAYER, 40.0, enemy_id, enemy_team, None, source);
    assert_eq!(s.events.len(), 0);
    assert_eq!(s.tanks[PLAYER].shield_points, 60.0);
}

#[test]
fn a_reflected_shell_points_toward_its_bounce_not_the_original_shooter() {
    let mut s = squad();
    s.add_cover(&CoverDef::new(
        CoverKind::Concrete,
        5.0,
        0.0,
        1.0,
        10.0,
        3.0,
        f64::INFINITY,
        0,
    ));
    s.world.step();
    let (enemy_id, enemy_team) = (s.tanks[ENEMY].id, s.tanks[ENEMY].team);
    let mut p = incoming(&mut s, Weapon::Ricochet, enemy_id, enemy_team);
    p.x = 3.0;
    p.bounces = 1;
    s.shots = vec![p];
    step_projectiles(&mut s, 0.4, false);
    let player_id = s.tanks[PLAYER].id;
    let hit = s
        .events
        .iter()
        .find(|e| e.kind == SimEventType::Hurt && e.id == Some(player_id))
        .expect("the reflected shell hits the player");
    let source = hit.damage_source.unwrap();
    assert_eq!(source.cause, DamageCause::Ricochet);
    assert!(source.origin.x > hit.x);
}

#[test]
fn a_ricochet_stops_harmlessly_at_its_shooter_and_teammates() {
    // Fired from the shooter's hull centre, so the outbound leg starts inside it.
    for (target, start_x) in [(PLAYER, 0.0), (ALLY, 3.0)] {
        let mut s = squad();
        let z = s.body_translation(s.tanks[target].body).z;
        s.add_cover(&CoverDef::new(
            CoverKind::Concrete,
            5.0,
            z,
            1.0,
            10.0,
            3.0,
            f64::INFINITY,
            0,
        ));
        s.world.step();
        let (player_id, player_team) = (s.tanks[PLAYER].id, s.tanks[PLAYER].team);
        let mut p = incoming(&mut s, Weapon::Ricochet, player_id, player_team);
        p.x = start_x;
        p.z = z;
        p.bounces = 1;
        s.shots = vec![p];
        let (hp, shield) = (s.tanks[target].hp, s.tanks[target].shield_points);
        step_projectiles(&mut s, 0.4, false);
        assert_eq!(count(&s, SimEventType::Ricochet), 1, "target {target}");
        assert_eq!(
            s.shots.len(),
            0,
            "the reflected shell stops at target {target}"
        );
        assert_eq!(count(&s, SimEventType::Impact), 2, "target {target}");
        assert_eq!(count(&s, SimEventType::Hurt), 0, "target {target}");
        assert_eq!(
            (s.tanks[target].hp, s.tanks[target].shield_points),
            (hp, shield)
        );
    }
}

#[test]
fn opposing_fast_shells_intercept_between_endpoints_while_allies_and_asynchronous_paths_pass() {
    let mut s = column(0);
    let a = shot(&mut s, -5.0, 0.0, 1000.0, 0.0, Team::Blue);
    let b = shot(&mut s, 5.0, 0.0, -1000.0, 0.0, Team::Red);
    s.shots = vec![a.clone(), b.clone()];
    step_projectiles(&mut s, STEP, false);
    assert_eq!(s.shots.len(), 0);
    assert_eq!(count(&s, SimEventType::Explosion), 1);
    let ally = Shot {
        team: Team::Blue,
        ..b
    };
    assert_eq!(interception_time(&a, &ally, 1.0), None);
    let crossing = shot(&mut s, 0.0, -8.0, 0.0, 100.0, Team::Red);
    let lane = shot(&mut s, -2.0, 0.0, 100.0, 0.0, Team::Blue);
    assert_eq!(interception_time(&lane, &crossing, 0.1), None);
}

#[test]
fn the_earliest_interception_consumes_each_bullet_once_independent_of_array_order() {
    for reversed in [false, true] {
        let mut s = column(0);
        let a = shot(&mut s, 0.0, 0.0, 100.0, 0.0, Team::Blue);
        let near = shot(&mut s, 2.0, 0.0, 0.0, 0.0, Team::Red);
        let far = shot(&mut s, 4.0, 0.0, 0.0, 0.0, Team::Red);
        let far_id = far.id;
        s.shots = if reversed {
            vec![far, near, a]
        } else {
            vec![a, near, far]
        };
        step_projectiles(&mut s, 0.1, false);
        let ids: Vec<u32> = s.shots.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![far_id], "reversed={reversed}");
        assert_eq!(count(&s, SimEventType::Explosion), 1);
    }
}

#[test]
fn a_wall_blocks_interception_while_a_reflected_shell_can_intercept_on_its_new_path() {
    let mut s = column(0);
    s.add_cover(&CoverDef::new(
        CoverKind::Concrete,
        0.0,
        0.0,
        0.5,
        8.0,
        3.0,
        f64::INFINITY,
        0,
    ));
    s.world.step();
    let a = shot(&mut s, -2.0, 0.0, 100.0, 0.0, Team::Blue);
    let b = shot(&mut s, 2.0, 0.0, -100.0, 0.0, Team::Red);
    s.shots = vec![a, b];
    step_projectiles(&mut s, 0.1, false);
    assert_eq!(count(&s, SimEventType::Explosion), 0);
    assert_eq!(s.shots.len(), 0);
    s.events.clear();
    let mut ricochet = shot(&mut s, -1.0, 0.0, 60.0, 0.0, Team::Blue);
    ricochet.weapon = Weapon::Ricochet;
    ricochet.bounces = 1;
    let chaser = shot(&mut s, -3.0, 0.0, 60.0, 0.0, Team::Red);
    s.shots = vec![ricochet, chaser];
    step_projectiles(&mut s, 0.05, false);
    assert_eq!(count(&s, SimEventType::Ricochet), 1);
    assert_eq!(count(&s, SimEventType::Explosion), 1);
    assert_eq!(s.shots.len(), 0);
}

#[test]
fn earlier_tank_impacts_and_lifetime_expiry_take_precedence_over_later_interception() {
    let mut s = column(1);
    let a = shot(&mut s, -4.0, 0.0, 100.0, 0.0, Team::Red);
    let b = shot(&mut s, 4.0, 0.0, -10.0, 0.0, Team::Blue);
    s.shots = vec![a, b];
    let hp = s.human().hp;
    step_projectiles(&mut s, 0.1, false);
    assert_eq!(s.human().hp, hp - 40.0);
    assert_eq!(s.shots.len(), 1);
    let mut expiring = shot(&mut s, -4.0, 5.0, 100.0, 0.0, Team::Blue);
    expiring.life = 0.001;
    let opposing = shot(&mut s, 4.0, 5.0, -100.0, 0.0, Team::Red);
    s.shots = vec![expiring, opposing];
    s.events.clear();
    step_projectiles(&mut s, 0.1, false);
    assert_eq!(s.shots.len(), 1);
    assert_eq!(count(&s, SimEventType::Explosion), 0);
}

#[test]
fn interception_blast_hurts_both_teams_once_and_credits_the_opposing_shooter() {
    let mut s = column(2);
    for i in 0..2 {
        set_translation(&mut s, i, 0.0, if i == 1 { 1.8 } else { -1.8 });
        s.tanks[i].hp = 40.0;
    }
    s.world.step();
    let a = shot(&mut s, -3.0, 0.0, 100.0, 0.0, Team::Blue);
    let b = shot(&mut s, 3.0, 0.0, -100.0, 0.0, Team::Red);
    s.shots = vec![a, b];
    step_projectiles(&mut s, 0.05, false);
    assert!(s.tanks.iter().all(|t| !t.alive));
    assert!(s.tanks.iter().all(|t| t.kills == 1));
    assert_eq!(s.match_state.scores, [1, 1]);
}

#[test]
fn rockets_accelerate_along_their_flight_path_reach_a_cap_and_outpace_their_launch_speed() {
    let mut s = column(0);
    let base = weapon(Weapon::Rocket).speed;
    let standard_speed = weapon(Weapon::Standard).speed;
    let mut rocket = shot(&mut s, 0.0, 0.0, base * 0.6, base * 0.8, Team::Blue);
    rocket.weapon = Weapon::Rocket;
    let standard = shot(&mut s, 0.0, 10.0, standard_speed, 0.0, Team::Blue);
    let (rocket_id, standard_id) = (rocket.id, standard.id);
    s.shots = vec![rocket, standard];
    let mut previous = base;
    for _ in 0..60 {
        step_projectiles(&mut s, STEP, false);
        let rocket = shot_by_id(&s, rocket_id);
        let speed = rocket.vx.hypot(rocket.vz);
        assert!(speed > previous - 1e-9);
        assert!((rocket.vx / rocket.vz - 0.75).abs() < 1e-9);
        previous = speed;
    }
    assert!((previous - base * 2.5).abs() < 1e-9);
    let rocket = shot_by_id(&s, rocket_id);
    assert!(rocket.x.hypot(rocket.z) > base * 1.7);
    assert_eq!(shot_by_id(&s, standard_id).vx, standard_speed);
    for _ in 0..60 {
        step_projectiles(&mut s, STEP, false);
    }
    let rocket = shot_by_id(&s, rocket_id);
    assert!((rocket.vx.hypot(rocket.vz) - base * 2.5).abs() < 1e-9);
}

#[test]
fn accelerated_rockets_still_hit_thin_cover_and_intercept_crossing_enemy_shells() {
    for obstacle in ["wall", "shell"] {
        let mut s = column(0);
        let mut rocket = shot(
            &mut s,
            -0.4,
            0.0,
            weapon(Weapon::Rocket).speed * 2.49,
            0.0,
            Team::Blue,
        );
        rocket.weapon = Weapon::Rocket;
        s.shots = vec![rocket];
        if obstacle == "wall" {
            s.add_cover(&CoverDef::new(
                CoverKind::Concrete,
                0.0,
                0.0,
                0.1,
                4.0,
                3.0,
                f64::INFINITY,
                0,
            ));
            s.world.step();
        } else {
            let enemy = shot(
                &mut s,
                0.7,
                0.0,
                -weapon(Weapon::Standard).speed,
                0.0,
                Team::Red,
            );
            s.shots.push(enemy);
        }
        step_projectiles(&mut s, STEP, false);
        assert_eq!(s.shots.len(), 0, "{obstacle}");
        assert_eq!(count(&s, SimEventType::Explosion), 1, "{obstacle}");
        if obstacle == "wall" {
            let explosion = s
                .events
                .iter()
                .find(|e| e.kind == SimEventType::Explosion)
                .unwrap();
            assert!(explosion.x < 0.0);
        }
    }
}
