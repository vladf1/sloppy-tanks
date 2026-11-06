//! Continuous projectile flight. Every tick resolves the earliest contact across all shells,
//! then queries again after any bounce, interception or destruction; array order never
//! decides which contact happens first.

use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::parry::shape::{Ball, Capsule};
use rapier3d::prelude::{ColliderHandle, Pose, Ray, RigidBodyHandle};

use super::combat_rules::COMBAT;
use super::data::{
    INTERCEPTION_BLAST_RADIUS, INTERCEPTION_RADIUS, LASER_DEFENSE, MINE_RADIUS, group, pickup,
    weapon,
};
use super::debris_physics::{blast_debris, hit_movable_cover, hit_projectile_debris};
use super::hitboxes::{HullAt, SHELL_HIT_RADIUS, ShotProbe, tank_hit_time};
use super::laser_defense::laser_contact_time;
use super::math::{Point3, Vec2, angle_delta, distance};
use super::physics::{from_vector, query_filter, vector};
use super::shooter_rewind::{ShooterRewind, target_ticks};
use super::simulation::{ProjectileMove, Simulation};
use super::structure_physics::{
    FUSE_DISTANCE, HE_BLAST_RADIUS, fragment_with_collider, structure_debris,
};
use super::tank_dimensions::tank_muzzle;
use super::types::{
    CoverKind, DamageCause, DamageSource, Mine, PickupKind, Shot, SimEvent, SimEventType, Weapon,
};

/// A shell meets a structure's fallen part lying under its path when the part's top
/// reaches within this far (m) below the shell's flight height, as a slightly plunging
/// shot would: a level shell at one metre passes over a slab lying flat on the ground.
const LOW_DEBRIS_REACH: f64 = 0.7;
/// A burst in a cottage's room tears this much (m) of its lath and plaster, this colour.
const ROOM_BURST_SIZE: f64 = 1.2;
const ROOM_PLASTER: u32 = 0xc9b48f;
/// How hard an armour-piercing round throws the pieces of a loose part it breaks, as a
/// share of a burst's.
const PIERCING_THROW: f64 = 0.4;

/// Impact flash colour for a shell striking a friendly hull.
const FRIENDLY_IMPACT_COLOR: u32 = 0xb9d7e5;
const INTERCEPTION_FLASH_COLOR: u32 = 0xfff0b4;

fn guide_tow_missile(simulation: &mut Simulation, shot_index: usize, dt: f64) {
    let shot = &simulation.shots[shot_index];
    let Some(target_id) = shot.target_id else {
        return;
    };
    if shot.weapon != Weapon::Tow {
        return;
    }
    let target = simulation.tanks.iter().find(|tank| {
        tank.id == target_id
            && Some(tank.life) == shot.target_life
            && tank.alive
            && tank.team != shot.team
    });
    let Some(target) = target else {
        // A lost target or a new life cannot inherit the launch lock.
        let shot = &mut simulation.shots[shot_index];
        shot.target_id = None;
        shot.target_life = None;
        return;
    };
    let speed = shot.vx.hypot(shot.vz);
    if speed <= 0.0 {
        return;
    }
    let position = simulation.body_translation(target.body);
    let desired = (position.x - shot.x).atan2(position.z - shot.z);
    let current = shot.vx.atan2(shot.vz);
    let turn = (-COMBAT.tow_turn_rate * dt)
        .max((COMBAT.tow_turn_rate * dt).min(angle_delta(current, desired)));
    let heading = current + turn;
    let shot = &mut simulation.shots[shot_index];
    shot.vx = heading.sin() * speed;
    shot.vz = heading.cos() * speed;
}

/// When a point at `x, z` moving at `vx, vz` enters the circle of `radius` about the
/// origin, within `limit` seconds; 0 when it starts inside.
#[inline]
fn entry_time(x: f64, z: f64, vx: f64, vz: f64, radius: f64, limit: f64) -> Option<f64> {
    let c = x * x + z * z - radius.powi(2);
    if c <= 0.0 {
        return Some(0.0);
    }
    let speed2 = vx * vx + vz * vz;
    let approach = x * vx + z * vz;
    if speed2 == 0.0 || approach >= 0.0 {
        return None;
    }
    let discriminant = approach * approach - speed2 * c;
    if discriminant < 0.0 {
        return None;
    }
    let time = (-approach - discriminant.sqrt()) / speed2;
    (time >= 0.0 && time <= limit).then_some(time)
}

