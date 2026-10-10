//! Shared page HUD records and ammo, repair and scoreboard rules for local and room play.

use serde::{Serialize, Serializer};
use sloppy_core::sim::ammunition::{AMMO_ORDER, equipped_weapon_for, has_ammo_for};
use sloppy_core::sim::data::vehicle;
use sloppy_core::sim::render_state::RenderTank;
use sloppy_core::sim::veterancy::{RANKS, REPAIR_DELAY, rank_index};
use sloppy_core::sim::{AmmoInventory, Tank, VehicleKind, Weapon};
use sloppy_render::presentation::hud::health_bar_state;

fn hud_ammo(
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

fn self_repair_active(alive: bool, repair: f64, hp: f64, max_hp: f64, since_combat: f64) -> bool {
    alive && repair > 0.0 && hp < max_hp && since_combat >= REPAIR_DELAY
}

/// The viewer's HUD block (`hud_json().human`). `elapsed` is the match clock, which
/// decides whether veteran self-repair is running.
pub fn human_json(tank: &RenderTank, elapsed: f64) -> HudHuman {
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
        kind: tank.kind,
        vehicle_name: vehicle(tank.kind).name,
        alive: tank.alive,
        hp: tank.hp,
        max_hp: tank.max_hp,
        health_ratio: health.ratio,
        health_color: health.color,
        rank,
        rank_name: stats.name,
        rank_damage: stats.damage,
        rank_fire_rate: stats.fire_rate,
        rank_health: stats.health,
        rank_repair: stats.repair,
        repair_delay: REPAIR_DELAY,
        ammo,
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

/// Every tank's name and kills (`hud_json().scoreboard`), from replicated or simulated
/// tanks.
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
                kills: tank.kills,
            })),
            Self::Simulated(tanks) => serializer.collect_seq(tanks.iter().map(|tank| HudScore {
                id: tank.id,
                name: &tank.name,
                kills: tank.kills,
            })),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudHuman {
    id: u32,
    kind: VehicleKind,
    vehicle_name: &'static str,
    alive: bool,
    hp: f64,
    max_hp: f64,
    health_ratio: f64,
    health_color: u32,
    rank: usize,
    rank_name: &'static str,
    rank_damage: f64,
    rank_fire_rate: f64,
    rank_health: f64,
    rank_repair: f64,
    repair_delay: f64,
    ammo: [HudAmmo; AMMO_ORDER.len()],
    mine_cooldown: f64,
    protection: f64,
    shield: f64,
    shield_points: f64,
    rapid: f64,
    speed: f64,
    laser: f64,
    respawn: f64,
    kills: u32,
    deaths: u32,
    self_repair: bool,
}

#[derive(Serialize)]
struct HudAmmo {
    weapon: Weapon,
    count: Option<f64>,
    selected: bool,
    available: bool,
}

#[derive(Serialize)]
struct HudScore<'a> {
    id: u32,
    name: &'a str,
    kills: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sloppy_core::sim::{RenderState, Simulation, SimulationSetup, Team, VehicleCommand};

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
            serde_json::to_string(&Scoreboard::Rendered(&state.tanks)).unwrap(),
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
        assert_eq!(human["ammo"][2]["selected"], json!(true));
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
        let text = serde_json::to_string(&Scoreboard::Rendered(&tanks)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value[0],
            json!({ "id": 7, "name": tanks[0].name, "kills": 12 })
        );
        assert_eq!(value[1]["id"], json!(2));
        assert_eq!(
            serde_json::to_string(&Scoreboard::Rendered(&[])).unwrap(),
            "[]"
        );
        let human = serde_json::to_value(human_json(&tanks[0], 100.0)).unwrap();
        // A Humvee fires its TOW, which no ammo slot offers.
        assert_eq!(
            equipped_weapon_for(
                VehicleKind::Humvee,
                &AmmoInventory::default(),
                Weapon::Standard
            ),
            Weapon::Tow
        );
        assert!(
            human["ammo"]
                .as_array()
                .unwrap()
                .iter()
                .all(|slot| slot["selected"] == json!(false))
        );
        assert_eq!(human["ammo"][0]["count"], serde_json::Value::Null);
        assert_eq!(human["selfRepair"], json!(false));
    }
}
