//! Crate collection.

use super::ammunition::{AMMO_RESPAWN_SECONDS, can_collect_ammo, has_advanced_ammo, refill_ammo, select_ammo};
use super::data::{LASER_DEFENSE, SHIELD_CAPACITY, pickup, weapon};
use super::simulation::Simulation;
use super::types::{AmmoSelection, PickupKind, SimEvent, SimEventType};

/// Give `pickup_index`'s crate to the tank if it can use it. Returns whether it was taken.
pub fn collect_pickup(simulation: &mut Simulation, tank_index: usize, pickup_index: usize) -> bool {
    let tank = &simulation.tanks[tank_index];
    let crate_ = &simulation.pickups[pickup_index];
    if !crate_.available || !tank.alive {
        return false;
    }
    let kind = crate_.kind;
    if kind == PickupKind::Repair && tank.hp >= simulation.max_health(tank) {
        return false;
    }
    let multiplier = simulation.ammo_crate_multiplier;
    if let Some(ammo) = kind.special_ammo()
        && !can_collect_ammo(tank, ammo, multiplier)
    {
        return false;
    }
    let records = simulation.records(tank);
    let max_health = simulation.max_health(tank);
    let crate_ = &mut simulation.pickups[pickup_index];
    crate_.available = false;
    if records {
        simulation.combat_record.pickups += 1;
    }
    crate_.cooldown = if kind == PickupKind::Laser {
        LASER_DEFENSE.respawn
    } else {
        AMMO_RESPAWN_SECONDS
    };
    crate_.cooldown_duration = crate_.cooldown;
    let (x, z) = (crate_.x, crate_.z);
    let stats = pickup(kind);
    let duration = stats.duration * simulation.power_up_duration_multiplier;
    let tank = &mut simulation.tanks[tank_index];
    let mut label = stats.name.to_string();
    match kind {
        PickupKind::Spread | PickupKind::Rocket | PickupKind::Ricochet | PickupKind::Piercing => {
            let ammo = kind.special_ammo().expect("ammunition crate");
            let should_auto_select = tank.human && !has_advanced_ammo(tank);
            label = format!("+{} {}", refill_ammo(tank, ammo, multiplier), weapon(ammo.weapon()).unit);
            if should_auto_select {
                select_ammo(tank, Some(AmmoSelection::Weapon(ammo.weapon())));
            }
        }
        PickupKind::Repair => tank.hp = max_health,
        PickupKind::Shield => {
            tank.shield = duration;
            tank.shield_points = SHIELD_CAPACITY;
        }
        PickupKind::Rapid => tank.rapid = duration,
        PickupKind::Speed => tank.speed = duration,
        PickupKind::Laser => {
            tank.laser = duration;
            label = "LASER DEFENSE".to_string();
        }
    }
    let mut event = SimEvent::at(SimEventType::Pickup, x, z);
    event.id = Some(tank.id);
    event.team = Some(tank.team);
    event.label = Some(label);
    event.color = Some(stats.color);
    simulation.events.push(event);
    true
}