/// Continuous relative-motion contact, including shots that cross between ticks.
pub fn interception_time(a: &Shot, b: &Shot, limit: f64) -> Option<f64> {
    if a.team == b.team || a.pierced_shot == Some(b.id) || b.pierced_shot == Some(a.id) {
        return None;
    }
    let (x, z) = (a.x - b.x, a.z - b.z);
    let (vx, vz) = (a.vx - b.vx, a.vz - b.vz);
    entry_time(x, z, vx, vz, INTERCEPTION_RADIUS, limit)
}

fn intercept(simulation: &mut Simulation, a: &Shot, b: &Shot) {
    let point = Vec2::new((a.x + b.x) / 2.0, (a.z + b.z) / 2.0);
    let radius = if a.weapon == Weapon::Rocket || b.weapon == Weapon::Rocket {
        COMBAT.rocket_blast_radius
    } else {
        INTERCEPTION_BLAST_RADIUS
    };
    let mut explosion = SimEvent::at(SimEventType::Explosion, point.x, point.z);
    explosion.size = Some(radius);
    explosion.color = Some(INTERCEPTION_FLASH_COLOR);
    simulation.events.push(explosion);
    let standard_damage = weapon(Weapon::Standard).damage;
    blast_debris(simulation, point, radius, standard_damage);
    // One standard hit, like V-Tanks. Each team receives the opposing shell's
    // damage ownership: both sides can be hurt, without double damage or ally fire.
    // This blast only hits tanks; it does not invent cover/mine chain reactions.
    for i in 0..simulation.tanks.len() {
        let tank = &simulation.tanks[i];
        if !tank.alive || distance(simulation.body_translation(tank.body).planar(), point) >= radius
        {
            continue;
        }
        let enemy_shot = if a.team != tank.team { a } else { b };
        simulation.damage_tank(
            i,
            standard_damage,
            enemy_shot.owner,
            enemy_shot.team,
            enemy_shot.owner_life,
            Some(DamageSource {
                cause: DamageCause::Interception,
                origin: point,
            }),
        );
    }
}

fn mine_hit_time(shot: &Shot, mine: &Mine, limit: f64) -> Option<f64> {
    let radius = MINE_RADIUS + SHELL_HIT_RADIUS;
    let (x, z) = (shot.x - mine.x, shot.z - mine.z);
    entry_time(x, z, shot.vx, shot.vz, radius, limit)
}

/// A rocket's speed-up: at the start of every tick its speed grows by `acceleration` times
/// the step, until `top_speed`. Room clients replay it to draw rockets between updates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RocketThrust {
    /// Metres per second squared.
    pub acceleration: f64,
    pub top_speed: f64,
}

impl RocketThrust {
    pub fn standard() -> Self {
        let base_speed = weapon(Weapon::Rocket).speed;
        let top_speed = base_speed * COMBAT.rocket_top_speed_multiplier;
        Self {
            acceleration: (top_speed - base_speed) / COMBAT.rocket_acceleration_seconds,
            top_speed,
        }
    }
}

