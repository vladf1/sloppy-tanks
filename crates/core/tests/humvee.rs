//! The TOW HMMWV: team-only roster, unlimited guided TOWs, forward-only driving, no shots at
//! cover or hidden tanks, debris avoidance, and its attack/withdraw tactics (the simulation
//! parts of the former `tests/humvee.test.ts`).

mod support;

use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder};
use sloppy_core::sim::ai::bot_command;
use sloppy_core::sim::ammunition::{can_collect_ammo, equipped_weapon};
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::bot_movement::steer_bot;
use sloppy_core::sim::bot_personalities::preferred_ammo;
use sloppy_core::sim::combat_rules::COMBAT;
use sloppy_core::sim::data::{STEP, group, vehicle, weapon};
use sloppy_core::sim::humvee_tactics::{
    HUMVEE_DEPARTURE_SECONDS, HumveePhase, HumveeTactics, humvee_holding_position,
    steady_humvee_shot, update_humvee_goal, withdraw_humvee,
};
use sloppy_core::sim::math::{Vec2, angle_delta, distance};
use sloppy_core::sim::physics::{interaction_groups, vector};
use sloppy_core::sim::projectiles::step_projectiles;
use sloppy_core::sim::tank_driving::drive_tank;
use sloppy_core::sim::weapons::{collect_pickup, fire_weapon};
use sloppy_core::sim::{
    BotMode, CoverKind, GameMode, Pickup, PickupKind, SimEventType, Simulation, SpecialAmmo,
    VehicleCommand, VehicleKind, Weapon,
};
use support::clear_arena;

struct Duel {
    simulation: Simulation,
    hunter: usize,
    target: usize,
}

fn humvee_index(simulation: &Simulation) -> usize {
    simulation
        .tanks
        .iter()
        .position(|tank| tank.kind == VehicleKind::Humvee)
        .expect("a team round has a HMMWV")
}

fn set_translation(simulation: &mut Simulation, index: usize, x: f64, z: f64) {
    let body = simulation.tanks[index].body;
    simulation.world.bodies[body].set_translation(vector(x, 0.65, z), true);
}

fn position(simulation: &Simulation, index: usize) -> Vec2 {
    simulation
        .body_translation(simulation.tanks[index].body)
        .planar()
}

fn tactics(simulation: &Simulation, index: usize) -> &HumveeTactics {
    simulation.tanks[index]
        .brain
        .humvee
        .as_ref()
        .expect("humvee tactics")
}

/// A HMMWV at the origin facing north and one enemy `range` metres ahead on an otherwise
/// empty map. Cover is added before navigation is rebuilt.
fn duel(range: f64, covers: &[CoverDef]) -> Duel {
    let mut simulation = Simulation::with_seed(123.0);
    let hunter = humvee_index(&simulation);
    let hunter_team = simulation.tanks[hunter].team;
    let target = simulation
        .tanks
        .iter()
        .position(|tank| tank.team != hunter_team)
        .expect("enemy");
    clear_arena(&mut simulation, &[hunter, target]);
    let (hunter, target) = (0, 1);
    for cover in covers {
        simulation.add_cover(cover);
    }
    simulation.nav.rebuild(&simulation.covers, None);
    set_translation(&mut simulation, hunter, 0.0, 0.0);
    set_translation(&mut simulation, target, 0.0, range);
    simulation.tanks[hunter].previous = Vec2::new(0.0, 0.0);
    simulation.tanks[target].previous = Vec2::new(0.0, range);
    simulation.tanks[hunter].aim = 0.0;
    simulation.tanks[hunter].heading = 0.0;
    simulation.tanks[hunter].cooldown = 0.0;
    simulation.tanks[hunter].protection = 0.0;
    simulation.tanks[target].protection = 0.0;
    simulation.world.step();
    Duel {
        simulation,
        hunter,
        target,
    }
}

fn retreat_command() -> VehicleCommand {
    VehicleCommand {
        move_z: -1.0,
        ..VehicleCommand::idle()
    }
}

fn drive(simulation: &mut Simulation, index: usize, command: &VehicleCommand) {
    let tank = &mut simulation.tanks[index];
    let body = &mut simulation.world.bodies[tank.body];
    drive_tank(tank, body, command, STEP, 1.0);
}

