//! The damage paths for tanks and cover, and blasts. Every tank or cover hit routes through
//! these helpers so team immunity, XP attribution and destruction chains stay consistent.

use rapier3d::prelude::{SharedShape, Vector};

use super::ammunition::clear_ammo;
use super::bot_strategy::notice_hit;
use super::combat_record::{record_death, record_kill};
use super::combat_rules::{BlastFalloff, COMBAT, MINE, StructureBlast};
use super::data::group;
use super::debris_physics::{blast_debris, blast_loose_debris};
use super::difficulty::difficulty_tuning;
use super::match_state::award_kill;
use super::math::{Point3, Vec2, distance};
use super::movable_cover::moved_cover_region;
use super::physics::{from_rotation, from_vector, interaction_groups, vector};
use super::scenery_pieces::{CoverPose, break_scenery};
use super::simulation::{GameMode, Simulation};
use super::simulation_rules::{SIMULATION_RULES, SOLO};
use super::structure_physics::{
    BLAST_CORE, BLAST_HEIGHT, HE_BLAST_RADIUS, STRUCTURE_BLAST_REACH, loose_throw,
    throw_structure_debris,
};
use super::tank_destruction::tank_burnout;
use super::tree_proportions::tree_proportions;
use super::types::{
    CoverKind, DamageCause, DamageSource, DeathStyle, FragmentShape, Mine, SimEvent,
    SimEventType, Team, VehicleKind,
};
use super::veterancy::{KILL_XP, earn_experience};
use super::wrecks::break_tank;

/// The upright footprint a felled tree leaves on its cover body: the stump's shape and
/// its offset from the body origin (the trunk's mid-height).
pub struct TreeStump {
    pub shape: SharedShape,
    pub offset: Vector,
}

/// How a blast from outside (a drum, a rocket, a mine) meets a structure's standing parts:
/// a drum's over its whole radius, the others' over [`STRUCTURE_BLAST_REACH`] of theirs.
fn outside_structure_blast(cause: DamageCause, radius: f64, damage: f64) -> StructureBlast {
    if cause == DamageCause::Drum {
        return COMBAT.drum_structure_blast;
    }
    StructureBlast {
        pressure: damage,
        falloff: BlastFalloff::Even,
        reach: radius * STRUCTURE_BLAST_REACH,
        shatter: BLAST_CORE,
        throw: loose_throw(damage),
    }
}

pub fn tree_stump(x: f64, z: f64, w: f64, d: f64, h: f64) -> TreeStump {
    let stump_radius = tree_proportions(x, z, w, d, h).stump_radius;
    TreeStump {
        shape: SharedShape::cylinder(0.8, stump_radius as f32),
        offset: vector(0.0, 0.8 - h / 2.0, 0.0),
    }
}

