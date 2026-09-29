//! The damage paths for tanks and cover, and blasts. Every tank or cover hit routes through
//! these helpers so team immunity, XP attribution and destruction chains stay consistent.

use rapier3d::prelude::SharedShape;

use super::ammunition::clear_ammo;
use super::arena::CoverDef;
use super::combat_record::{record_death, record_kill};
use super::combat_rules::{COMBAT, MINE};
use super::data::group;
use super::debris_physics::blast_debris;
use super::difficulty::difficulty_tuning;
use super::match_state::award_kill;
use super::math::{Point3, Vec2, distance};
use super::movable_cover::moved_cover_region;
use super::physics::{from_rotation, from_vector, interaction_groups, vector};
use super::scenery_pieces::{CoverPose, break_scenery};
use super::simulation::{GameMode, Simulation};
use super::simulation_rules::{SIMULATION_RULES, SOLO};
use super::tank_destruction::tank_burnout;
use super::timber_layout::TimberHit;
use super::tower_layout::TOWER_BASE;
use super::tree_proportions::tree_proportions;
use super::types::{
    CoverKind, DamageCause, DamageSource, DeathStyle, FragmentShape, SimEvent, SimEventType, Team,
    VehicleKind,
};
use super::veterancy::{KILL_XP, earn_experience};
use super::wrecks::break_tank;

/// Remembered impact marks per timber wall.
const MAX_TIMBER_MARKS: usize = 6;

#[allow(clippy::too_many_arguments)]
pub fn damage_tank(
    simulation: &mut Simulation,
    tank_index: usize,
    amount: f64,
    owner: u32,
    team: Team,
    owner_life: Option<u32>,
    source: Option<DamageSource>,
) {
    let mut amount = amount;
    let tank = &simulation.tanks[tank_index];
    if !tank.alive || tank.protection > 0.0 || (tank.team == team && tank.id != owner) {
        return;
    }
    let attacker = simulation.tank_index(owner);
    if simulation.multiplayer()
        && attacker.is_some_and(|a| !simulation.tanks[a].human && simulation.tanks[a].team == team)
    {
        amount *= difficulty_tuning(simulation.difficulty).damage;
    }
    if !simulation.multiplayer()
        && team != simulation.human_team
        && tank.team == simulation.human_team
    {
        amount *= difficulty_tuning(simulation.difficulty).damage;
    }
    if simulation.game_mode == GameMode::Solo && team != simulation.human_team {
        amount *= SOLO.enemy_damage_multiplier;
    }
    let records = simulation.records(tank);
    let elapsed = simulation.elapsed;
    let tank = &mut simulation.tanks[tank_index];
    if amount > 0.0 {
        tank.last_combat = elapsed;
    }
    if tank.shield > 0.0 && tank.shield_points > 0.0 {
        let absorbed = amount.min(tank.shield_points);
        tank.shield_points -= absorbed;
        if records {
            simulation.combat_record.shield_absorbed += absorbed;
        }
        amount -= absorbed;
        if tank.shield_points == 0.0 {
            tank.shield = 0.0;
        }
    }
    let hull_damage = tank.hp.min(0f64.max(amount));
    tank.hp -= amount;
    let (victim_team, victim_id, victim_hp) = (tank.team, tank.id, tank.hp);
    if records {
        simulation.combat_record.damage_taken += hull_damage;
    }
    if let Some(a) = attacker
        && simulation.tanks[a].team == team
        && simulation.tanks[a].team != victim_team
        && hull_damage > 0.0
    {
        simulation.tanks[a].damage_dealt += hull_damage;
        let kill_bonus = if victim_hp <= 0.0 { KILL_XP } else { 0.0 };
        earn_experience(simulation, a, hull_damage + kill_bonus, owner_life);
    }
    let position = simulation.body_translation(simulation.tanks[tank_index].body);
    if simulation.tanks[tank_index].hp > 0.0 {
        if amount > 0.0 {
            let mut hurt = SimEvent::at(SimEventType::Hurt, position.x, position.z);
            hurt.damage_source = source;
            hurt.id = Some(victim_id);
            hurt.owner = Some(owner);
            hurt.team = Some(victim_team);
            hurt.size = Some(amount);
            simulation.events.push(hurt);
        }
        return;
    }
    simulation.tanks[tank_index].hp = 0.0;
    record_death(simulation, tank_index, owner);
    let tank = &mut simulation.tanks[tank_index];
    tank.alive = false;
    tank.laser = 0.0;
    clear_ammo(tank);
    tank.deaths += 1;
    tank.life += 1;
    tank.respawn = SIMULATION_RULES.respawn_seconds;
    tank.previous = position.planar();
    let (life, kind) = (tank.life, tank.kind);
    if let Some(killer) = attacker
        && killer != tank_index
        && simulation.tanks[killer].team != victim_team
    {
        simulation.tanks[killer].kills += 1;
        record_kill(simulation, killer, tank_index, owner_life, source);
        // Old ordnance counts toward the round, never toward a replacement life.
        let killer_tank = &mut simulation.tanks[killer];
        if killer_tank.alive && owner_life.is_none_or(|life| life == killer_tank.life) {
            killer_tank.life_kills += 1;
            killer_tank.best_life_kills = killer_tank.best_life_kills.max(killer_tank.life_kills);
        }
    }
    if simulation.game_mode == GameMode::Team {
        let allow_victory = !simulation.endless_match;
        award_kill(
            &mut simulation.match_state,
            victim_team,
            team,
            owner == victim_id,
            allow_victory,
        );
    }
    simulation.check_solo_result();
    let burnout = tank_burnout(simulation.seed, victim_id, life);
    if !burnout {
        blast_debris(simulation, position.planar(), 3.0, 60.0);
    }
    break_tank(simulation, tank_index, burnout);
    let mut death = SimEvent::at(SimEventType::Death, position.x, position.z);
    death.death_style = burnout.then_some(DeathStyle::Burnout);
    death.damage_source = source;
    death.id = Some(victim_id);
    death.owner = Some(owner);
    death.owner_life = owner_life;
    death.team = Some(victim_team);
    death.size = Some(match kind {
        VehicleKind::Scout => 2.6,
        VehicleKind::Heavy => 3.6,
        _ => 3.0,
    });
    simulation.events.push(death);
}