#[test]
fn team_rounds_include_tow_hmmwvs_while_solo_rounds_do_not() {
    let team = Simulation::with_seed(123.0);
    let hunters: Vec<_> = team
        .tanks
        .iter()
        .filter(|tank| tank.kind == VehicleKind::Humvee)
        .collect();
    assert_eq!(hunters.len(), 2);
    assert!(hunters.iter().all(|tank| !tank.human));
    assert!(
        hunters
            .iter()
            .all(|tank| equipped_weapon(tank) == Weapon::Tow)
    );
    assert!(
        hunters
            .iter()
            .all(|tank| preferred_ammo(tank) == Weapon::Tow)
    );

    let mut solo = Simulation::with_seed(123.0);
    solo.game_mode = GameMode::Solo;
    solo.reset(None);
    assert_eq!(
        solo.tanks
            .iter()
            .filter(|tank| tank.kind == VehicleKind::Humvee)
            .count(),
        0
    );
}

#[test]
fn a_hmmwv_fires_an_unlimited_tow_that_leaves_a_fresh_bruiser_alive() {
    let Duel {
        mut simulation,
        hunter,
        target,
    } = duel(12.0, &[]);
    simulation.tanks[hunter].brain.target = simulation.tanks[target].id;
    simulation.start();
    fire_weapon(&mut simulation, hunter);
    let shot = &simulation.shots[0];
    assert_eq!(shot.weapon, Weapon::Tow);
    assert_eq!(shot.damage, weapon(Weapon::Tow).damage);
    assert!(shot.visual_y.unwrap_or(0.0) > shot.y.unwrap_or(0.0));
    assert_eq!(shot.target_id, Some(simulation.tanks[target].id));
    assert_eq!(simulation.tanks[hunter].ammo.rocket, 0.0);
    for _ in 0..40 {
        step_projectiles(&mut simulation, STEP, true);
    }
    let target_tank = &simulation.tanks[target];
    assert!(target_tank.alive);
    assert_eq!(
        target_tank.hp,
        vehicle(target_tank.kind).health - weapon(Weapon::Tow).damage
    );
    assert!(
        !simulation
            .events
            .iter()
            .any(|event| event.kind == SimEventType::Explosion)
    );
}

#[test]
fn hmmwvs_drive_forward_except_during_recovery_and_guide_tows_with_a_capped_turn() {
    let Duel {
        mut simulation,
        hunter,
        target,
    } = duel(24.0, &[]);
    simulation.tanks[hunter].brain.target = simulation.tanks[target].id;
    simulation.start();
    drive(&mut simulation, hunter, &retreat_command());
    let body = simulation.tanks[hunter].body;
    assert!(
        simulation.body_linvel(body).z >= 0.0,
        "normal retreat never drives backward"
    );
    assert_ne!(
        simulation.tanks[hunter].heading, 0.0,
        "normal retreat starts a forward-facing pivot"
    );
    simulation.tanks[hunter].brain.recovery = 1.0;
    simulation.world.bodies[body].set_linvel(vector(0.0, 0.0, 0.0), true);
    drive(&mut simulation, hunter, &retreat_command());
    assert!(
        simulation.body_linvel(body).z < 0.0,
        "stuck recovery may use reverse gear"
    );
    simulation.tanks[hunter].brain.recovery = 0.0;

    fire_weapon(&mut simulation, hunter);
    let shot = &simulation.shots[0];
    let before = shot.vx.atan2(shot.vz);
    set_translation(&mut simulation, target, 12.0, 24.0);
    step_projectiles(&mut simulation, 0.1, false);
    let shot = &simulation.shots[0];
    let after = shot.vx.atan2(shot.vz);
    assert!(angle_delta(before, after).abs() <= COMBAT.tow_turn_rate * 0.1 + 1e-9);
    assert!(shot.vx > 0.0, "the TOW bends toward the marked target");
}

#[test]
fn hmmwvs_never_spend_tows_on_cover_or_hidden_tanks() {
    for kind in [CoverKind::Tree, CoverKind::Timber] {
        let height = if kind == CoverKind::Tree { 6.0 } else { 2.0 };
        let Duel {
            mut simulation,
            hunter,
            target,
        } = duel(
            10.0,
            &[CoverDef::new(kind, 0.0, 5.0, 2.0, 2.0, height, 80.0, 0)],
        );
        simulation.tanks[hunter].brain.target = simulation.tanks[target].id;
        simulation.tanks[hunter].brain.memory = 1.0;
        assert!(!simulation.visible(position(&simulation, hunter), position(&simulation, target)));
        fire_weapon(&mut simulation, hunter);
        assert_eq!(
            simulation.shots.len(),
            0,
            "{kind:?}: hidden tank must not attract a TOW"
        );

        let brain = &mut simulation.tanks[hunter].brain;
        brain.target = 0;
        brain.memory = 0.0;
        brain.decision = 999.0;
        brain.path = Vec::new();
        brain.goal = Vec2::new(0.0, 10.0);
        let command = bot_command(&mut simulation, hunter, STEP);
        assert!(
            !command.fire,
            "{kind:?}: HMMWV must not use TOWs as breach shots"
        );
    }
}

