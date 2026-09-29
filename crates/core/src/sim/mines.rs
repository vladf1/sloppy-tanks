//! Proximity mines.

use super::combat_rules::MINE;
use super::damage::explode;
use super::math::{Vec2, distance};
use super::simulation::Simulation;
use super::types::{DamageCause, Mine};
use super::veterancy::rank_stats;

pub fn place_mine(simulation: &mut Simulation, tank_index: usize) {
    let tank = &simulation.tanks[tank_index];
    if tank.mine_cooldown > 0.0 || !tank.alive {
        return;
    }
    let position = simulation.body_translation(tank.body);
    let id = simulation.next_id;
    simulation.next_id += 1;
    let tank = &simulation.tanks[tank_index];
    let mine = Mine {
        id,
        owner: tank.id,
        owner_life: Some(tank.life),
        damage: Some(MINE.damage * rank_stats(tank.xp).damage),
        team: tank.team,
        x: position.x,
        z: position.z,
        arm: MINE.arm_seconds,
        life: MINE.lifetime_seconds,
    };
    simulation.mines.push(mine);
    let elapsed = simulation.elapsed;
    let tank = &mut simulation.tanks[tank_index];
    tank.mine_cooldown = MINE.cooldown_seconds;
    tank.last_combat = elapsed;
}

pub fn step_mines(simulation: &mut Simulation, dt: f64) {
    // A detonation can recursively remove other mines. Iterate stable identities, not
    // mutable indices.
    let ids: Vec<u32> = simulation.mines.iter().map(|mine| mine.id).collect();
    for id in ids {
        let Some(index) = simulation.mines.iter().position(|mine| mine.id == id) else {
            continue;
        };
        let mine = &mut simulation.mines[index];
        mine.arm -= dt;
        mine.life -= dt;
        let mine = mine.clone();
        let at = Vec2::new(mine.x, mine.z);
        let triggered = mine.arm <= 0.0
            && simulation.tanks.iter().any(|tank| {
                tank.alive
                    && tank.team != mine.team
                    && distance(simulation.body_translation(tank.body).planar(), at) < MINE.trigger_radius
            });
        if triggered {
            simulation.mines.remove(index);
            explode(
                simulation,
                at,
                MINE.blast_radius,
                mine.damage.unwrap_or(MINE.damage),
                mine.owner,
                mine.team,
                mine.owner_life,
                DamageCause::Mine,
            );
        } else if mine.life <= 0.0 {
            simulation.mines.remove(index);
        }
    }
}