pub fn step_projectiles(simulation: &mut Simulation, dt: f64, sweep_tank_motion: bool) {
    simulation.shots.retain(|shot| shot.life > 0.0);
    ShooterRewind::record(simulation);
    // Accelerate once per fixed tick, before all continuous collision sweeps.
    // Contact retries within this tick must not apply thrust again.
    for i in 0..simulation.shots.len() {
        let life = simulation.shots[i].life;
        guide_tow_missile(simulation, i, dt.min(life));
    }
    let thrust = RocketThrust::standard();
    for shot in simulation
        .shots
        .iter_mut()
        .filter(|shot| shot.weapon == Weapon::Rocket)
    {
        let speed = shot.vx.hypot(shot.vz);
        if speed > 0.0 && speed < thrust.top_speed {
            let scale = thrust
                .top_speed
                .min(speed + thrust.acceleration * dt.min(shot.life))
                / speed;
            shot.vx *= scale;
            shot.vz *= scale;
        }
    }
    let mut remaining = dt;
    // Resolve the earliest contact across all shells, then query again after any
    // bounce/destruction. A wall or tank hit cannot be undone by a later intercept.
    let defenses = simulation
        .tanks
        .iter()
        .filter(|tank| tank.alive && tank.laser > 0.0)
        .count();
    let budget = simulation.shots.len() * (COMBAT.contacts_per_shot + defenses) + 1;
    let mut event = 0;
    let mut tank_positions = std::mem::take(&mut simulation.projectile_tank_positions);
    let mut structure_bodies = std::mem::take(&mut simulation.projectile_structure_bodies);
    while remaining > COMBAT.contact_time_epsilon && !simulation.shots.is_empty() && event < budget
    {
        let (next, time) = find_next_contact(
            simulation,
            &mut tank_positions,
            &mut structure_bodies,
            remaining,
            dt - remaining,
            if sweep_tank_motion { dt } else { 0.0 },
            defenses > 0,
        );
        let offset = dt - remaining;
        for shot in &mut simulation.shots {
            shot.x += shot.vx * time;
            shot.z += shot.vz * time;
            shot.life -= time;
            if let Some(fuse) = &mut shot.fuse {
                *fuse -= shot.vx.hypot(shot.vz) * time;
            }
        }
        if let Some(moves) = &mut simulation.projectile_moves {
            moves.extend(simulation.shots.iter().map(|shot| ProjectileMove {
                shot: shot.clone(),
                seconds: time,
                offset,
            }));
        }
        remaining -= time;
        let Some((shot_index, next)) = next else {
            break;
        };
        let shot_id = simulation.shots[shot_index].id;
        let fraction = if sweep_tank_motion {
            (dt - remaining) / dt
        } else {
            1.0
        };
        if resolve_contact(simulation, shot_index, next, fraction, dt - remaining) {
            remove_shot(simulation, shot_id);
        }
        event += 1;
    }
    simulation.projectile_tank_positions = tank_positions;
    simulation.projectile_structure_bodies = structure_bodies;
}

/// The earliest contact found for one shell, returned beside that shell's index; indices
/// are valid until something moves.
#[derive(Clone, Copy, Debug)]
enum Contact {
    World {
        collider: ColliderHandle,
        normal: Vec2,
    },
    Debris {
        fragment: usize,
        collider: ColliderHandle,
    },
    Tank {
        tank: usize,
    },
    Mine {
        mine: usize,
    },
    Pair {
        other: usize,
    },
    Laser {
        tank: usize,
    },
    /// An HE shell's fuse after striking a cottage has run out.
    Fuse,
    Expiry,
}