#[test]
fn hmmwvs_steer_around_substantial_debris_instead_of_driving_through_it() {
    let mut simulation = Simulation::with_seed(123.0);
    let hunter = humvee_index(&simulation);
    clear_arena(&mut simulation, &[hunter]);
    let hunter = 0;
    set_translation(&mut simulation, hunter, 0.0, -5.0);
    simulation.tanks[hunter].previous = Vec2::new(0.0, -5.0);
    simulation.tanks[hunter].heading = 0.0;
    let debris = simulation
        .world
        .insert_body(RigidBodyBuilder::fixed().translation(vector(0.0, 0.65, -2.5)));
    simulation.world.insert_collider(
        ColliderBuilder::cuboid(1.1, 0.6, 1.1)
            .collision_groups(interaction_groups(group::PUSHABLE_DEBRIS))
            .mass(6.0),
        Some(debris),
    );
    simulation.world.step();

    let steer = steer_bot(&mut simulation, hunter, Vec2::new(0.0, 1.0), STEP);
    assert!(
        steer.x.abs() > 0.1 || steer.z < 0.5,
        "HMMWV must choose an open side"
    );
}

#[test]
fn a_rejected_tow_launch_preserves_protection_reload_recoil_and_combat_time() {
    let mut simulation = Simulation::with_seed(123.0);
    let hunter = humvee_index(&simulation);
    simulation.tanks[hunter].brain.target = 0;
    let state = |simulation: &Simulation| {
        let tank = &simulation.tanks[hunter];
        [
            tank.protection,
            tank.cooldown,
            tank.recoil,
            tank.last_combat,
        ]
    };
    let before = state(&simulation);
    simulation.elapsed = 10.0;
    fire_weapon(&mut simulation, hunter);
    assert_eq!(simulation.shots.len(), 0);
    assert_eq!(state(&simulation), before);
}

#[test]
fn hmmwvs_leave_unusable_ammo_crates_for_other_vehicles() {
    let mut simulation = Simulation::with_seed(123.0);
    let hunter = humvee_index(&simulation);
    let mut pickup = Pickup {
        id: 999,
        kind: PickupKind::Ricochet,
        x: 0.0,
        z: 0.0,
        available: true,
        cooldown: 0.0,
        cooldown_duration: 0.0,
    };
    assert!(!can_collect_ammo(
        &simulation.tanks[hunter],
        SpecialAmmo::Ricochet,
        1.0
    ));
    assert!(!collect_pickup(&mut simulation, hunter, &mut pickup));
    assert!(pickup.available);
    assert_eq!(simulation.tanks[hunter].ammo.ricochet, 0.0);
}

#[test]
fn tow_guidance_cannot_transfer_to_a_new_life_of_the_marked_target() {
    let Duel {
        mut simulation,
        hunter,
        target,
    } = duel(24.0, &[]);
    simulation.tanks[hunter].brain.target = simulation.tanks[target].id;
    fire_weapon(&mut simulation, hunter);
    assert!(!simulation.shots.is_empty());
    simulation.tanks[target].life += 1;
    set_translation(&mut simulation, target, 12.0, 24.0);
    step_projectiles(&mut simulation, STEP, false);
    let shot = &simulation.shots[0];
    assert_eq!(shot.vx, 0.0);
    assert_eq!(shot.target_id, None);
}

