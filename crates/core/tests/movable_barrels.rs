//! Movable barrels (the former `tests/movable-barrels.test.ts`): every chassis pushes and can
//! roll a drum without damage, displaced drums keep navigation current and chain-explode where
//! they lie, and destroying a drum clears its last baked footprint.

mod support;

use std::f64::consts::FRAC_1_SQRT_2;

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::{STEP, group};
use sloppy_core::sim::debris_physics::blast_debris;
use sloppy_core::sim::math::{Quat4, Vec2};
use sloppy_core::sim::physics::{to_rotation, vector};
use sloppy_core::sim::projectiles::step_projectiles;
use sloppy_core::sim::simulation::packed_groups;
use sloppy_core::sim::{
    CoverKind, FragmentShape, Shot, SimEventType, Simulation, Team, VehicleCommand, VehicleKind,
};
use support::{clear_arena, cover_at};

fn arena() -> Simulation {
    let mut sim = Simulation::with_seed(123.0);
    clear_arena(&mut sim, &[]);
    sim.start();
    sim
}

fn barrel(sim: &mut Simulation, x: f64, z: f64) -> usize {
    sim.add_cover(&CoverDef::new(
        CoverKind::Drum,
        x,
        z,
        1.2,
        1.2,
        1.7,
        30.0,
        0xff5b24,
    ))
}

#[test]
fn every_tank_can_push_barrels_and_hard_shoves_can_tip_and_roll_them_before_they_settle_and_wake() {
    let mut rolling_cases = 0;
    for kind in VehicleKind::PLAYABLE {
        let mut sim = arena();
        let drum = barrel(&mut sim, 0.0, 0.0);
        let tank = sim.add_tank(Team::Blue, true, kind, 0);
        sim.tanks[tank].heading = 0.0;
        sim.tanks[tank].protection = 0.0;
        let tank_body = sim.tanks[tank].body;
        sim.world.bodies[tank_body].set_translation(vector(0.0, 0.65, -5.0), true);
        let hp = sim.tanks[tank].hp;
        let drum_body = sim.covers[drum].body;
        let mut tipped = false;
        let mut rolled = false;
        let steps = 120 + (20.0 / STEP) as usize;
        for i in 0..steps {
            let command = if i < 120 {
                VehicleCommand {
                    move_z: 1.0,
                    ..VehicleCommand::idle()
                }
            } else {
                VehicleCommand::idle()
            };
            sim.step(command, false);
            let q = sim.body_rotation(drum_body);
            let sideways = (1.0 - 2.0 * (q.x * q.x + q.z * q.z)).abs() < 0.4;
            tipped |= sideways;
            let v = sim.body_linvel(drum_body);
            let spin = sim.world.bodies[drum_body].angvel();
            rolled |=
                sideways && v.x.hypot(v.z) > 0.5 && (spin.x as f64).hypot(spin.z as f64) > 0.5;
        }
        assert!(
            sim.covers[drum].z > 3.0,
            "{kind:?} pushes the barrel: {}",
            sim.covers[drum].z
        );
        if tipped && rolled {
            rolling_cases += 1;
        }
        assert_eq!(sim.covers[drum].hp, 30.0);
        assert_eq!(sim.tanks[tank].hp, hp);
        assert!(
            sim.world.bodies[drum_body].is_sleeping(),
            "{kind:?}: barrel settles"
        );
        assert_eq!(sim.world.bodies[drum_body].colliders().len(), 1);
        let collider = sim.covers[drum].collider;
        assert_eq!(
            packed_groups(sim.world.colliders[collider].collision_groups()),
            group::MOVABLE_COVER
        );
        let origin = Vec2::new(sim.covers[drum].x - 1.0, sim.covers[drum].z);
        blast_debris(&mut sim, origin, 4.0, 15.0);
        assert!(!sim.world.bodies[drum_body].is_sleeping());
        let v = sim.body_linvel(drum_body);
        assert!((v.x * v.x + v.y * v.y + v.z * v.z).sqrt() > 0.5);
        assert_eq!(sim.covers[drum].hp, 30.0);
    }
    assert!(
        rolling_cases >= 2,
        "faster shoves tip barrels into a physical roll: {rolling_cases}"
    );
}