/// Query without moving entities; equal-time contacts preserve the original priority order.
/// `tank_positions` and `structure_bodies` are scratch space reused across the queries of
/// a tick.
fn find_next_contact(
    simulation: &Simulation,
    tank_positions: &mut Vec<Option<Point3>>,
    structure_bodies: &mut Vec<RigidBodyHandle>,
    limit: f64,
    elapsed: f64,
    tank_frame_delta: f64,
    defenses: bool,
) -> (Option<(usize, Contact)>, f64) {
    let mut next = None;
    let mut time = limit;
    // Nothing moves during the query, so each hull is read once rather than once per shell.
    // A resolved contact can destroy a tank, so the positions are read again for every query.
    tank_positions.clear();
    tank_positions.extend(
        simulation
            .tanks
            .iter()
            .map(|tank| tank.alive.then(|| simulation.body_translation(tank.body))),
    );
    let shell_shape = Ball::new(SHELL_HIT_RADIUS as f32);
    // The low reach under a shell for a cottage's loose parts: a capsule from the shell
    // down to [`LOW_DEBRIS_REACH`] under it.
    let low_half = ((LOW_DEBRIS_REACH - SHELL_HIT_RADIUS) / 2.0) as f32;
    let low_shape = Capsule::new_y(low_half, SHELL_HIT_RADIUS as f32);
    // A resolved contact can split or remove a fallen body, so these are read again for
    // every query too; sorted, the cast's predicate finds a body by halving.
    structure_bodies.clear();
    structure_bodies.extend(
        simulation
            .fragments
            .iter()
            .filter(|fragment| structure_debris(fragment))
            .map(|fragment| fragment.body),
    );
    structure_bodies.sort_unstable_by_key(|body| body.into_raw_parts());
    let structure_bodies = &*structure_bodies;
    let in_structure_debris = |_: ColliderHandle, collider: &rapier3d::prelude::Collider| {
        collider.parent().is_some_and(|body| {
            structure_bodies
                .binary_search_by_key(&body.into_raw_parts(), |body| body.into_raw_parts())
                .is_ok()
        })
    };
    for (si, shot) in simulation.shots.iter().enumerate() {
        if shot.life <= time {
            time = shot.life;
            next = Some((si, Contact::Expiry));
        }
        let speed = shot.vx.hypot(shot.vz);
        if let Some(fuse) = shot.fuse
            && speed > 0.0
            && (fuse / speed).max(0.0) <= time
        {
            time = (fuse / speed).max(0.0);
            next = Some((si, Contact::Fuse));
        }
        if speed > 0.0 {
            let ray = Ray::new(
                vector(shot.x, shot.combat_y(), shot.z),
                vector(shot.vx / speed, 0.0, shot.vz / speed),
            );
            if let Some((collider, hit)) = simulation.world.cast_ray_and_get_normal(
                &ray,
                (speed * time) as f32,
                true,
                query_filter(group::COVER_QUERY),
            ) && hit.time_of_impact as f64 / speed <= time
            {
                time = hit.time_of_impact as f64 / speed;
                let normal = Vec2::new(hit.normal.x as f64, hit.normal.z as f64);
                next = Some((si, Contact::World { collider, normal }));
            }
            let options = ShapeCastOptions {
                max_time_of_impact: time as f32,
                target_distance: 0.0,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            };
            // The bodies its own strike knocked loose it flew on through.
            let not_loosened = |_: ColliderHandle, collider: &rapier3d::prelude::Collider| {
                collider
                    .parent()
                    .is_none_or(|body| !shot.loosened.contains(&body))
            };
            let in_reach = |handle: ColliderHandle, collider: &rapier3d::prelude::Collider| {
                in_structure_debris(handle, collider) && not_loosened(handle, collider)
            };
            let debris_filter = if shot.loosened.is_empty() {
                query_filter(group::DEBRIS_QUERY)
            } else {
                query_filter(group::DEBRIS_QUERY).predicate(&not_loosened)
            };
            // Only a fragment's collider counts as a debris contact, so without fragments the
            // cast cannot change the result.
            if !simulation.fragments.is_empty()
                && let Some((collider, hit)) = simulation.world.cast_shape(
                    &Pose::from_translation(vector(shot.x, shot.combat_y(), shot.z)),
                    vector(shot.vx, 0.0, shot.vz),
                    &shell_shape,
                    options,
                    debris_filter,
                )
                && hit.time_of_impact as f64 <= time
                && let Some(fragment) = fragment_with_collider(simulation, collider)
            {
                time = hit.time_of_impact as f64;
                next = Some((si, Contact::Debris { fragment, collider }));
            }
            // The capsule's top is the shell's own ball; its foot the low reach.
            let low = shot.combat_y() - f64::from(low_half);
            if !structure_bodies.is_empty()
                && let Some((collider, hit)) = simulation.world.cast_shape(
                    &Pose::from_translation(vector(shot.x, low, shot.z)),
                    vector(shot.vx, 0.0, shot.vz),
                    &low_shape,
                    ShapeCastOptions {
                        max_time_of_impact: time as f32,
                        ..options
                    },
                    query_filter(group::DEBRIS_QUERY).predicate(&in_reach),
                )
                && (hit.time_of_impact as f64) < time
                && let Some(fragment) = fragment_with_collider(simulation, collider)
            {
                time = hit.time_of_impact as f64;
                next = Some((si, Contact::Debris { fragment, collider }));
            }
        }
        let probe = ShotProbe::from(shot);
        // A player's shell meets the other hulls where its shooter saw them. A shooter
        // killed since sees them all on the delayed display, none near its hull.
        let rewind = simulation
            .shooter_rewind
            .as_ref()
            .map(|rewind| (rewind, rewind.shell_ticks(shot.id)))
            .filter(|(_, ticks)| *ticks > 0)
            .map(|(rewind, ticks)| {
                let shooter = simulation
                    .tank_index(shot.owner)
                    .and_then(|index| tank_positions[index]);
                (rewind, ticks, shooter)
            });
        for (ti, tank) in simulation.tanks.iter().enumerate() {
            let past = rewind
                .filter(|_| tank.id != shot.owner)
                .zip(tank_positions[ti])
                .and_then(|((rewind, ticks, shooter), at)| {
                    let ticks = shooter.map_or(ticks, |shooter| {
                        target_ticks(ticks, distance(shooter.planar(), at.planar()))
                    });
                    (ticks > 0)
                        .then(|| rewind.hull(tank.id, tank.life, ticks))
                        .flatten()
                });
            let hull = match (past, tank_positions[ti]) {
                (Some(past), _) => HullAt::Past(past),
                (None, Some(end)) => HullAt::Translation(end),
                (None, None) => HullAt::Body,
            };
            if let Some(contact) = tank_hit_time(
                simulation,
                &probe,
                tank,
                time,
                elapsed,
                tank_frame_delta,
                hull,
            ) && (contact < time || next.is_none())
            {
                time = contact;
                next = Some((si, Contact::Tank { tank: ti }));
            }
            if defenses
                && tank.laser > 0.0
                && let Some(laser) =
                    laser_contact_time(simulation, shot, tank, time, elapsed, tank_frame_delta)
                && (laser < time || next.is_none())
            {
                time = laser;
                next = Some((si, Contact::Laser { tank: ti }));
            }
        }
        for (mi, mine) in simulation.mines.iter().enumerate() {
            if let Some(contact) = mine_hit_time(shot, mine, time)
                && (contact < time || next.is_none())
            {
                time = contact;
                next = Some((si, Contact::Mine { mine: mi }));
            }
        }
    }
    let shots = &simulation.shots;
    for i in 0..shots.len() {
        for j in i + 1..shots.len() {
            let (a, b) = (&shots[i], &shots[j]);
            let Some(contact) = interception_time(a, b, time) else {
                continue;
            };
            if contact >= time && next.is_some() {
                continue;
            }
            // Generous shell contact radii must not reach through thin cover.
            let ax = a.x + a.vx * contact;
            let az = a.z + a.vz * contact;
            let bx = b.x + b.vx * contact;
            let bz = b.z + b.vz * contact;
            let separation = (bx - ax).hypot(bz - az);
            if separation > COMBAT.separation_epsilon {
                let ray = Ray::new(
                    vector(ax, 1.0, az),
                    vector((bx - ax) / separation, 0.0, (bz - az) / separation),
                );
                if simulation
                    .world
                    .cast_ray(
                        &ray,
                        separation as f32,
                        true,
                        query_filter(group::COVER_QUERY),
                    )
                    .is_some()
                {
                    continue;
                }
            }
            time = contact;
            next = Some((i, Contact::Pair { other: j }));
        }
    }
    (next, time)
}

