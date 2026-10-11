//! The Cybertruck easter egg, on debug single-player pages only: a few neutral
//! trucks roam the arena as targets. They drive between reachable spots, never fire
//! or lay mines, bolt away from whoever shoots them, and come back somewhere else a
//! while after they are destroyed. Bots ignore them and they earn no experience,
//! kills or score (`VehicleKind::neutral`).
//!
//! A shell that strikes near a wheel can knock it off: the wheel rolls away on its
//! own physics while the truck careens toward the bare corner, then rolls over onto
//! it and is destroyed.
//!
//! Nothing here draws from the combat RNG: every choice hashes the truck and the
//! match clock, so a seeded match only differs while the trucks are on.

use std::f64::consts::FRAC_PI_2;

use glam::{DQuat, DVec3};
use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder, Vector};

use super::bot_movement::{recover_bot, route_direction, steer_bot};
use super::data::{STEP, group, vehicle};
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::math::{Quat4, Vec2, distance};
use super::physics::{interaction_groups, to_rotation, vector};
use super::simulation::Simulation;
use super::tank_destruction::HumveeTumble;
use super::types::{
    Brain, DamageCause, DamageSource, Driver, Fragment, Tank, Team, VehicleCommand, VehicleKind,
    WreckPart,
};
use crate::models::{CYBERTRUCK_WHEEL_RADIUS, CYBERTRUCK_WHEEL_WIDTH, CYBERTRUCK_WHEELS};

/// Trucks a round starts with; every `GOLD_EVERY`th is wrapped in gold foil.
pub const CYBERTRUCK_COUNT: usize = 6;
pub const GOLD_EVERY: usize = 3;
/// A gold truck's blast when destroyed: a little bigger than a fuel drum's.
pub const GOLD_BLAST_RADIUS: f64 = 7.5;
pub const GOLD_BLAST_DAMAGE: f64 = 70.0;
/// Seconds a destroyed truck stays gone.
pub const CYBERTRUCK_RESPAWN_SECONDS: f64 = 6.0;
/// Throttle while cruising, and flat out while fleeing.
const CRUISE: f64 = 0.65;
const FLEE: f64 = 1.0;
/// A goal this close counts as reached; one not reached in this long is dropped.
const ARRIVED: f64 = 4.0;
const GOAL_SECONDS: f64 = 14.0;
/// A cruising truck heads for spots at least this far away.
const MIN_LEG: f64 = 14.0;
/// A shell striking within this planar distance of a wheel's hub may knock it off,
/// one time in `WHEEL_LOSS_ODDS`.
const WHEEL_HIT_REACH: f64 = 0.9;
const WHEEL_LOSS_ODDS: u32 = 2;
/// Seconds a truck careens on three wheels before it rolls over.
const CAREEN_SECONDS: f64 = 1.5;
/// How far off its heading a careening truck drags toward the bare corner, radians.
const CAREEN_VEER: f64 = 1.1;
/// A loose wheel's launch, on top of the truck's own motion: speeds (m/s) out from
/// the flank, fore or aft with its axle, along the truck with the shell, and up.
const WHEEL_KICK: [f64; 4] = [4.5, 1.5, 2.0, 3.0];
/// It leans out this far (radians) as it goes, so it rolls off in a curve.
const WHEEL_LEAN: f64 = 0.12;
/// Its linear and angular damping: rolling resistance that slows it within its life.
const WHEEL_DRAG: [f64; 2] = [0.3, 0.02];
/// A tyre's crown: the share of its half-width rounded off at each shoulder.
const WHEEL_CROWN: f64 = 0.9;
/// Below this planar speed (m/s) a rolling wheel wobbles over: it is tipped toward the
/// side it already leans to at this rate (radians per second) until it leans past
/// `TIPPED_AXLE` (the sine of its axle's rise), and gravity lays it down.
const TOPPLE_SPEED: f64 = 1.3;
const TOPPLE_RATE: f64 = 1.2;
const TIPPED_AXLE: f64 = 0.3;
const WHEEL_MASS: f64 = 30.0;
/// Seconds a loose wheel lasts before it sinks away: it rolls, wobbles over and
/// lies still a moment.
const WHEEL_SECONDS: f64 = 11.0;
/// Planar speed below which a loose wheel just topples instead of rolling.
const ROLLING_SPEED: f64 = 0.5;
/// Chance, one in this many, that a plain truck's crash only burns out.
const QUIET_CRASH_ODDS: u32 = 2;