#[test]
fn a_displaced_tipped_barrel_updates_navigation_and_can_be_shot_to_chain_explode_at_its_new_position()
 {
    let mut sim = arena();
    let drum = barrel(&mut sim, 0.0, 0.0);
    sim.nav.rebuild(&sim.covers, None);
    let old = sim.nav.index(Vec2::ZERO);
    assert_eq!(sim.nav.blocked[old], 1);
    let drum_body = sim.covers[drum].body;
    sim.world.bodies[drum_body].set_translation(vector(12.0, 0.61, 0.0), true);
    // Lay its cylinder axis along X.
    sim.world.bodies[drum_body].set_rotation(
        to_rotation(Quat4 {
            x: 0.0,
            y: 0.0,
            z: FRAC_1_SQRT_2,
            w: FRAC_1_SQRT_2,
        }),
        true,
    );
    barrel(&mut sim, 15.0, 0.0);
    sim.step(VehicleCommand::idle(), false);
    assert_eq!(sim.nav.blocked[old], 0);
    assert!(sim.nav.is_blocked(cover_at(&sim, drum)));
    let position = sim.body_translation(drum_body);
    let handle = sim.covers[drum].collider;
    let id = sim.next_id;
    sim.next_id += 1;
    sim.shots.push(Shot {
        id,
        owner: 999,
        team: Team::Blue,
        x: 8.0,
        z: 0.0,
        y: Some(1.0),
        vx: 25.0,
        vz: 0.0,
        damage: 40.0,
        life: 2.0,
        ..Shot::default()
    });
    step_projectiles(&mut sim, 0.2, false);
    assert!(!sim.covers[drum].alive);
    assert!(!sim.world.bodies.contains(drum_body));
    assert!(!sim.cover_by_collider.contains_key(&handle));
    assert_eq!(sim.destroyed, 2);
    let explosions: Vec<_> = sim
        .events
        .iter()
        .filter(|e| e.kind == SimEventType::Explosion && e.cover_kind == Some(CoverKind::Drum))
        .collect();
    assert_eq!(explosions.len(), 2);
    assert!((explosions[0].x - position.x).abs() < 0.01);
    assert!(explosions.iter().all(|e| e.x > 10.0));
    let lid = sim
        .fragments
        .iter()
        .find(|f| f.shape == Some(FragmentShape::DrumLid))
        .unwrap();
    assert!(
        (lid.dimensions.unwrap().x - 0.78).abs() < 0.001,
        "navigation bounds never inflate the lid"
    );
    assert!(
        (sim.body_translation(lid.body).x - (position.x - 0.85)).abs() < 0.02,
        "debris follows the tipped body pose"
    );
    for _ in 0..120 {
        sim.step(VehicleCommand::idle(), false);
    }
}

#[test]
fn destroying_a_barrel_between_navigation_updates_clears_its_old_footprint() {
    let mut sim = arena();
    let drum = barrel(&mut sim, 0.0, 0.0);
    sim.nav.rebuild(&sim.covers, None);
    let old = sim.nav.index(cover_at(&sim, drum));
    let drum_body = sim.covers[drum].body;
    sim.world.bodies[drum_body].set_translation(vector(8.0, 0.85, 0.0), true);
    // Destroy before update_movable_cover has synchronized its location or navigation.
    sim.damage_cover(drum, 40.0, 999, Team::Blue, None, None);
    assert_eq!(sim.nav.blocked[old], 0);
    let explosion = sim
        .events
        .iter()
        .find(|e| e.kind == SimEventType::Explosion)
        .unwrap();
    assert_eq!(explosion.x, 8.0);
}