fn impact_event(x: f64, z: f64, size: f64, color: u32) -> SimEvent {
    let mut impact = SimEvent::at(SimEventType::Impact, x, z);
    impact.size = Some(size);
    impact.color = Some(color);
    impact
}

/// The way a shell flies, as a unit vector on the ground (+z when it stands still).
fn shell_heading(shot: &Shot) -> Vec2 {
    let speed = shot.vx.hypot(shot.vz);
    if speed > 0.0 {
        Vec2::new(shot.vx / speed, shot.vz / speed)
    } else {
        Vec2::new(0.0, 1.0)
    }
}

fn remove_shot(simulation: &mut Simulation, id: u32) {
    if let Some(index) = simulation.shots.iter().position(|shot| shot.id == id) {
        simulation.shots.remove(index);
    }
}

/// A rocket's blast where it struck.
fn rocket_blast(simulation: &mut Simulation, shot: &Shot) {
    simulation.explode(
        Vec2::new(shot.x, shot.z),
        COMBAT.rocket_blast_radius,
        shot.damage,
        shot.owner,
        shot.team,
        shot.owner_life,
        DamageCause::Rocket,
    );
}

/// Apply shell `si`'s contact `elapsed` seconds into the sweep. Return whether that shell
/// should be removed.
fn resolve_contact(
    simulation: &mut Simulation,
    si: usize,
    next: Contact,
    fraction: f64,
    elapsed: f64,
) -> bool {
    let shot = simulation.shots[si].clone();
    let shot_color = weapon(shot.weapon).color;
    let mut remove = true;
    match next {
        Contact::Laser { tank: ti } => {
            let tank_id = simulation.tanks[ti].id;
            simulation.shots[si].laser_checked_by.push(tank_id);
            remove = simulation.rng.next() < LASER_DEFENSE.chance;
            if remove {
                // Offsets within this sweep line up with next tick's decrement by STEP.
                simulation.tanks[ti].laser_recharge = elapsed + LASER_DEFENSE.recharge;
                let tank = &simulation.tanks[ti];
                let end = simulation.body_translation(tank.body);
                let mut laser = SimEvent::at(SimEventType::Laser, shot.x, shot.z);
                laser.height = Some(shot.combat_y());
                laser.from = Some(Point3::new(
                    tank.previous.x + (end.x - tank.previous.x) * fraction,
                    end.y - 0.4 + tank_muzzle(tank.kind).y + 0.3,
                    tank.previous.z + (end.z - tank.previous.z) * fraction,
                ));
                laser.id = Some(tank.id);
                laser.team = Some(tank.team);
                laser.color = Some(pickup(PickupKind::Laser).color);
                laser.size = Some(0.35);
                simulation.events.push(laser);
            }
            // A successful zap vaporizes the shell without triggering a rocket blast.
        }
        Contact::Pair { other: oi } => {
            let other = simulation.shots[oi].clone();
            let a_pierces = shot.piercing > 0;
            let b_pierces = other.piercing > 0;
            if a_pierces || b_pierces {
                simulation.events.push(impact_event(
                    (shot.x + other.x) / 2.0,
                    (shot.z + other.z) / 2.0,
                    0.35,
                    weapon(Weapon::Piercing).color,
                ));
                if a_pierces {
                    simulation.shots[si].piercing -= 1;
                }
                if b_pierces {
                    simulation.shots[oi].piercing -= 1;
                }
                if a_pierces && b_pierces {
                    simulation.shots[si].pierced_shot = Some(other.id);
                    simulation.shots[oi].pierced_shot = Some(shot.id);
                }
                remove = !a_pierces;
                if !b_pierces {
                    remove_shot(simulation, other.id);
                }
            } else {
                intercept(simulation, &shot, &other);
                remove_shot(simulation, other.id);
            }
        }
        Contact::Mine { mine: mi } => {
            // Remove first so the blast cannot rediscover and detonate this mine twice.
            let mine = simulation.mines.remove(mi);
            simulation.detonate_mine(&mine, shot.owner, shot.team, shot.owner_life);
        }
        Contact::Debris {
            fragment: fi,
            collider,
        } if structure_debris(&simulation.fragments[fi]) => {
            // A structure's fallen part breaks where the shell meets it: an HE round
            // bursts on it, an armour-piercing round breaks the part it strikes and
            // flies on, a rocket goes off.
            let color = simulation.fragments[fi].color;
            let shell_point = Point3::new(shot.x, shot.combat_y(), shot.z);
            let (point, part) = simulation.world.colliders.get(collider).map_or(
                (shell_point, usize::MAX),
                |struck| {
                    let on = struck.shape().project_point(
                        struck.position(),
                        vector(shell_point.x, shell_point.y, shell_point.z),
                        true,
                    );
                    (from_vector(on.point), struck.user_data as usize)
                },
            );
            let kind = simulation.fragments[fi].source_kind;
            let mut impact = impact_event(point.x, point.z, 0.6, color);
            impact.height = Some(point.y);
            impact.cover_kind = kind;
            simulation.events.push(impact);
            match shot.weapon {
                Weapon::Rocket => rocket_blast(simulation, &shot),
                Weapon::Piercing => {
                    simulation.break_loose_body(fi, part, point, PIERCING_THROW);
                    remove = false;
                }
                _ => burst_at(simulation, &shot, point),
            }
        }
        Contact::Debris { fragment: fi, .. } => {
            let fragment = &simulation.fragments[fi];
            let shell_point = Point3::new(shot.x, shot.combat_y(), shot.z);
            let body = &simulation.world.bodies[fragment.body];
            let point = body
                .colliders()
                .first()
                .map(|&handle| {
                    let collider = &simulation.world.colliders[handle];
                    from_vector(
                        collider
                            .shape()
                            .project_point(
                                collider.position(),
                                vector(shell_point.x, shell_point.y, shell_point.z),
                                true,
                            )
                            .point,
                    )
                })
                .unwrap_or(shell_point);
            if shot.weapon == Weapon::Rocket {
                rocket_blast(simulation, &shot);
            } else {
                hit_projectile_debris(simulation, fi, &shot, point);
            }
            let mut impact = impact_event(point.x, point.z, 0.6, shot_color);
            impact.height = Some(point.y);
            simulation.events.push(impact);
        }
        Contact::Tank { tank: ti } => {
            let target = &simulation.tanks[ti];
            if !shot.recap_hit
                && simulation
                    .tanks
                    .iter()
                    .any(|tank| simulation.records(tank) && tank.id == shot.owner)
                && target.team != shot.team
                && target.protection <= 0.0
            {
                simulation.shots[si].recap_hit = true;
                simulation.combat_record.direct_hits += 1;
            }
            let target_team = simulation.tanks[ti].team;
            if shot.weapon == Weapon::Rocket {
                rocket_blast(simulation, &shot);
            } else if simulation.tanks[ti].id != shot.owner {
                // A shell that ricochets back stops at its shooter, harmless like at an ally.
                let position = simulation.body_translation(simulation.tanks[ti].body);
                let speed = match shot.vx.hypot(shot.vz) {
                    0.0 => 1.0,
                    speed => speed,
                };
                simulation.damage_tank(
                    ti,
                    shot.damage,
                    shot.owner,
                    shot.team,
                    shot.owner_life,
                    Some(DamageSource {
                        cause: shot.weapon.into(),
                        origin: Vec2::new(
                            position.x - shot.vx / speed,
                            position.z - shot.vz / speed,
                        ),
                    }),
                );
            }
            let mut impact = impact_event(
                shot.x,
                shot.z,
                0.6,
                if target_team == shot.team {
                    FRIENDLY_IMPACT_COLOR
                } else {
                    shot_color
                },
            );
            impact.height = Some(1.0);
            simulation.events.push(impact);
        }
        // A structure's breakable part; a watchtower's concrete footings take a shell
        // like other concrete.
        Contact::World { collider, .. }
            if simulation
                .structure_part(collider, Point3::new(shot.x, shot.combat_y(), shot.z))
                .is_some_and(|(cover, part)| {
                    simulation.covers[cover].structure.as_ref().is_some_and(|state| {
                        state.layout.parts[part].strength.is_finite()
                    })
                }) =>
        {
            let (cover, part) = simulation
                .structure_part(collider, Point3::new(shot.x, shot.combat_y(), shot.z))
                .expect("checked by the guard");
            remove = strike_structure_part(simulation, si, cover, part);
        }
        Contact::World { collider, normal } => {
            let cover = simulation.cover_by_collider.get(&collider).copied();
            if let Some(ci) = cover {
                hit_movable_cover(simulation, ci, &shot);
            }
            if shot.weapon == Weapon::Rocket {
                rocket_blast(simulation, &shot);
            } else if let Some(ci) = cover {
                simulation.damage_cover(
                    ci,
                    shot.damage,
                    shot.owner,
                    shot.team,
                    shot.owner_life,
                    Some(Point3::new(shot.x, shot.combat_y(), shot.z)),
                );
                if simulation.covers[ci].alive && shot.bounces > 0 {
                    let bounced = &mut simulation.shots[si];
                    let dot = bounced.vx * normal.x + bounced.vz * normal.z;
                    bounced.vx -= 2.0 * dot * normal.x;
                    bounced.vz -= 2.0 * dot * normal.z;
                    bounced.bounces -= 1;
                    bounced.ricocheted = true;
                    bounced.x += normal.x * COMBAT.bounce_clearance;
                    bounced.z += normal.z * COMBAT.bounce_clearance;
                    let mut ricochet = SimEvent::at(SimEventType::Ricochet, bounced.x, bounced.z);
                    ricochet.id = Some(bounced.id);
                    ricochet.owner = Some(bounced.owner);
                    ricochet.owner_life = bounced.owner_life;
                    ricochet.weapon = Some(bounced.weapon);
                    ricochet.team = Some(bounced.team);
                    ricochet.size = Some(0.6);
                    simulation.events.push(ricochet);
                    remove = false;
                }
            }
            let shot = &simulation.shots[si];
            let chipped = cover
                .map(|ci| &simulation.covers[ci])
                .filter(|cover| cover.alive && cover.kind == CoverKind::Tree);
            let mut impact = impact_event(
                shot.x,
                shot.z,
                0.6,
                chipped.map_or(shot_color, |cover| cover.color),
            );
            impact.cover_kind = chipped.map(|cover| cover.kind);
            impact.height = chipped.map(|cover| cover.h);
            simulation.events.push(impact);
        }
        Contact::Fuse => burst(simulation, &shot),
        Contact::Expiry => {}
    }
    remove
}

