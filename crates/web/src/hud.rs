//! Shared page HUD records and ammo, repair and scoreboard rules for local and room play.

use serde::{Serialize, Serializer};
use sloppy_core::sim::ammunition::{AMMO_ORDER, has_ammo_for};
use sloppy_core::sim::data::vehicle;
use sloppy_core::sim::render_state::RenderTank;
use sloppy_core::sim::veterancy::{RANKS, REPAIR_DELAY, rank_index};
use sloppy_core::sim::{AmmoInventory, Tank, Team, VehicleKind, Weapon};
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

pub(crate) fn hud_ammo(
    kind: VehicleKind,
    ammo: &AmmoInventory,
    equipped: Weapon,
) -> [HudAmmo; AMMO_ORDER.len()] {
    AMMO_ORDER.map(|weapon| HudAmmo {
        weapon,
        count: weapon.special().map(|kind| ammo.get(kind)),
        selected: weapon == equipped,
        available: has_ammo_for(kind, ammo, weapon),
    })
}

pub(crate) fn self_repair_active(
    alive: bool,
    repair: f64,
    hp: f64,
    max_hp: f64,
    since_combat: f64,
) -> bool {
    alive && repair > 0.0 && hp < max_hp && since_combat >= REPAIR_DELAY
}

/// The viewer's HUD block (`hud_json().human`). `elapsed` is the match clock, which
/// decides whether veteran self-repair is running.
pub fn human_json(tank: &RenderTank, elapsed: f64) -> HudHuman<'_> {
    let rank = rank_index(tank.xp);
    let stats = &RANKS[rank];
    let health = health_bar_state(tank.hp, tank.max_hp, tank.team);
    let equipped = equipped_weapon_for(tank.kind, &tank.ammo, tank.selected_ammo);
    let ammo = hud_ammo(tank.kind, &tank.ammo, equipped);
    let self_repair = self_repair_active(
        tank.alive,
        stats.repair,
        tank.hp,
        tank.max_hp,
        elapsed - tank.last_combat,
    );
    HudHuman {
        id: tank.id,
        name: &tank.name,
        kind: tank.kind,
        vehicle_name: vehicle(tank.kind).name,
        team: tank.team,
        alive: tank.alive,
        hp: tank.hp,
        max_hp: tank.max_hp,
        health_ratio: health.ratio,
        health_color: health.color,
        xp: tank.xp,
        rank,
        rank_name: stats.name,
        rank_damage: stats.damage,
        rank_fire_rate: stats.fire_rate,
        rank_health: stats.health,
        rank_repair: stats.repair,
        repair_delay: REPAIR_DELAY,
        selected_ammo: tank.selected_ammo,
        equipped,
        ammo,
        cooldown: tank.cooldown,
        mine_cooldown: tank.mine_cooldown,
        protection: tank.protection,
        shield: tank.shield,
        shield_points: tank.shield_points,
        rapid: tank.rapid,
        speed: tank.speed,
        laser: tank.laser,
        respawn: tank.respawn,
        kills: tank.kills,
        deaths: tank.deaths,
        self_repair,
    }
}

/// Every tank's name, side and tally (`hud_json().scoreboard`).
pub fn scoreboard_json(tanks: &[RenderTank]) -> Scoreboard<'_> {
    Scoreboard::Rendered(tanks)
}

pub enum Scoreboard<'a> {
    Rendered(&'a [RenderTank]),
    Simulated(&'a [Tank]),
}

impl Serialize for Scoreboard<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Rendered(tanks) => serializer.collect_seq(tanks.iter().map(|tank| HudScore {
                id: tank.id,
                name: &tank.name,
                team: tank.team,
                kind: tank.kind,
                human: tank.human,
                alive: tank.alive,
                kills: tank.kills,
                deaths: tank.deaths,
                rank: rank_index(tank.xp),
            })),
            Self::Simulated(tanks) => serializer.collect_seq(tanks.iter().map(|tank| HudScore {
                id: tank.id,
                name: &tank.name,
                team: tank.team,
                kind: tank.kind,
                human: tank.human,
                alive: tank.alive,
                kills: tank.kills,
                deaths: tank.deaths,
                rank: rank_index(tank.xp),
            })),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudHuman<'a> {
    pub(crate) id: u32,
    pub(crate) name: &'a str,
    pub(crate) kind: VehicleKind,
    pub(crate) vehicle_name: &'static str,
    pub(crate) team: Team,
    pub(crate) alive: bool,
    pub(crate) hp: f64,
    pub(crate) max_hp: f64,
    pub(crate) health_ratio: f64,
    pub(crate) health_color: u32,
    pub(crate) xp: f64,
    pub(crate) rank: usize,
    pub(crate) rank_name: &'static str,
    pub(crate) rank_damage: f64,
    pub(crate) rank_fire_rate: f64,
    pub(crate) rank_health: f64,
    pub(crate) rank_repair: f64,
    pub(crate) repair_delay: f64,
    pub(crate) selected_ammo: Weapon,
    pub(crate) equipped: Weapon,
    pub(crate) ammo: [HudAmmo; AMMO_ORDER.len()],
    pub(crate) cooldown: f64,
    pub(crate) mine_cooldown: f64,
    pub(crate) protection: f64,
    pub(crate) shield: f64,
    pub(crate) shield_points: f64,
    pub(crate) rapid: f64,
    pub(crate) speed: f64,
    pub(crate) laser: f64,
    pub(crate) respawn: f64,
    pub(crate) kills: u32,
    pub(crate) deaths: u32,
    pub(crate) self_repair: bool,
}

