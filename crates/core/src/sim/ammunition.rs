//! Ammunition choice, inventory and crate refills.

use super::data::{vehicle, weapon};
use super::types::{
    AmmoInventory, AmmoSelection, PickupKind, SpecialAmmo, Tank, VehicleKind, Weapon,
};

pub const AMMO_ORDER: [Weapon; 5] = [
    Weapon::Standard,
    Weapon::Spread,
    Weapon::Rocket,
    Weapon::Ricochet,
    Weapon::Piercing,
];
pub const PROJECTILE_ORDER: [Weapon; 6] = [
    Weapon::Standard,
    Weapon::Spread,
    Weapon::Rocket,
    Weapon::Ricochet,
    Weapon::Piercing,
    Weapon::Tow,
];

pub const AMMO_RESPAWN_SECONDS: f64 = 13.0;
pub const AMMO_SCROLL_INTERVAL_MS: f64 = 120.0;

pub fn empty_ammo() -> AmmoInventory {
    AmmoInventory::default()
}

/// Whether a crate kind carries special ammunition.
pub fn is_special_ammo(kind: PickupKind) -> bool {
    kind.special_ammo().is_some()
}

pub fn has_ammo_for(kind: VehicleKind, ammo: &AmmoInventory, selected: Weapon) -> bool {
    match selected {
        Weapon::Tow => vehicle(kind).weapon == Weapon::Tow,
        Weapon::Standard => vehicle(kind).weapon == Weapon::Standard,
        other => {
            vehicle(kind).weapon == Weapon::Standard
                && other.special().is_some_and(|s| ammo.get(s) > 0.0)
        }
    }
}

pub fn has_ammo(tank: &Tank, selected: Weapon) -> bool {
    has_ammo_for(tank.kind, &tank.ammo, selected)
}

pub fn has_advanced_ammo(tank: &Tank) -> bool {
    tank.ammo.values().iter().any(|&count| count > 0.0)
}

pub fn can_collect_ammo(tank: &Tank, kind: SpecialAmmo, multiplier: f64) -> bool {
    vehicle(tank.kind).weapon == Weapon::Standard
        && tank.ammo.get(kind) < weapon(kind.weapon()).carry_limit * multiplier
}

pub fn equipped_weapon(tank: &Tank) -> Weapon {
    equipped_weapon_for(tank.kind, &tank.ammo, tank.selected_ammo)
}

/// The weapon a tank fires now: its fixed gun, or the selected special ammo while any
/// remains (also for tanks known only by their replicated values).
pub fn equipped_weapon_for(kind: VehicleKind, ammo: &AmmoInventory, selected: Weapon) -> Weapon {
    let primary = vehicle(kind).weapon;
    if primary != Weapon::Standard {
        return primary;
    }
    if has_ammo_for(kind, ammo, selected) && selected != Weapon::Tow {
        selected
    } else {
        Weapon::Standard
    }
}

pub fn select_ammo(tank: &mut Tank, selection: Option<AmmoSelection>) {
    if !tank.alive {
        return;
    }
    tank.selected_ammo = equipped_weapon(tank);
    if tank.selected_ammo == Weapon::Tow {
        return;
    }
    match selection {
        Some(AmmoSelection::Weapon(chosen)) => {
            if has_ammo(tank, chosen) {
                tank.selected_ammo = chosen;
            }
        }
        Some(AmmoSelection::Step(step)) if step != 0 => {
            if let Some(next) = step_ammo(tank.kind, &tank.ammo, tank.selected_ammo, step) {
                tank.selected_ammo = next;
            }
        }
        _ => {}
    }
}

/// The first weapon `step` places away from `current` along [`AMMO_ORDER`], wrapping
/// around, that `kind` can fire with `ammo`. A weapon outside the order starts before
/// its first entry.
pub fn step_ammo(
    kind: VehicleKind,
    ammo: &AmmoInventory,
    current: Weapon,
    step: i8,
) -> Option<Weapon> {
    let count = AMMO_ORDER.len() as i32;
    let start = AMMO_ORDER
        .iter()
        .position(|&w| w == current)
        .map_or(-1, |i| i as i32);
    (1..=count)
        .map(|offset| AMMO_ORDER[(start + step as i32 * offset + count).rem_euclid(count) as usize])
        .find(|&candidate| has_ammo_for(kind, ammo, candidate))
}

pub fn consume_ammo(tank: &mut Tank, fired: Weapon) {
    let Some(kind) = fired.special() else {
        return;
    };
    *tank.ammo.get_mut(kind) -= 1.0;
    if !has_ammo(tank, fired) {
        tank.selected_ammo = Weapon::Standard;
    }
}

pub fn refill_ammo(tank: &mut Tank, kind: SpecialAmmo, multiplier: f64) -> f64 {
    let stats = weapon(kind.weapon());
    let received =
        (stats.per_crate * multiplier).min(stats.carry_limit * multiplier - tank.ammo.get(kind));
    *tank.ammo.get_mut(kind) += received;
    received
}

pub fn clear_ammo(tank: &mut Tank) {
    tank.ammo = empty_ammo();
    tank.selected_ammo = Weapon::Standard;
    tank.command.ammo_selection = None;
}