/// A Cybertruck careening on three wheels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Careen {
    /// The wheel it lost (`CYBERTRUCK_WHEELS` order).
    pub wheel: usize,
    /// Seconds until it rolls over.
    pub seconds: f64,
    /// Whoever shot the wheel off owns the crash.
    pub owner: u32,
    pub team: Team,
    pub owner_life: Option<u32>,
}

/// A cheap integer hash, for choices that must not touch the combat RNG.
fn mix(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^ (x >> 12)
}

/// Spots a truck may head for or appear at: the pickup places, which every map
/// keeps reachable.
fn spots(simulation: &Simulation) -> Vec<Vec2> {
    simulation
        .pickups
        .iter()
        .map(|pickup| Vec2::new(pickup.x, pickup.z))
        .collect()
}

/// Room a truck needs clear of every other vehicle where it appears.
const SPAWN_CLEARANCE: f64 = 6.0;

/// Where truck `salt` appears: one of the spots in the half of the map farther from
/// the player, clear of every vehicle; with every spot taken, the roomiest.
pub(crate) fn cybertruck_spawn(simulation: &Simulation, salt: u32) -> Vec2 {
    // A dead player has no body; it waits where it fell.
    let player = simulation
        .tanks
        .iter()
        .find(|tank| tank.human)
        .map_or(Vec2::ZERO, |tank| simulation.tank_position(tank).planar());
    let vehicles: Vec<Vec2> = simulation
        .tanks
        .iter()
        .filter(|tank| tank.alive)
        .map(|tank| simulation.body_translation(tank.body).planar())
        .collect();
    let room = |spot: Vec2| {
        vehicles
            .iter()
            .map(|&at| distance(spot, at))
            .fold(f64::INFINITY, f64::min)
    };
    let spots = spots(simulation);
    let mut far: Vec<Vec2> = spots
        .iter()
        .copied()
        .filter(|&spot| room(spot) > SPAWN_CLEARANCE)
        .collect();
    if far.is_empty() {
        return spots
            .into_iter()
            .max_by(|a, b| room(*a).total_cmp(&room(*b)))
            .unwrap_or(Vec2::ZERO);
    }
    far.sort_by(|a, b| distance(*b, player).total_cmp(&distance(*a, player)));
    // The farther half of the map, picked by the truck and the clock.
    let choices = far.len().div_ceil(2);
    let pick = mix(salt, (simulation.elapsed * 10.0) as u32) as usize % choices;
    far[pick]
}

impl Simulation {
    /// Add a neutral roaming truck of `kind` at `position`. It sits on the player's opposing
    /// team only so the player's shells strike and confirm hits; every rule that
    /// would make it a combatant checks `VehicleKind::neutral` instead.
    pub fn add_cybertruck(&mut self, kind: VehicleKind, position: Vec2) -> usize {
        let speed_scale = self.speed_tuning.tank_speed;
        let (body, collider) =
            super::tank_lifecycle::create_tank_body(self, kind, position, speed_scale);
        let mut tank = Tank {
            id: self.allocate_id(),
            name: "CYBERTRUCK".to_string(),
            team: self.human_team.opponent(),
            driver: Driver::Bot,
            kind,
            body,
            collider,
            alive: true,
            previous: position,
            brain: Brain {
                last_seen: position,
                last: position,
                recovery_goal: position,
                attacked_from: position,
                goal: position,
                ..Brain::default()
            },
            ..Tank::default()
        };
        tank.hp = self.max_health(&tank);
        self.tanks.push(tank);
        self.tanks.len() - 1
    }

    /// The round's trucks, at the spots farthest from the player.
    pub(crate) fn add_cybertrucks(&mut self) {
        for truck in 0..CYBERTRUCK_COUNT {
            let at = cybertruck_spawn(self, truck as u32 + 1);
            let kind = if (truck + 1).is_multiple_of(GOLD_EVERY) {
                VehicleKind::GoldCybertruck
            } else {
                VehicleKind::Cybertruck
            };
            self.add_cybertruck(kind, at);
        }
    }
}

