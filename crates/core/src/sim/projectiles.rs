//! Continuous projectile flight. Every tick resolves the earliest contact across all shells,
//! then queries again after any bounce, interception or destruction; array order never
//! decides which contact happens first.

use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::parry::shape::Ball;
use rapier3d::prelude::{ColliderHandle, Pose, Ray};

use super::combat_rules::{COMBAT, MINE};
use super::damage::{damage_cover, damage_tank, explode};
use super::data::{
    INTERCEPTION_BLAST_RADIUS, INTERCEPTION_RADIUS, LASER_DEFENSE, MINE_RADIUS, group, pickup,
    weapon,
};
use super::debris_physics::{blast_debris, hit_movable_cover, hit_projectile_debris};
use super::hitboxes::{SHELL_HIT_RADIUS, ShotProbe, tank_hit_time};
use super::laser_defense::laser_contact_time;
use super::math::{Point3, Vec2, angle_delta, distance};
use super::physics::{from_vector, query_filter, vector};
use super::simulation::{ProjectileMove, Simulation};
use super::tank_dimensions::tank_muzzle;
use super::types::{
    CoverKind, DamageCause, DamageSource, Mine, PickupKind, Shot, SimEvent, SimEventType, Weapon,
};

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

/// Continuous relative-motion contact, including shots that cross between ticks.
pub fn interception_time(a: &Shot, b: &Shot, limit: f64) -> Option<f64> {
    if a.team == b.team || a.pierced_shot == Some(b.id) || b.pierced_shot == Some(a.id) {
        return None;
    }
    let x = a.x - b.x;
    let z = a.z - b.z;
    let vx = a.vx - b.vx;
    let vz = a.vz - b.vz;
    let c = x * x + z * z - INTERCEPTION_RADIUS.powi(2);
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
    (time <= limit).then_some(time)
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
        damage_tank(
            simulation,
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
    let x = shot.x - mine.x;
    let z = shot.z - mine.z;
    let c = x * x + z * z - (MINE_RADIUS + SHELL_HIT_RADIUS).powi(2);
    if c <= 0.0 {
        return Some(0.0);
    }
    let speed2 = shot.vx.powi(2) + shot.vz.powi(2);
    let approach = x * shot.vx + z * shot.vz;
    if speed2 == 0.0 || approach >= 0.0 {
        return None;
    }
    let discriminant = approach.powi(2) - speed2 * c;
    if discriminant < 0.0 {
        return None;
    }
    let time = (-approach - discriminant.sqrt()) / speed2;
    (time >= 0.0 && time <= limit).then_some(time)
}

pub fn step_projectiles(simulation: &mut Simulation, dt: f64, sweep_tank_motion: bool) {
    simulation.shots.retain(|shot| shot.life > 0.0);
    // Accelerate once per fixed tick, before all continuous collision sweeps.
    // Contact retries within this tick must not apply thrust again.
    for i in 0..simulation.shots.len() {
        let life = simulation.shots[i].life;
        guide_tow_missile(simulation, i, dt.min(life));
    }
    let rocket_base_speed = weapon(Weapon::Rocket).speed * simulation.speed_tuning.bullet_speed;
    let rocket_top_speed = rocket_base_speed * COMBAT.rocket_top_speed_multiplier;
    let rocket_acceleration =
        (rocket_top_speed - rocket_base_speed) / COMBAT.rocket_acceleration_seconds;
    for shot in simulation
        .shots
        .iter_mut()
        .filter(|shot| shot.weapon == Weapon::Rocket)
    {
        let speed = shot.vx.hypot(shot.vz);
        if speed > 0.0 && speed < rocket_top_speed {
            let scale =
                rocket_top_speed.min(speed + rocket_acceleration * dt.min(shot.life)) / speed;
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
    let mut tank_positions = Vec::with_capacity(simulation.tanks.len());
    while remaining > COMBAT.contact_time_epsilon && !simulation.shots.is_empty() && event < budget
    {
        let (next, time) = find_next_contact(
            simulation,
            &mut tank_positions,
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
        }
        if let Some(moves) = &mut simulation.projectile_moves {
            moves.extend(simulation.shots.iter().map(|shot| ProjectileMove {
                shot: shot.clone(),
                seconds: time,
                offset,
            }));
        }
        remaining -= time;
        let Some(next) = next else {
            break;
        };
        let shot_id = simulation.shots[next.shot()].id;
        let fraction = if sweep_tank_motion {
            (dt - remaining) / dt
        } else {
            1.0
        };
        if resolve_contact(simulation, next, fraction)
            && let Some(index) = simulation.shots.iter().position(|shot| shot.id == shot_id)
        {
            simulation.shots.remove(index);
        }
        event += 1;
    }
}

/// The earliest contact found for one shell; indices are valid until something moves.
#[derive(Clone, Copy, Debug)]
enum Contact {
    World {
        shot: usize,
        collider: ColliderHandle,
        normal: Vec2,
    },
    Debris {
        shot: usize,
        fragment: usize,
    },
    Tank {
        shot: usize,
        tank: usize,
    },
    Mine {
        shot: usize,
        mine: usize,
    },
    Pair {
        shot: usize,
        other: usize,
    },
    Laser {
        shot: usize,
        tank: usize,
    },
    Expiry {
        shot: usize,
    },
}

impl Contact {
    fn shot(self) -> usize {
        match self {
            Contact::World { shot, .. }
            | Contact::Debris { shot, .. }
            | Contact::Tank { shot, .. }
            | Contact::Mine { shot, .. }
            | Contact::Pair { shot, .. }
            | Contact::Laser { shot, .. }
            | Contact::Expiry { shot } => shot,
        }
    }
}

/// Query without moving entities; equal-time contacts preserve the original priority order.
/// `tank_positions` is scratch space reused across the queries of one tick.
fn find_next_contact(
    simulation: &Simulation,
    tank_positions: &mut Vec<Option<Point3>>,
    limit: f64,
    elapsed: f64,
    tank_frame_delta: f64,
    defenses: bool,
) -> (Option<Contact>, f64) {
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
    for (si, shot) in simulation.shots.iter().enumerate() {
        if shot.life <= time {
            time = shot.life;
            next = Some(Contact::Expiry { shot: si });
        }
        let speed = shot.vx.hypot(shot.vz);
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
                next = Some(Contact::World {
                    shot: si,
                    collider,
                    normal: Vec2::new(hit.normal.x as f64, hit.normal.z as f64),
                });
            }
            let options = ShapeCastOptions {
                max_time_of_impact: time as f32,
                target_distance: 0.0,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            };
            // Only a fragment's collider counts as a debris contact, so without fragments the
            // cast cannot change the result.
            if !simulation.fragments.is_empty()
                && let Some((collider, hit)) = simulation.world.cast_shape(
                    &Pose::from_translation(vector(shot.x, shot.combat_y(), shot.z)),
                    vector(shot.vx, 0.0, shot.vz),
                    &shell_shape,
                    options,
                    query_filter(group::DEBRIS_QUERY),
                )
                && hit.time_of_impact as f64 <= time
                && let Some(fragment) = simulation.fragments.iter().position(|fragment| {
                    simulation.world.bodies[fragment.body].colliders().first() == Some(&collider)
                })
            {
                time = hit.time_of_impact as f64;
                next = Some(Contact::Debris { shot: si, fragment });
            }
        }
        let probe = ShotProbe::from(shot);
        for (ti, tank) in simulation.tanks.iter().enumerate() {
            if let Some(contact) = tank_hit_time(
                simulation,
                &probe,
                tank,
                time,
                elapsed,
                tank_frame_delta,
                tank_positions[ti],
            ) && (contact < time || next.is_none())
            {
                time = contact;
                next = Some(Contact::Tank { shot: si, tank: ti });
            }
            if defenses
                && tank.laser > 0.0
                && let Some(laser) =
                    laser_contact_time(simulation, shot, tank, time, elapsed, tank_frame_delta)
                && (laser < time || next.is_none())
            {
                time = laser;
                next = Some(Contact::Laser { shot: si, tank: ti });
            }
        }
        for (mi, mine) in simulation.mines.iter().enumerate() {
            if let Some(contact) = mine_hit_time(shot, mine, time)
                && (contact < time || next.is_none())
            {
                time = contact;
                next = Some(Contact::Mine { shot: si, mine: mi });
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
            next = Some(Contact::Pair { shot: i, other: j });
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

fn remove_shot(simulation: &mut Simulation, id: u32) {
    if let Some(index) = simulation.shots.iter().position(|shot| shot.id == id) {
        simulation.shots.remove(index);
    }
}

/// Apply one contact. Return whether its primary shot should be removed.
fn resolve_contact(simulation: &mut Simulation, next: Contact, fraction: f64) -> bool {
    let shot = simulation.shots[next.shot()].clone();
    let shot_color = weapon(shot.weapon).color;
    let mut remove = true;
    match next {
        Contact::Laser { shot: si, tank: ti } => {
            let tank_id = simulation.tanks[ti].id;
            simulation.shots[si].laser_checked_by.push(tank_id);
            remove = simulation.rng.next() < LASER_DEFENSE.chance;
            if remove {
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
        Contact::Pair {
            shot: si,
            other: oi,
        } => {
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
        Contact::Mine { mine: mi, .. } => {
            // Remove first so the blast cannot rediscover and detonate this mine twice.
            let mine = simulation.mines.remove(mi);
            explode(
                simulation,
                Vec2::new(mine.x, mine.z),
                MINE.blast_radius,
                mine.damage.unwrap_or(MINE.damage),
                shot.owner,
                shot.team,
                shot.owner_life,
                DamageCause::Mine,
            );
        }
        Contact::Debris { fragment: fi, .. } => {
            let fragment = &simulation.fragments[fi];
            let timber = fragment.timber_part.is_some();
            let fragment_color = fragment.color;
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
                explode(
                    simulation,
                    Vec2::new(shot.x, shot.z),
                    COMBAT.rocket_blast_radius,
                    shot.damage,
                    shot.owner,
                    shot.team,
                    shot.owner_life,
                    DamageCause::Rocket,
                );
            } else {
                hit_projectile_debris(simulation, fi, &shot, point);
            }
            let mut impact = impact_event(
                point.x,
                point.z,
                0.6,
                if timber { fragment_color } else { shot_color },
            );
            impact.height = Some(point.y);
            impact.cover_kind = timber.then_some(CoverKind::Timber);
            simulation.events.push(impact);
        }
        Contact::Tank { shot: si, tank: ti } => {
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
                explode(
                    simulation,
                    Vec2::new(shot.x, shot.z),
                    COMBAT.rocket_blast_radius,
                    shot.damage,
                    shot.owner,
                    shot.team,
                    shot.owner_life,
                    DamageCause::Rocket,
                );
            } else if simulation.tanks[ti].id != shot.owner {
                // A shell that ricochets back stops at its shooter, harmless like at an ally.
                let position = simulation.body_translation(simulation.tanks[ti].body);
                let speed = match shot.vx.hypot(shot.vz) {
                    0.0 => 1.0,
                    speed => speed,
                };
                damage_tank(
                    simulation,
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
        Contact::World {
            shot: si,
            collider,
            normal,
        } => {
            let cover = simulation.cover_by_collider.get(&collider).copied();
            if let Some(ci) = cover {
                if matches!(
                    simulation.covers[ci].kind,
                    CoverKind::Timber | CoverKind::Tower
                ) {
                    let speed = shot.vx.hypot(shot.vz);
                    if speed > 0.0 {
                        simulation.covers[ci].kick =
                            Some(Vec2::new(shot.vx / speed, shot.vz / speed));
                    }
                }
                hit_movable_cover(simulation, ci, &shot);
            }
            if shot.weapon == Weapon::Rocket {
                explode(
                    simulation,
                    Vec2::new(shot.x, shot.z),
                    COMBAT.rocket_blast_radius,
                    shot.damage,
                    shot.owner,
                    shot.team,
                    shot.owner_life,
                    DamageCause::Rocket,
                );
            } else if let Some(ci) = cover {
                damage_cover(
                    simulation,
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
            let chipped = cover.map(|ci| &simulation.covers[ci]).filter(|cover| {
                cover.alive
                    && matches!(
                        cover.kind,
                        CoverKind::Tree | CoverKind::Timber | CoverKind::Cargo
                    )
            });
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
        Contact::Expiry { .. } => {}
    }
    remove
}
