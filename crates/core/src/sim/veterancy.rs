//! Per-life combat experience; bonuses apply equally to humans and bots.

use super::simulation::Simulation;
use super::types::{SimEvent, SimEventType};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rank {
    pub name: &'static str,
    pub xp: f64,
    pub damage: f64,
    pub fire_rate: f64,
    pub health: f64,
    pub repair: f64,
}

pub const RANKS: [Rank; 4] = [
    Rank {
        name: "Rookie",
        xp: 0.0,
        damage: 1.0,
        fire_rate: 1.0,
        health: 1.0,
        repair: 0.0,
    },
    Rank {
        name: "Veteran",
        xp: 300.0,
        damage: 1.1,
        fire_rate: 1.1,
        health: 1.1,
        repair: 0.0,
    },
    Rank {
        name: "Elite",
        xp: 750.0,
        damage: 1.2,
        fire_rate: 1.15,
        health: 1.15,
        repair: 0.01,
    },
    Rank {
        name: "Heroic",
        xp: 1500.0,
        damage: 1.3,
        fire_rate: 1.2,
        health: 1.2,
        repair: 0.02,
    },
];
pub const KILL_XP: f64 = 50.0;
pub const REPAIR_DELAY: f64 = 5.0;

/// The rank a tank with `xp` experience holds.
pub fn rank_index(xp: f64) -> usize {
    (1..RANKS.len()).rev().find(|&i| xp >= RANKS[i].xp).unwrap_or(0)
}

pub fn rank_stats(xp: f64) -> &'static Rank {
    &RANKS[rank_index(xp)]
}

pub fn earn_experience(simulation: &mut Simulation, tank_index: usize, amount: f64, owner_life: Option<u32>) {
    let tank = &simulation.tanks[tank_index];
    // A mine/shell from a destroyed tank must not promote its replacement.
    if !tank.alive || amount <= 0.0 || owner_life.is_some_and(|life| life != tank.life) {
        return;
    }
    let before = rank_index(tank.xp);
    let old_max = simulation.max_health(tank);
    let tank = &mut simulation.tanks[tank_index];
    tank.xp = RANKS[RANKS.len() - 1].xp.min(tank.xp + amount);
    let after = rank_index(tank.xp);
    tank.highest_rank = tank.highest_rank.max(after);
    if after == before {
        return;
    }
    // Preserve the hull percentage: promotion is a capacity upgrade, not a full repair.
    let new_max = simulation.max_health(&simulation.tanks[tank_index]);
    let tank = &mut simulation.tanks[tank_index];
    tank.hp = new_max.min((tank.hp / old_max) * new_max);
    let reload_scale = RANKS[before].fire_rate / RANKS[after].fire_rate;
    tank.cooldown *= reload_scale;
    tank.brain.fire_delay *= reload_scale;
    let (id, team, body) = (tank.id, tank.team, tank.body);
    let position = simulation.body_translation(body);
    let mut event = SimEvent::at(SimEventType::Promotion, position.x, position.z);
    event.id = Some(id);
    event.team = Some(team);
    event.label = Some(format!("PROMOTED TO {}", RANKS[after].name.to_uppercase()));
    event.color = Some(0xffd477);
    simulation.events.push(event);
}

pub fn repair_veteran(simulation: &mut Simulation, tank_index: usize, dt: f64) {
    let tank = &simulation.tanks[tank_index];
    let rate = rank_stats(tank.xp).repair;
    if !tank.alive || rate == 0.0 || simulation.elapsed - tank.last_combat < REPAIR_DELAY {
        return;
    }
    let max = simulation.max_health(tank);
    let tank = &mut simulation.tanks[tank_index];
    tank.hp = max.min(tank.hp + max * rate * dt);
}
