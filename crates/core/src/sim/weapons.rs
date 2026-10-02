//! Firing: muzzle placement, reloads and the launched rounds.

use rapier3d::prelude::Ray;

use super::ammunition::{consume_ammo, equipped_weapon};
use super::combat_rules::COMBAT;
use super::data::{PLAYER_FIRE_RATE_MULTIPLIER, TEAM_COLORS, group, weapon};
use super::hitboxes::{ShotProbe, tank_hit_time};
use super::humvee_tactics::withdraw_humvee;
use super::math::Vec2;
use super::physics::{query_filter, vector};
use super::simulation::Simulation;
use super::tank_dimensions::{tank_muzzle, tank_visual_muzzle};
use super::types::{Shot, SimEvent, SimEventType, Tank, VehicleKind, Weapon};
use super::veterancy::rank_stats;

// Preserve the combat API used by tests and harnesses.
pub use super::mines::{place_mine, step_mines};
pub use super::pickups::collect_pickup;
pub use super::projectiles::{interception_time, step_projectiles};

/// Height of the hull origin above the ground-contact frame the muzzle offsets use.
const MUZZLE_FRAME_OFFSET: f64 = 0.4;

pub fn fire_weapon(simulation: &mut Simulation, tank_index: usize) {
    let tank = &simulation.tanks[tank_index];
    if !tank.alive || tank.cooldown > 0.0 {
        return;
    }
    let position = simulation.body_translation(tank.body);
    let fired = equipped_weapon(tank);
    let stats = weapon(fired);
    let muzzle = tank_muzzle(tank.kind);
    let visual_muzzle = tank_visual_muzzle(tank.kind);
    let muzzle_height = position.y - MUZZLE_FRAME_OFFSET + muzzle.y;
    let visual_muzzle_height = position.y - MUZZLE_FRAME_OFFSET + visual_muzzle.y;
    let tow_target = (fired == Weapon::Tow)
        .then(|| {
            simulation.tanks.iter().find(|candidate| {
                candidate.id == tank.brain.target && candidate.alive && candidate.team != tank.team
            })
        })
        .flatten()
        .map(|target| (target.id, target.life, target.body));
    // Only launch at a visible enemy; rejected requests must not consume a reload.
    if fired == Weapon::Tow {
        let visible = tow_target.is_some_and(|(_, _, body)| {
            simulation.visible(
                position.planar(),
                simulation.body_translation(body).planar(),
            )
        });
        if !visible {
            return;
        }
    }
    let elapsed = simulation.elapsed;
    let interval = weapon_interval(tank);
    let tank = &mut simulation.tanks[tank_index];
    tank.protection = 0.0;
    tank.last_combat = elapsed;
    tank.cooldown = interval;
    tank.recoil = 1.0;
    let (aim, id, team, life, xp, human, kind) = (
        tank.aim, tank.id, tank.team, tank.life, tank.xp, tank.human, tank.kind,
    );
    let direction = Vec2::new(aim.sin(), aim.cos());
    // Trace to the muzzle so a barrel poking into cover or a tank cannot shoot through it.
    let mut spawn_distance = muzzle.z;
    let ray = Ray::new(
        vector(position.x, muzzle_height, position.z),
        vector(direction.x, 0.0, direction.z),
    );
    if let Some((_, time)) = simulation.world.cast_ray(
        &ray,
        spawn_distance as f32,
        true,
        query_filter(group::COVER_QUERY),
    ) {
        spawn_distance = spawn_distance.min(time as f64);
    }
    let probe = ShotProbe {
        x: position.x,
        y: Some(muzzle_height),
        z: position.z,
        vx: direction.x,
        vz: direction.z,
        ignored: Some(id),
    };
    for target in &simulation.tanks {
        if let Some(hit) = tank_hit_time(simulation, &probe, target, spawn_distance, 0.0, 0.0, None)
        {
            spawn_distance = spawn_distance.min(hit);
        }
    }
    if spawn_distance < muzzle.z {
        spawn_distance = 0f64.max(spawn_distance - COMBAT.muzzle_clearance);
    }
    let offsets: &[f64] = if fired == Weapon::Spread {
        &[-COMBAT.spread_angle, 0.0, COMBAT.spread_angle]
    } else {
        &[0.0]
    };
    let records = simulation.records(&simulation.tanks[tank_index]);
    let bullet_speed = simulation.speed_tuning.bullet_speed;
    for offset in offsets {
        let angle = aim + offset;
        let shot_id = simulation.next_id;
        simulation.next_id += 1;
        simulation.shots.push(Shot {
            id: shot_id,
            x: position.x + direction.x * spawn_distance,
            z: position.z + direction.z * spawn_distance,
            y: Some(muzzle_height),
            recap_hit: false,
            visual_y: Some(visual_muzzle_height),
            target_id: tow_target.map(|(target, _, _)| target),
            target_life: tow_target.map(|(_, target_life, _)| target_life),
            owner: id,
            owner_life: Some(life),
            team,
            vx: angle.sin() * stats.speed * bullet_speed,
            vz: angle.cos() * stats.speed * bullet_speed,
            damage: stats.damage * rank_stats(xp).damage,
            bounces: stats.bounces,
            ricocheted: false,
            // Rockets accelerate during flight; all rounds share the expiry limit.
            life: COMBAT.projectile_lifetime,
            weapon: fired,
            piercing: if fired == Weapon::Piercing { 1 } else { 0 },
            pierced_shot: None,
            laser_checked_by: Vec::new(),
        });
        simulation.shots_fired += 1;
        if records {
            simulation.combat_record.shots += 1;
        }
    }
    consume_ammo(&mut simulation.tanks[tank_index], fired);
    if kind == VehicleKind::Humvee && !human {
        withdraw_humvee(simulation, tank_index);
    }
    if human
        && fired != Weapon::Standard
        && simulation.tanks[tank_index].selected_ammo == Weapon::Standard
    {
        let mut notice = SimEvent::at(SimEventType::Notice, position.x, position.z);
        notice.id = Some(id);
        notice.label = Some(format!(
            "{} EMPTY — switched to STANDARD (unlimited)",
            stats.label
        ));
        simulation.events.push(notice);
    }
    let mut shot = SimEvent::at(
        SimEventType::Shot,
        position.x + direction.x * muzzle.z,
        position.z + direction.z * muzzle.z,
    );
    shot.weapon = Some(fired);
    shot.id = Some(id);
    shot.team = Some(team);
    shot.size = Some(if fired == Weapon::Rocket { 1.5 } else { 1.0 });
    shot.color = Some(TEAM_COLORS[team.index()]);
    simulation.events.push(shot);
}

/// Seconds between this tank's shots with its equipped weapon.
pub fn weapon_interval(tank: &Tank) -> f64 {
    (weapon(equipped_weapon(tank)).interval
        * if tank.rapid > 0.0 {
            COMBAT.rapid_reload_multiplier
        } else {
            1.0
        })
        / ((if tank.human {
            PLAYER_FIRE_RATE_MULTIPLIER
        } else {
            1.0
        }) * rank_stats(tank.xp).fire_rate)
}
