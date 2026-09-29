//! Page HUD records built from a `RenderTank`, the type both local play and the network
//! client draw. The field names match `Game::hud_json` (`human` and `scoreboard`), so one
//! page HUD renders either source.

use serde_json::{Value, json};
use sloppy_core::sim::ammunition::{AMMO_ORDER, has_ammo_for};
use sloppy_core::sim::data::vehicle;
use sloppy_core::sim::render_state::RenderTank;
use sloppy_core::sim::veterancy::{RANKS, REPAIR_DELAY, rank_index};
use sloppy_core::sim::{AmmoInventory, VehicleKind, Weapon};
use sloppy_render::presentation::hud::health_bar_state;

/// The weapon a tank fires now: its fixed gun, or the selected special ammo while any
/// remains (`equippedWeapon`, for tanks known only by their replicated values).
pub fn equipped_weapon_for(kind: VehicleKind, ammo: &AmmoInventory, selected: Weapon) -> Weapon {
    let primary = vehicle(kind).weapon;
    if primary != Weapon::Standard {
        primary
    } else if has_ammo_for(kind, ammo, selected) && selected != Weapon::Tow {
        selected
    } else {
        Weapon::Standard
    }
}

/// The viewer's HUD block (`hud_json().human`). `elapsed` is the match clock, which
/// decides whether veteran self-repair is running.
pub fn human_json(tank: &RenderTank, elapsed: f64) -> Value {
    let rank = rank_index(tank.xp);
    let stats = &RANKS[rank];
    let health = health_bar_state(tank.hp, tank.max_hp, tank.team);
    let equipped = equipped_weapon_for(tank.kind, &tank.ammo, tank.selected_ammo);
    let ammo: Vec<Value> = AMMO_ORDER
        .iter()
        .map(|&weapon| {
            json!({
                "weapon": weapon,
                "count": weapon.special().map(|kind| tank.ammo.get(kind)),
                "selected": weapon == equipped,
                "available": has_ammo_for(tank.kind, &tank.ammo, weapon),
            })
        })
        .collect();
    let self_repair = tank.alive
        && stats.repair > 0.0
        && tank.hp < tank.max_hp
        && elapsed - tank.last_combat >= REPAIR_DELAY;
    json!({
        "id": tank.id,
        "name": tank.name,
        "kind": tank.kind,
        "vehicleName": vehicle(tank.kind).name,
        "team": tank.team,
        "alive": tank.alive,
        "hp": tank.hp,
        "maxHp": tank.max_hp,
        "healthRatio": health.ratio,
        "healthColor": health.color,
        "xp": tank.xp,
        "rank": rank,
        "rankName": stats.name,
        "rankDamage": stats.damage,
        "rankFireRate": stats.fire_rate,
        "rankHealth": stats.health,
        "rankRepair": stats.repair,
        "selectedAmmo": tank.selected_ammo,
        "equipped": equipped,
        "ammo": ammo,
        "cooldown": tank.cooldown,
        "mineCooldown": tank.mine_cooldown,
        "protection": tank.protection,
        "shield": tank.shield,
        "shieldPoints": tank.shield_points,
        "rapid": tank.rapid,
        "speed": tank.speed,
        "laser": tank.laser,
        "respawn": tank.respawn,
        "kills": tank.kills,
        "deaths": tank.deaths,
        "selfRepair": self_repair,
    })
}

/// Every tank's name, side and tally (`hud_json().scoreboard`).
pub fn scoreboard_json(tanks: &[RenderTank]) -> Value {
    Value::Array(
        tanks
            .iter()
            .map(|tank| {
                json!({
                    "id": tank.id, "name": tank.name, "team": tank.team, "kind": tank.kind,
                    "human": tank.human, "alive": tank.alive, "kills": tank.kills,
                    "deaths": tank.deaths, "rank": rank_index(tank.xp),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_ammo_is_equipped_only_while_it_lasts() {
        let mut ammo = AmmoInventory::default();
        let kind = VehicleKind::Balanced;
        assert_eq!(
            equipped_weapon_for(kind, &ammo, Weapon::Rocket),
            Weapon::Standard
        );
        ammo.rocket = 2.0;
        assert_eq!(
            equipped_weapon_for(kind, &ammo, Weapon::Rocket),
            Weapon::Rocket
        );
        let tank = RenderTank {
            kind,
            ammo,
            selected_ammo: Weapon::Rocket,
            hp: 50.0,
            max_hp: 100.0,
            alive: true,
            ..RenderTank::default()
        };
        let human = human_json(&tank, 0.0);
        assert_eq!(human["equipped"], json!("rocket"));
        assert_eq!(human["healthRatio"], json!(0.5));
        assert_eq!(human["ammo"][2]["count"], json!(2.0));
    }
}