#[derive(Serialize)]
pub(crate) struct HudAmmo {
    pub(crate) weapon: Weapon,
    pub(crate) count: Option<f64>,
    pub(crate) selected: bool,
    pub(crate) available: bool,
}

#[derive(Serialize)]
pub(crate) struct HudScore<'a> {
    pub(crate) id: u32,
    pub(crate) name: &'a str,
    pub(crate) team: Team,
    pub(crate) kind: VehicleKind,
    pub(crate) human: bool,
    pub(crate) alive: bool,
    pub(crate) kills: u32,
    pub(crate) deaths: u32,
    pub(crate) rank: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sloppy_core::sim::{RenderState, Simulation, SimulationSetup, VehicleCommand};

    #[test]
    fn self_repair_requires_a_living_wounded_veteran_past_the_delay() {
        assert!(self_repair_active(true, 1.0, 50.0, 100.0, REPAIR_DELAY));
        assert!(!self_repair_active(
            true,
            1.0,
            50.0,
            100.0,
            REPAIR_DELAY - 0.01
        ));
        assert!(!self_repair_active(false, 1.0, 50.0, 100.0, REPAIR_DELAY));
        assert!(!self_repair_active(true, 0.0, 50.0, 100.0, REPAIR_DELAY));
        assert!(!self_repair_active(true, 1.0, 100.0, 100.0, REPAIR_DELAY));
    }

    #[test]
    fn local_and_replicated_scoreboards_match() {
        let mut simulation = Simulation::new(4242.0, SimulationSetup::default());
        simulation.start();
        for _ in 0..120 {
            simulation.step(VehicleCommand::default(), true);
        }
        let tank = &mut simulation.tanks[0];
        tank.name = "Quote \" slash \\ newline\n火".into();
        tank.kills = 7;
        tank.deaths = 2;
        tank.xp = 500.0;
        tank.alive = false;
        let mut state = RenderState::default();
        simulation.fill_render_state(&mut state, None);
        assert_eq!(
            serde_json::to_string(&Scoreboard::Simulated(&simulation.tanks)).unwrap(),
            serde_json::to_string(&scoreboard_json(&state.tanks)).unwrap(),
        );
        assert_eq!(
            serde_json::to_string(&Scoreboard::Simulated(&[])).unwrap(),
            "[]"
        );
    }

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
        let human = serde_json::to_value(human_json(&tank, 0.0)).unwrap();
        assert_eq!(human["equipped"], json!("rocket"));
        assert_eq!(human["healthRatio"], json!(0.5));
        assert_eq!(human["repairDelay"], json!(REPAIR_DELAY));
        assert_eq!(human["ammo"][2]["count"], json!(2.0));
    }

    #[test]
    fn borrowed_scoreboard_preserves_order_escaping_and_empty_lists() {
        let tanks = [
            RenderTank {
                id: 7,
                name: "Quote \" slash \\ newline\n火".into(),
                team: Team::Red,
                kind: VehicleKind::Humvee,
                human: true,
                alive: false,
                kills: 12,
                deaths: 3,
                xp: 0.0,
                ..RenderTank::default()
            },
            RenderTank {
                id: 2,
                name: "Second".into(),
                ..RenderTank::default()
            },
        ];
        let text = serde_json::to_string(&scoreboard_json(&tanks)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value[0],
            json!({
                "id": 7, "name": tanks[0].name, "team": Team::Red, "kind": "humvee",
                "human": true, "alive": false, "kills": 12, "deaths": 3, "rank": 0,
            })
        );
        assert_eq!(value[1]["id"], json!(2));
        assert_eq!(serde_json::to_string(&scoreboard_json(&[])).unwrap(), "[]");
        let human = serde_json::to_value(human_json(&tanks[0], 100.0)).unwrap();
        assert_eq!(human["name"], json!(tanks[0].name));
        assert_eq!(human["equipped"], json!("tow"));
        assert_eq!(human["ammo"][0]["count"], serde_json::Value::Null);
        assert_eq!(human["selfRepair"], json!(false));
    }
}