#[allow(clippy::too_many_arguments)]
pub fn damage_cover(
    simulation: &mut Simulation,
    cover_index: usize,
    amount: f64,
    owner: u32,
    team: Team,
    owner_life: Option<u32>,
    impact: Option<Point3>,
) {
    let cover = &mut simulation.covers[cover_index];
    if !cover.alive || !cover.destructible {
        return;
    }
    let previous_hp = cover.hp;
    cover.hp -= amount;
    if cover.hp > 0.0 {
        if cover.kind == CoverKind::Timber && amount > 0.0 {
            let marks = &mut cover.timber_hits;
            if marks.len() < MAX_TIMBER_MARKS {
                let size = if marks.is_empty() {
                    1.0
                } else {
                    1.8f64.min(1.5 + (marks.len() as f64 - 1.0) * 0.15)
                };
                marks.push(TimberHit {
                    x: impact.map_or(0.0, |point| point.x - cover.x),
                    y: impact.map_or_else(|| 1f64.min(cover.h * 0.5), |point| point.y),
                    z: impact.map_or(-cover.d / 2.0, |point| point.z - cover.z),
                    size,
                });
            }
        }
        return;
    }
    let body = cover.body;
    let pose = cover.motion.map(|_| {
        let rigid_body = &simulation.world.bodies[body];
        CoverPose {
            position: from_vector(rigid_body.translation()),
            rotation: from_rotation(*rigid_body.rotation()),
        }
    });
    let cover = &mut simulation.covers[cover_index];
    if let Some(pose) = pose {
        cover.x = pose.position.x;
        cover.z = pose.position.z;
    }
    cover.alive = false;
    simulation.destroyed += 1;
    if simulation
        .tanks
        .iter()
        .any(|tank| simulation.records(tank) && tank.id == owner && tank.team == team)
    {
        simulation.combat_record.cover_destroyed += 1;
    }
    for &collider in simulation.world.bodies[body].colliders() {
        simulation.cover_by_collider.remove(&collider);
    }
    let cover = &simulation.covers[cover_index];
    if cover.kind == CoverKind::Tree {
        // A tank-only upright footprint stops planar hulls climbing or crossing the
        // stump while shells can still fly through the space left by the crown.
        let stump_radius =
            tree_proportions(cover.x, cover.z, cover.w, cover.d, cover.h).stump_radius;
        let offset = 0.8 - cover.h / 2.0;
        let collider = &mut simulation.world.colliders[cover.collider];
        collider.set_shape(SharedShape::cylinder(0.8, stump_radius as f32));
        collider.set_translation_wrt_parent(vector(0.0, offset, 0.0));
        collider.set_collision_groups(interaction_groups(group::STUMP_CONTACT));
    } else {
        simulation.remove_body(body);
    }
    let region = moved_cover_region(&simulation.covers[cover_index]);
    simulation.nav.rebuild(&simulation.covers, Some(region));
    let cover = &simulation.covers[cover_index];
    let (kind, x, z, w, d, h, color, id) = (
        cover.kind,
        cover.x,
        cover.z,
        cover.w,
        cover.d,
        cover.h,
        cover.color,
        cover.id,
    );
    let mut destroy = SimEvent::at(SimEventType::Destroy, x, z);
    destroy.cover_kind = Some(kind);
    destroy.height = Some(h);
    destroy.id = Some(id);
    destroy.size = Some(if kind == CoverKind::Tower { 7.0 } else { 2.0 });
    destroy.color = Some(color);
    simulation.events.push(destroy);
    if !break_scenery(simulation, cover_index, pose, previous_hp) {
        for _ in 0..3 {
            let fx = x + simulation.rng.range(-w / 2.0, w / 2.0);
            let fz = z + simulation.rng.range(-d / 2.0, d / 2.0);
            let size = simulation.rng.range(0.3, 0.7);
            let shape = if kind == CoverKind::House {
                FragmentShape::Track
            } else {
                FragmentShape::Shard
            };
            simulation.fragment(fx, fz, color, size, shape, 1.0);
        }
    }
    if kind == CoverKind::Tower {
        // One authored support object; its destruction leaves two flank foundations and an
        // open middle.
        for side in [-1.0, 1.0] {
            let mut rubble = CoverDef::new(
                CoverKind::Rubble,
                x + side * TOWER_BASE.offset,
                z,
                TOWER_BASE.width,
                TOWER_BASE.depth,
                TOWER_BASE.rubble_height,
                f64::INFINITY,
                color,
            );
            rubble.debris_seed = Some((simulation.rng.next() * 4_294_967_296.0).floor());
            simulation.add_cover(&rubble);
        }
        let region = moved_cover_region(&simulation.covers[cover_index]);
        simulation.nav.rebuild(&simulation.covers, Some(region));
    }
    if kind == CoverKind::Drum {
        explode(
            simulation,
            Vec2::new(x, z),
            COMBAT.drum_blast_radius,
            COMBAT.drum_damage,
            owner,
            team,
            owner_life,
            DamageCause::Drum,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn explode(
    simulation: &mut Simulation,
    position: Vec2,
    radius: f64,
    damage: f64,
    owner: u32,
    team: Team,
    owner_life: Option<u32>,
    cause: DamageCause,
) {
    let mut explosion = SimEvent::at(SimEventType::Explosion, position.x, position.z);
    explosion.size = Some(radius);
    explosion.cover_kind = (cause == DamageCause::Drum).then_some(CoverKind::Drum);
    simulation.events.push(explosion);
    blast_debris(simulation, position, radius, damage);
    // Blast-triggered mines retain the initiator of this chain, like drums.
    let (chained, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut simulation.mines)
        .into_iter()
        .partition(|mine| distance(position, Vec2::new(mine.x, mine.z)) < radius);
    simulation.mines = rest;
    for mine in chained {
        explode(
            simulation,
            Vec2::new(mine.x, mine.z),
            MINE.blast_radius,
            mine.damage.unwrap_or(MINE.damage),
            owner,
            team,
            owner_life,
            DamageCause::Mine,
        );
    }
    for i in 0..simulation.tanks.len() {
        if !simulation.tanks[i].alive {
            continue;
        }
        let q = simulation.body_translation(simulation.tanks[i].body);
        let d = distance(position, q.planar());
        if d > radius {
            continue;
        }
        let factor = COMBAT.minimum_blast_damage_fraction.max(1.0 - d / radius);
        damage_tank(
            simulation,
            i,
            damage * factor,
            owner,
            team,
            owner_life,
            Some(DamageSource {
                cause,
                origin: position,
            }),
        );
        let tank = &simulation.tanks[i];
        if tank.alive && (tank.team != team || tank.id == owner) {
            let m = COMBAT.minimum_blast_distance.max(d);
            simulation.world.bodies[tank.body].apply_impulse(
                vector(
                    ((q.x - position.x) / m) * COMBAT.blast_impulse * factor,
                    0.0,
                    ((q.z - position.z) / m) * COMBAT.blast_impulse * factor,
                ),
                true,
            );
        }
    }
    // alive is cleared before recursion, so drums and chains are exactly-once and keep the
    // original owner. Rubble added during the loop is not part of this blast.
    let count = simulation.covers.len();
    for i in 0..count {
        let cover = &mut simulation.covers[i];
        if cover.alive
            && cover.destructible
            && distance(position, Vec2::new(cover.x, cover.z))
                < radius + cover.w.max(cover.d) * COMBAT.cover_blast_allowance
        {
            if cover.kind == CoverKind::Timber {
                let dx = cover.x - position.x;
                let dz = cover.z - position.z;
                let length = dx.hypot(dz);
                cover.timber_kick = (length > 0.001).then(|| Vec2::new(dx / length, dz / length));
            }
            damage_cover(
                simulation,
                i,
                damage,
                owner,
                team,
                owner_life,
                Some(Point3::new(position.x, 1.0, position.z)),
            );
        }
    }
}