/// A new goal: away from `threat` when fleeing, else a spot some way off.
fn next_goal(simulation: &Simulation, tank_index: usize, threat: Option<Vec2>) -> Vec2 {
    let position = simulation.tank_planar(tank_index);
    let spots = spots(simulation);
    if let Some(threat) = threat {
        // Far from the shooter, but not across the whole map from here.
        return spots
            .iter()
            .copied()
            .max_by(|a, b| {
                let score = |spot: Vec2| distance(spot, threat) - 0.5 * distance(spot, position);
                score(*a).total_cmp(&score(*b))
            })
            .unwrap_or(position);
    }
    let far: Vec<Vec2> = spots
        .iter()
        .copied()
        .filter(|&spot| distance(spot, position) > MIN_LEG)
        .collect();
    if far.is_empty() {
        return position;
    }
    let tank = &simulation.tanks[tank_index];
    far[mix(tank.id, (simulation.elapsed * 10.0) as u32) as usize % far.len()]
}

/// The truck's drive for this tick: on to its goal, or away from a shooter while the
/// hit alarm lasts. `brain.target` holds the shooter it is fleeing.
pub fn cybertruck_command(
    simulation: &mut Simulation,
    tank_index: usize,
    dt: f64,
) -> VehicleCommand {
    if let Some(careen) = &simulation.tanks[tank_index].careen {
        return careen_command(&simulation.tanks[tank_index], careen);
    }
    let position = simulation.tank_planar(tank_index);
    let brain = &mut simulation.tanks[tank_index].brain;
    brain.alarm -= dt;
    brain.decision -= dt;
    let fleeing = brain.alarm > 0.0;
    let startled = fleeing && brain.target != brain.attacker;
    if !fleeing {
        brain.target = 0;
    }
    let replan = startled || brain.decision <= 0.0 || distance(position, brain.goal) < ARRIVED;
    if replan {
        let threat = fleeing.then_some(brain.attacked_from);
        if startled {
            brain.target = brain.attacker;
        }
        let goal = next_goal(simulation, tank_index, threat);
        let mut path = std::mem::take(&mut simulation.tanks[tank_index].brain.path);
        simulation.nav.find_into(position, goal, &mut path);
        let brain = &mut simulation.tanks[tank_index].brain;
        brain.path = path;
        brain.goal = goal;
        brain.decision = GOAL_SECONDS;
    }
    let Vec2 { x: mx, z: mz } = route_direction(simulation, tank_index);
    recover_bot(simulation, tank_index, Vec2::new(mx, mz), dt);
    let mut desired = Vec2::new(mx, mz);
    if simulation.tanks[tank_index].brain.recovery > 0.0 {
        desired = route_direction(simulation, tank_index);
    }
    let Vec2 { x: mx, z: mz } = steer_bot(simulation, tank_index, desired, dt);
    let tank = &simulation.tanks[tank_index];
    let throttle = if tank.brain.alarm > 0.0 { FLEE } else { CRUISE };
    let length = 1f64.max(mx.hypot(mz));
    VehicleCommand {
        move_x: mx / length * throttle,
        move_z: mz / length * throttle,
        ..VehicleCommand::idle_aiming(tank.heading)
    }
}

/// The careening truck's drive: flat out, dragging toward the bare corner.
fn careen_command(tank: &Tank, careen: &Careen) -> VehicleCommand {
    let side = CYBERTRUCK_WHEELS[careen.wheel][0].signum();
    // The truck's local +X points at heading + 90 degrees on the X/Z plane.
    let veer = tank.heading + side * CAREEN_VEER;
    VehicleCommand {
        move_x: veer.sin() * FLEE,
        move_z: veer.cos() * FLEE,
        ..VehicleCommand::idle_aiming(tank.heading)
    }
}

