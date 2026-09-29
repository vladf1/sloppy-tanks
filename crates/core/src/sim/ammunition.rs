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

pub const fn ammo_help(kind: Weapon) -> &'static str {
    match kind {
        Weapon::Standard => "Unlimited shells · stops at walls",
        Weapon::Spread => "Three shells per volley · best up close",
        Weapon::Rocket => "Accelerates in flight · explosive blast · can hurt you",
        Weapon::Ricochet => "High damage · bounces up to three times",
        Weapon::Piercing => "Passes through one enemy shell · stops at tanks and cover",
        Weapon::Tow => "Fast anti-tank missile · bot-only vehicle weapon",
    }
}

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
    let primary = vehicle(tank.kind).weapon;
    if primary != Weapon::Standard {
        return primary;
    }
    if has_ammo(tank, tank.selected_ammo) && tank.selected_ammo != Weapon::Tow {
        tank.selected_ammo
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
            let count = AMMO_ORDER.len() as i32;
            let start = AMMO_ORDER
                .iter()
                .position(|&w| w == tank.selected_ammo)
                .map_or(-1, |i| i as i32);
            for offset in 1..=count {
                let candidate =
                    AMMO_ORDER[(start + step as i32 * offset + count).rem_euclid(count) as usize];
                if has_ammo(tank, candidate) {
                    tank.selected_ammo = candidate;
                    break;
                }
            }
        }
        _ => {}
    }
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