/// An HE shell bursting where its fuse ran out (or where it struck again): the parts
/// round it take the blast; tanks are hit only by what strikes them. A burst inside a
/// cottage names the hole the shell came in by, which its flash and dust escape through.
fn burst(simulation: &mut Simulation, shot: &Shot) {
    burst_at(simulation, shot, Point3::new(shot.x, shot.combat_y(), shot.z));
}

/// [`burst`] at `point`: on a structure's fallen part, where the shell met it.
fn burst_at(simulation: &mut Simulation, shot: &Shot, point: Point3) {
    let mut explosion = SimEvent::at(SimEventType::Explosion, point.x, point.z);
    explosion.size = Some(HE_BLAST_RADIUS);
    explosion.height = Some(point.y);
    // A shell fused in a cottage bursts in a room; one fused by a watchtower's member, a
    // timber wall or a crate bursts in the open.
    let in_cottage = simulation.covers.iter().any(|cover| {
        cover.kind == CoverKind::House
            && cover.structure.is_some()
            && (point.x - cover.x).abs() < cover.w / 2.0
            && (point.z - cover.z).abs() < cover.d / 2.0
    });
    if let Some(fuse) = shot.fuse.filter(|_| in_cottage) {
        let along = shell_heading(shot);
        let flown = (FUSE_DISTANCE - fuse.max(0.0)).max(0.0);
        explosion.cover_kind = Some(CoverKind::House);
        explosion.from = Some(Point3::new(
            point.x - along.x * flown,
            point.y,
            point.z - along.z * flown,
        ));
    }
    // In a room, the burst tears the room's lath and plaster round it: splinters and
    // dust fly from where it went off, inside, not only out of the hole.
    if let (Some(CoverKind::House), Some(hole)) = (explosion.cover_kind, explosion.from) {
        let mut torn = SimEvent::at(SimEventType::Shatter, point.x, point.z);
        torn.height = Some(point.y);
        torn.size = Some(ROOM_BURST_SIZE);
        torn.color = Some(ROOM_PLASTER);
        torn.cover_kind = Some(CoverKind::House);
        torn.from = Some(hole);
        simulation.events.push(explosion);
        simulation.events.push(torn);
    } else {
        simulation.events.push(explosion);
    }
    simulation.structure_blast(
        point,
        HE_BLAST_RADIUS,
        shot.damage,
        Some((shot.owner, shot.team)),
    );
}