#[test]
fn humvees_plan_an_escape_fire_once_withdraw_and_wait_before_attacking_again() {
    let Duel {
        mut simulation,
        hunter,
        target,
    } = duel(28.0, &[]);
    let target_id = simulation.tanks[target].id;
    let brain = &mut simulation.tanks[hunter].brain;
    brain.target = target_id;
    brain.memory = 5.0;
    brain.last_seen = Vec2::new(0.0, 28.0);
    brain.decision = 0.0;
    brain.reaction = 0.0;
    bot_command(&mut simulation, hunter, STEP);
    assert_eq!(tactics(&simulation, hunter).phase, HumveePhase::Attack);
    assert!(
        distance(
            tactics(&simulation, hunter).escape,
            position(&simulation, target)
        ) > 28.0
    );
    let firing_point = simulation.tanks[hunter].brain.goal;
    simulation.tanks[hunter].brain.decision = 0.0;
    bot_command(&mut simulation, hunter, STEP);
    assert_eq!(
        simulation.tanks[hunter].brain.goal, firing_point,
        "target acquisition must not overwrite the firing position"
    );
    fire_weapon(&mut simulation, hunter);
    assert_eq!(tactics(&simulation, hunter).phase, HumveePhase::Withdraw);
    assert_eq!(
        simulation.tanks[hunter].brain.goal,
        tactics(&simulation, hunter).escape
    );
    assert!(tactics(&simulation, hunter).ready_at > simulation.tanks[hunter].cooldown);
    simulation.tanks[hunter].cooldown = 0.0;
    simulation.tanks[hunter].brain.fire_delay = 0.0;
    simulation.tanks[hunter].brain.decision = 0.0;
    assert!(!bot_command(&mut simulation, hunter, STEP).fire);
    let escape = tactics(&simulation, hunter).escape;
    set_translation(&mut simulation, hunter, escape.x, escape.z);
    simulation.tanks[hunter].brain.decision = 0.0;
    bot_command(&mut simulation, hunter, STEP);
    assert_ne!(
        tactics(&simulation, hunter).phase,
        HumveePhase::Attack,
        "arrival does not bypass the withdrawal pause"
    );
    let plan = tactics(&simulation, hunter);
    simulation.elapsed = plan.ready_at.max(plan.deadline) + 0.1;
    simulation.tanks[hunter].brain.decision = 0.0;
    bot_command(&mut simulation, hunter, STEP);
    assert_eq!(
        tactics(&simulation, hunter).phase,
        HumveePhase::Attack,
        "open terrain withdrawal remains bounded"
    );
    let last_shot = tactics(&simulation, hunter).last_shot.expect("last shot");
    assert!(
        distance(simulation.tanks[hunter].brain.goal, last_shot) >= 5.0,
        "next attack uses a different position"
    );
}

#[test]
fn humvees_prefer_a_concealed_escape_and_do_not_escort_idle_allies() {
    let Duel {
        mut simulation,
        hunter,
        target,
    } = duel(
        28.0,
        &[CoverDef::new(
            CoverKind::Concrete,
            7.0,
            3.0,
            5.0,
            2.0,
            3.0,
            f64::INFINITY,
            0,
        )],
    );
    simulation.tanks[hunter].brain.decision = 0.0;
    bot_command(&mut simulation, hunter, STEP);
    let escape = tactics(&simulation, hunter).escape;
    assert_eq!(tactics(&simulation, hunter).phase, HumveePhase::Attack);
    assert!(!simulation.visible(escape, position(&simulation, target)));
    assert!(
        simulation
            .nav
            .clear_line(simulation.tanks[hunter].brain.goal, escape)
    );
    simulation.tanks[target].team = simulation.tanks[hunter].team;
    let brain = &mut simulation.tanks[hunter].brain;
    brain.target = 0;
    brain.memory = 0.0;
    brain.decision = 0.0;
    bot_command(&mut simulation, hunter, STEP);
    assert_ne!(simulation.tanks[hunter].brain.mode, BotMode::Escort);
}

#[test]
fn humvees_must_settle_to_fire_and_remain_exposed_briefly_after_launch() {
    let mut simulation = Simulation::with_seed(123.0);
    let hunter = humvee_index(&simulation);
    update_humvee_goal(&mut simulation, hunter);
    simulation.tanks[hunter].brain.target = 1234;
    for _ in 0..30 {
        assert!(!steady_humvee_shot(&mut simulation, hunter, true, STEP));
    }
    assert!(humvee_holding_position(&simulation, hunter));
    assert!(!steady_humvee_shot(&mut simulation, hunter, false, STEP));
    assert!(!humvee_holding_position(&simulation, hunter));
    for _ in 0..60 {
        steady_humvee_shot(&mut simulation, hunter, true, STEP);
    }
    assert!(steady_humvee_shot(&mut simulation, hunter, true, STEP));
    withdraw_humvee(&mut simulation, hunter);
    assert!(humvee_holding_position(&simulation, hunter));
    simulation.elapsed += HUMVEE_DEPARTURE_SECONDS + STEP;
    assert!(!humvee_holding_position(&simulation, hunter));
}
