//! The local player's round statistics for the recap. These counters never affect combat
//! or RNG.

use serde::{Deserialize, Serialize};

use super::simulation::Simulation;
use super::types::{DamageCause, DamageSource};

/// A hard safety bound on remembered kill times, for custom stress worlds.
const MAX_RECENT_KILLS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatRecord {
    pub life_started: f64,
    pub longest_life: f64,
    pub recent_kills: Vec<f64>,
    pub busiest_minute: usize,
    pub multikill: usize,
    /// Tank id of the last killer; -1 when there is none.
    pub revenge_target: i64,
    pub revenge_kills: u32,
    pub clutch_kills: u32,
    pub posthumous_kills: u32,
    pub mine_kills: u32,
    pub cover_destroyed: u32,
    pub pickups: u32,
    pub shots: u32,
    pub direct_hits: u32,
    pub damage_taken: f64,
    pub shield_absorbed: f64,
}

impl Default for CombatRecord {
    fn default() -> Self {
        Self {
            life_started: 0.0,
            longest_life: 0.0,
            recent_kills: Vec::new(),
            busiest_minute: 0,
            multikill: 0,
            revenge_target: -1,
            revenge_kills: 0,
            clutch_kills: 0,
            posthumous_kills: 0,
            mine_kills: 0,
            cover_destroyed: 0,
            pickups: 0,
            shots: 0,
            direct_hits: 0,
            damage_taken: 0.0,
            shield_absorbed: 0.0,
        }
    }
}

pub fn record_death(simulation: &mut Simulation, victim: usize, owner: u32) {
    if !simulation.records(&simulation.tanks[victim]) {
        return;
    }
    let victim_id = simulation.tanks[victim].id;
    let elapsed = simulation.elapsed;
    let stats = &mut simulation.combat_record;
    stats.longest_life = stats.longest_life.max(elapsed - stats.life_started);
    stats.revenge_target = if owner == victim_id { -1 } else { owner as i64 };
}

/// Called only for credited enemy kills.
pub fn record_kill(
    simulation: &mut Simulation,
    killer: usize,
    victim: usize,
    owner_life: Option<u32>,
    source: Option<DamageSource>,
) {
    if !simulation.records(&simulation.tanks[killer]) {
        return;
    }
    let killer_tank = &simulation.tanks[killer];
    let current_life = killer_tank.alive && owner_life.is_none_or(|life| life == killer_tank.life);
    let clutch = killer_tank.hp <= simulation.max_health(killer_tank) * 0.25;
    let victim_id = simulation.tanks[victim].id as i64;
    let elapsed = simulation.elapsed;
    let stats = &mut simulation.combat_record;
    // Keep only a sliding minute, with a hard safety bound.
    stats.recent_kills.retain(|&time| elapsed - time < 60.0);
    stats.recent_kills.push(elapsed);
    if stats.recent_kills.len() > MAX_RECENT_KILLS {
        stats.recent_kills.remove(0);
    }
    stats.busiest_minute = stats.busiest_minute.max(stats.recent_kills.len());
    stats.multikill = stats
        .multikill
        .max(stats.recent_kills.iter().filter(|&&time| elapsed - time < 5.0).count());
    if !current_life {
        stats.posthumous_kills += 1;
    } else if clutch {
        stats.clutch_kills += 1;
    }
    if victim_id == stats.revenge_target {
        stats.revenge_kills += 1;
        stats.revenge_target = -1;
    }
    if source.is_some_and(|source| source.cause == DamageCause::Mine) {
        stats.mine_kills += 1;
    }
}

pub fn longest_life(simulation: &Simulation) -> f64 {
    let stats = &simulation.combat_record;
    stats.longest_life.max(if simulation.human().alive {
        simulation.elapsed - stats.life_started
    } else {
        0.0
    })
}