/// A shell flying along `direction` struck truck `tank_index` at `point`. Near a
/// wheel it knocks that wheel off one time in `WHEEL_LOSS_ODDS`, on `owner`'s account.
pub(crate) fn strike_truck(
    simulation: &mut Simulation,
    tank_index: usize,
    point: Vec2,
    direction: Vec2,
    owner: (u32, Team, Option<u32>),
) {
    let tank = &simulation.tanks[tank_index];
    if !tank.alive || !tank.kind.neutral() || tank.careen.is_some() || tank.protection > 0.0 {
        return;
    }
    let center = simulation.tank_planar(tank_index);
    let scale = vehicle(tank.kind).scale;
    let (sin, cos) = tank.heading.sin_cos();
    // The strike in the truck's frame: across its local +X, along to its nose.
    let (dx, dz) = (point.x - center.x, point.z - center.z);
    let across = dx * cos - dz * sin;
    let along = dx * sin + dz * cos;
    let Some((wheel, reach)) = CYBERTRUCK_WHEELS
        .iter()
        .map(|&[x, z]| (across - x * scale).hypot(along - z * scale))
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(&b.1))
    else {
        return;
    };
    let roll = mix(
        tank.id ^ tank.life.rotate_left(16),
        (simulation.elapsed / STEP) as u32,
    );
    if reach > WHEEL_HIT_REACH || !roll.is_multiple_of(WHEEL_LOSS_ODDS) {
        return;
    }
    let (owner, team, owner_life) = owner;
    simulation.tanks[tank_index].careen = Some(Careen {
        wheel,
        seconds: CAREEN_SECONDS,
        owner,
        team,
        owner_life,
    });
    launch_wheel(simulation, tank_index, wheel, direction);
}

/// Send truck `tank_index`'s `wheel` off on its own: a cylinder body knocked out from
/// the flank, spinning as it would roll that way, with gyroscopic forces so it rolls
/// on its rim like a wheel until it slows and falls.
fn launch_wheel(simulation: &mut Simulation, tank_index: usize, wheel: usize, direction: Vec2) {
    let tank = &simulation.tanks[tank_index];
    let (kind, team, heading, body) = (tank.kind, tank.team, tank.heading, tank.body);
    let scale = vehicle(kind).scale;
    let radius = CYBERTRUCK_WHEEL_RADIUS * scale;
    let half_width = CYBERTRUCK_WHEEL_WIDTH * scale / 2.0;
    let shoulder = WHEEL_CROWN * half_width;
    let [x, z] = CYBERTRUCK_WHEELS[wheel];
    let (sin, cos) = heading.sin_cos();
    // Local +X and the nose on the X/Z plane.
    let (side, nose) = (Vec2::new(cos, -sin), Vec2::new(sin, cos));
    let center = simulation.tank_planar(tank_index);
    let hub = Vec2::new(
        center.x + (side.x * x + nose.x * z) * scale,
        center.z + (side.z * x + nose.z * z) * scale,
    );
    let carried = simulation.body_linvel(body);
    let length = direction.x.hypot(direction.z).max(1e-9);
    let out = x.signum() * WHEEL_KICK[0];
    // The shell only pushes it fore or aft: out of the flank it always goes.
    let shove = (direction.x * nose.x + direction.z * nose.z) / length;
    let along = z.signum() * WHEEL_KICK[1] + shove * WHEEL_KICK[2];
    let velocity = Vec2::new(
        carried.x + side.x * out + nose.x * along,
        carried.z + side.z * out + nose.z * along,
    );
    let speed = velocity.x.hypot(velocity.z);
    // The axle stands across the wheel's travel so it rolls, not skids; a wheel
    // knocked off slowly keeps the truck's axle line and topples.
    let axle = if speed > ROLLING_SPEED {
        Vec2::new(velocity.z / speed, -velocity.x / speed)
    } else {
        side
    };
    // The body's Y (the cylinder's axis, the wheel model's axle) lies along `axle`,
    // tipped about its travel so the top leans away from the truck: a positive turn
    // about the travel tips the top toward -axle.
    let travel = DVec3::new(-axle.z, 0.0, axle.x);
    let outward = x.signum() * (side.x * axle.x + side.z * axle.z);
    let lean = DQuat::from_axis_angle(travel, -WHEEL_LEAN * outward.signum());
    let rotation =
        lean * DQuat::from_rotation_y((-axle.z).atan2(axle.x)) * DQuat::from_rotation_z(-FRAC_PI_2);
    // Rolling: spin = up x velocity / radius.
    simulation.reserve_fragments(1);
    let wheel_body = simulation.world.insert_body(
        RigidBodyBuilder::dynamic()
            .translation(vector(hub.x, radius + 0.05, hub.z))
            .linvel(vector(velocity.x, WHEEL_KICK[3], velocity.z))
            .angvel(vector(velocity.z / radius, 0.0, -velocity.x / radius))
            .gyroscopic_forces_enabled(true)
            .linear_damping(WHEEL_DRAG[0] as f32)
            .angular_damping(WHEEL_DRAG[1] as f32)
            .ccd_enabled(true),
    );
    simulation.world.bodies[wheel_body].set_rotation(
        to_rotation(Quat4 {
            x: rotation.x,
            y: rotation.y,
            z: rotation.z,
            w: rotation.w,
        }),
        true,
    );
    simulation.world.insert_collider(
        // A crowned tread lets it lean and curve; a flat one would stand for good.
        ColliderBuilder::round_cylinder(
            (half_width - shoulder) as f32,
            (radius - shoulder) as f32,
            shoulder as f32,
        )
        .collision_groups(interaction_groups(group::FRAGMENT))
        .mass(WHEEL_MASS as f32)
        .friction(1.1)
        .restitution(0.35),
        Some(wheel_body),
    );
    let id = simulation.allocate_id();
    let mut fragment = Fragment::new(
        id,
        wheel_body,
        WHEEL_SECONDS + DEBRIS_CLEANUP_SECONDS,
        1.0,
        0,
    );
    fragment.wreck = Some(kind);
    fragment.part = Some(WreckPart::Wheel);
    fragment.team = Some(team);
    simulation.fragments.push(fragment);
}