impl Simulation {
    /// Damage a tank: team immunity, shields, XP attribution, kills and its wreck.
    pub fn damage_tank(
        &mut self,
        tank_index: usize,
        amount: f64,
        owner: u32,
        team: Team,
        owner_life: Option<u32>,
        source: Option<DamageSource>,
    ) {
        let mut amount = amount;
        let tank = &self.tanks[tank_index];
        if !tank.alive || tank.protection > 0.0 || (tank.team == team && tank.id != owner) {
            return;
        }
        let attacker = self.tank_index(owner);
        if self.multiplayer()
            && attacker.is_some_and(|a| !self.tanks[a].human && self.tanks[a].team == team)
        {
            amount *= difficulty_tuning(self.difficulty).damage;
        }
        if !self.multiplayer() && team != self.human_team && tank.team == self.human_team {
            amount *= difficulty_tuning(self.difficulty).damage;
        }
        if self.game_mode == GameMode::Solo && team != self.human_team {
            amount *= SOLO.enemy_damage_multiplier;
        }
        let records = self.records(tank);
        let elapsed = self.elapsed;
        // Before the shield: a shell it absorbs still shows a bot who fired it.
        if amount > 0.0 {
            notice_hit(self, tank_index, owner, source);
        }
        let tank = &mut self.tanks[tank_index];
        if amount > 0.0 {
            tank.last_combat = elapsed;
        }
        if tank.shield > 0.0 && tank.shield_points > 0.0 {
            let absorbed = amount.min(tank.shield_points);
            tank.shield_points -= absorbed;
            if records {
                self.combat_record.shield_absorbed += absorbed;
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
            self.combat_record.damage_taken += hull_damage;
        }
        if let Some(a) = attacker
            && self.tanks[a].team == team
            && self.tanks[a].team != victim_team
            && hull_damage > 0.0
        {
            self.tanks[a].damage_dealt += hull_damage;
            let kill_bonus = if victim_hp <= 0.0 { KILL_XP } else { 0.0 };
            earn_experience(self, a, hull_damage + kill_bonus, owner_life);
        }
        let position = self.body_translation(self.tanks[tank_index].body);
        if self.tanks[tank_index].hp > 0.0 {
            if amount > 0.0 {
                let mut hurt = SimEvent::at(SimEventType::Hurt, position.x, position.z);
                hurt.damage_source = source;
                hurt.id = Some(victim_id);
                hurt.owner = Some(owner);
                hurt.team = Some(victim_team);
                hurt.size = Some(amount);
                self.events.push(hurt);
            }
            return;
        }
        self.tanks[tank_index].hp = 0.0;
        record_death(self, tank_index, owner);
        let tank = &mut self.tanks[tank_index];
        tank.alive = false;
        tank.laser = 0.0;
        tank.laser_recharge = 0.0;
        clear_ammo(tank);
        tank.deaths += 1;
        tank.life += 1;
        tank.respawn = SIMULATION_RULES.respawn_seconds;
        tank.previous = position.planar();
        let (life, kind) = (tank.life, tank.kind);
        if let Some(killer) = attacker
            && killer != tank_index
            && self.tanks[killer].team != victim_team
        {
            self.tanks[killer].kills += 1;
            record_kill(self, killer, tank_index, owner_life, source);
            // Old ordnance counts toward the round, never toward a replacement life.
            let killer_tank = &mut self.tanks[killer];
            if killer_tank.alive && owner_life.is_none_or(|life| life == killer_tank.life) {
                killer_tank.life_kills += 1;
                killer_tank.best_life_kills =
                    killer_tank.best_life_kills.max(killer_tank.life_kills);
            }
        }
        if self.game_mode == GameMode::Team {
            let allow_victory = !self.endless_match;
            award_kill(
                &mut self.match_state,
                victim_team,
                team,
                owner == victim_id,
                allow_victory,
            );
        }
        self.check_solo_result();
        let burnout = tank_burnout(self.seed, victim_id, life);
        if !burnout {
            blast_debris(self, position.planar(), 3.0, 60.0);
            let point = Point3::new(position.x, BLAST_HEIGHT, position.z);
            for cover in 0..self.covers.len() {
                self.structure_blast_with(
                    cover,
                    point,
                    COMBAT.death_structure_blast,
                    Some((owner, team)),
                );
            }
        }
        break_tank(self, tank_index, burnout);
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
        self.events.push(death);
    }

    /// Damage a cover: impact marks, destruction, debris, rubble and drum chains.
    pub fn damage_cover(
        &mut self,
        cover_index: usize,
        amount: f64,
        owner: u32,
        team: Team,
        owner_life: Option<u32>,
        impact: Option<Point3>,
    ) {
        if let Some(cover) = self.covers.get(cover_index)
            && cover.structure.is_some()
        {
            // A structure takes damage where it lands, a burst on its parts there; with no
            // point every part takes it.
            let by = Some((owner, team));
            match impact {
                Some(point) => {
                    self.structure_blast_on(cover_index, point, HE_BLAST_RADIUS, amount, by)
                }
                None => self.damage_every_part(cover_index, amount, by),
            }
            return;
        }
        let cover = &mut self.covers[cover_index];
        if !cover.alive || !cover.destructible {
            return;
        }
        cover.hp -= amount;
        if cover.hp > 0.0 {
            return;
        }
        let body = cover.body;
        let pose = cover.motion.map(|_| {
            let rigid_body = &self.world.bodies[body];
            CoverPose {
                position: from_vector(rigid_body.translation()),
                rotation: from_rotation(*rigid_body.rotation()),
            }
        });
        let cover = &mut self.covers[cover_index];
        if let Some(pose) = pose {
            cover.x = pose.position.x;
            cover.z = pose.position.z;
        }
        cover.alive = false;
        self.destroyed += 1;
        if self
            .tanks
            .iter()
            .any(|tank| self.records(tank) && tank.id == owner && tank.team == team)
        {
            self.combat_record.cover_destroyed += 1;
        }
        for &collider in self.world.bodies[body].colliders() {
            self.cover_by_collider.remove(&collider);
        }
        let cover = &self.covers[cover_index];
        if cover.kind == CoverKind::Tree {
            // A tank-only upright footprint stops planar hulls climbing or crossing the
            // stump while shells can still fly through the space left by the crown.
            let stump = tree_stump(cover.x, cover.z, cover.w, cover.d, cover.h);
            let collider = &mut self.world.colliders[cover.collider];
            collider.set_shape(stump.shape);
            collider.set_translation_wrt_parent(stump.offset);
            collider.set_collision_groups(interaction_groups(group::STUMP_CONTACT));
        } else {
            self.remove_body(body);
        }
        let region = moved_cover_region(&self.covers[cover_index]);
        self.nav.rebuild(&self.covers, Some(region));
        let cover = &self.covers[cover_index];
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
        destroy.size = Some(2.0);
        destroy.color = Some(color);
        self.events.push(destroy);
        if !break_scenery(self, cover_index, pose) {
            for _ in 0..3 {
                let fx = x + self.rng.range(-w / 2.0, w / 2.0);
                let fz = z + self.rng.range(-d / 2.0, d / 2.0);
                let size = self.rng.range(0.3, 0.7);
                self.fragment(fx, fz, color, size, FragmentShape::Shard, 1.0);
            }
        }
        if kind == CoverKind::Drum {
            self.explode(
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

    /// A blast damaging tanks, cover and mines.
    #[allow(clippy::too_many_arguments)]
    pub fn explode(
        &mut self,
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
        self.events.push(explosion);
        blast_loose_debris(self, position, radius, damage);
        let throw = outside_structure_blast(cause, radius, damage).throw;
        throw_structure_debris(self, Point3::new(position.x, BLAST_HEIGHT, position.z), radius, throw);
        // Blast-triggered mines retain the initiator of this chain, like drums.
        let (chained, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.mines)
            .into_iter()
            .partition(|mine| distance(position, Vec2::new(mine.x, mine.z)) < radius);
        self.mines = rest;
        for mine in chained {
            self.detonate_mine(&mine, owner, team, owner_life);
        }
        for i in 0..self.tanks.len() {
            if !self.tanks[i].alive {
                continue;
            }
            let q = self.body_translation(self.tanks[i].body);
            let d = distance(position, q.planar());
            if d > radius {
                continue;
            }
            let factor = COMBAT.minimum_blast_damage_fraction.max(1.0 - d / radius);
            self.damage_tank(
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
            let tank = &self.tanks[i];
            if tank.alive && (tank.team != team || tank.id == owner) {
                let m = COMBAT.minimum_blast_distance.max(d);
                self.world.bodies[tank.body].apply_impulse(
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
        // original owner.
        let structure_blast = outside_structure_blast(cause, radius, damage);
        let point = Point3::new(position.x, BLAST_HEIGHT, position.z);
        let count = self.covers.len();
        for i in 0..count {
            if self.covers[i].structure.is_some() {
                self.structure_blast_with(i, point, structure_blast, Some((owner, team)));
                continue;
            }
            let cover = &mut self.covers[i];
            if cover.alive
                && cover.destructible
                && distance(position, Vec2::new(cover.x, cover.z))
                    < radius + cover.w.max(cover.d) * COMBAT.cover_blast_allowance
            {
                self.damage_cover(
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

    /// A removed mine's blast, credited to whoever set it off: its layer, a shell's owner
    /// or the initiator of a chain.
    pub(crate) fn detonate_mine(
        &mut self,
        mine: &Mine,
        owner: u32,
        team: Team,
        owner_life: Option<u32>,
    ) {
        self.explode(
            Vec2::new(mine.x, mine.z),
            MINE.blast_radius,
            mine.damage.unwrap_or(MINE.damage),
            owner,
            team,
            owner_life,
            DamageCause::Mine,
        );
    }
}