/// A shell striking a standing part of a structure. A rocket breaks it and bursts; an
/// armour-piercing round breaks it and flies on; an HE round breaks a part it can and
/// flies on [`FUSE_DISTANCE`] (bursting at once against one it cannot, such as a
/// watchtower's leg, or on a second strike). A shell that ends there flashes on the part;
/// one that flies on through shows only the part breaking, and a fused round striking a
/// wall from inside shows no hit on its outer face. Returns whether the shell is spent.
fn strike_structure_part(simulation: &mut Simulation, si: usize, cover: usize, part: usize) -> bool {
    let shot = simulation.shots[si].clone();
    let point = Point3::new(shot.x, shot.combat_y(), shot.z);
    let along = shell_heading(&shot);
    let by = Some((shot.owner, shot.team));
    let mut impact = impact_event(shot.x, shot.z, 0.6, weapon(shot.weapon).color);
    impact.id = Some(simulation.covers[cover].id);
    impact.height = Some(point.y);
    match shot.weapon {
        Weapon::Rocket => {
            simulation.events.push(impact);
            simulation.strike_structure(cover, part, shot.damage, true, point, along, by);
            rocket_blast(simulation, &shot);
            true
        }
        Weapon::Piercing => {
            let first = simulation.next_id;
            simulation.strike_structure(cover, part, shot.damage, true, point, along, by);
            fly_past_loosened(simulation, si, first);
            false
        }
        _ => {
            let first = simulation.next_id;
            let broke = simulation.strike_structure(cover, part, shot.damage, false, point, along, by);
            if broke && shot.fuse.is_none() {
                simulation.shots[si].fuse = Some(FUSE_DISTANCE);
                fly_past_loosened(simulation, si, first);
                false
            } else {
                if shot.fuse.is_none() {
                    simulation.events.push(impact);
                }
                burst(simulation, &shot);
                true
            }
        }
    }
}

/// Shell `si` flies on through the part it broke: the bodies that came down from the
/// strike (the structure debris given ids from `first` on) start round it, and it never
/// meets them, as the shooter's own forecast has it.
fn fly_past_loosened(simulation: &mut Simulation, si: usize, first: u32) {
    let Simulation { fragments, shots, .. } = simulation;
    shots[si].loosened.extend(
        fragments
            .iter()
            .filter(|fragment| fragment.id >= first && structure_debris(fragment))
            .map(|fragment| fragment.body),
    );
}