/// Tip over the loose wheels that have slowed down, as a rolling wheel wobbles and
/// falls; a rolling one stays upright on its own.
fn topple_slow_wheels(simulation: &mut Simulation) {
    for fragment in &simulation.fragments {
        if fragment.part != Some(WreckPart::Wheel) {
            continue;
        }
        let body = &mut simulation.world.bodies[fragment.body];
        let velocity = body.linvel();
        let axle = *body.rotation() * Vector::Y;
        if f64::from(velocity.x.hypot(velocity.z)) > TOPPLE_SPEED
            || f64::from(axle.y.abs()) > TIPPED_AXLE
        {
            continue;
        }
        // Its spin no longer holds it up: without gyroscopic forces the push tips it
        // rather than steering it. Turning about axle x up raises the axle, onto the
        // side it already leans to.
        body.enable_gyroscopic_forces(false);
        let Some(over) = axle.cross(Vector::Y).try_normalize() else {
            continue;
        };
        let side = if axle.y < 0.0 { -1.0 } else { 1.0 };
        let tipping = body.angvel().dot(over) * side;
        if tipping < TOPPLE_RATE as f32 {
            body.set_angvel(
                body.angvel() + over * side * (TOPPLE_RATE as f32 - tipping),
                true,
            );
        }
    }
}

/// Count down the careening trucks; one whose time is up rolls over and is destroyed.
/// Runs after the shells and mines, so a truck shot dead meanwhile is already a wreck.
/// Loose wheels that have slowed down fall over.
pub(crate) fn step_cybertrucks(simulation: &mut Simulation) {
    topple_slow_wheels(simulation);
    for i in 0..simulation.tanks.len() {
        let tank = &mut simulation.tanks[i];
        let Some(careen) = tank.careen.as_mut().filter(|_| tank.alive) else {
            continue;
        };
        careen.seconds -= STEP;
        if careen.seconds > 0.0 {
            continue;
        }
        let careen = *careen;
        let lethal = tank.hp + tank.shield_points + 1.0;
        let origin = simulation.tank_planar(i);
        simulation.damage_tank(
            i,
            lethal,
            careen.owner,
            careen.team,
            careen.owner_life,
            Some(DamageSource {
                cause: DamageCause::Explosion,
                origin,
            }),
        );
    }
}

/// Whether a truck's death is a quiet burnout: some plain trucks that crashed on
/// three wheels only roll over and burn; every other truck goes up in a blast.
pub(crate) fn quiet_crash(tank: &Tank) -> bool {
    tank.careen.is_some()
        && tank.kind == VehicleKind::Cybertruck
        && mix(tank.id, tank.life).is_multiple_of(QUIET_CRASH_ODDS)
}

/// A crashing truck's tumble: rolled over onto the bare corner rather than at random.
pub(crate) fn careen_tumble(careen: &Careen) -> HumveeTumble {
    let [x, z] = CYBERTRUCK_WHEELS[careen.wheel];
    // A positive roll lifts local +X; a positive pitch drops the nose.
    HumveeTumble {
        height: 1.1,
        pitch: 1.3 * z.signum(),
        roll: -5.2 * x.signum(),
        yaw: 0.6 * x.signum(),
        damping: 0.6,
    }
}
