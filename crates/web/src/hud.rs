//! Shared page HUD records and ammo, repair and scoreboard rules for local and room play.

use serde::{Serialize, Serializer};
use serde_json::Value;
use sloppy_core::sim::ammunition::{AMMO_ORDER, equipped_weapon_for, has_ammo_for};
use sloppy_core::sim::data::{SCORE_LIMIT, TEAM_NAMES, vehicle};
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::render_state::RenderTank;
use sloppy_core::sim::simulation::SpeedTuning;
use sloppy_core::sim::veterancy::{RANKS, REPAIR_DELAY, rank_index};
use sloppy_core::sim::{
    AmmoInventory, GameMode, Match, RenderState, Simulation, Tank, Team, VehicleKind, Weapon,
};
use sloppy_render::presentation::hud::health_bar_state;

/// `Game.hud_json`: the page HUD's view of the match and the human's tank.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hud<'a> {
    #[serde(rename = "match")]
    match_state: &'a Match,
    elapsed: f64,
    game_mode: GameMode,
    endless_match: bool,
    map_mode: MapId,
    map_name: String,
    difficulty: Difficulty,
    human_team: Team,
    score_limit: u32,
    team_names: [&'static str; 2],
    active_enemies: usize,
    speed_tuning: &'a SpeedTuning,
    human: HudHuman,
    scoreboard: Scoreboard<'a>,
    recap: Option<&'a Value>,
}

/// Single player's HUD. The human's tank as the HUD reads it is refilled into `tank`
/// from the simulation, not the last frame's state: debug hooks change tanks between
/// frames. `recap` is the finished round's, while results show.
pub fn simulation_hud<'a>(
    sim: &'a Simulation,
    tank: &mut RenderTank,
    recap: Option<&'a Value>,
) -> Hud<'a> {
    sim.fill_tank(tank, sim.human());
    Hud {
        match_state: &sim.match_state,
        elapsed: sim.elapsed,
        game_mode: sim.game_mode,
        endless_match: sim.endless_match,
        map_mode: sim.map_mode,
        map_name: sim.map_name(),
        difficulty: sim.difficulty,
        human_team: sim.human_team,
        score_limit: SCORE_LIMIT,
        team_names: TEAM_NAMES,
        active_enemies: sim.tanks.iter().filter(|t| !t.human && t.alive).count(),
        speed_tuning: &sim.speed_tuning,
        human: human_json(tank, sim.elapsed),
        scoreboard: Scoreboard::Simulated(&sim.tanks),
        recap,
    }
}

/// `NetGame.hud_json`: a room's HUD for the displayed scene; the room lobby has the
/// rest.
#[derive(Serialize)]
pub struct RoomHud<'a> {
    #[serde(rename = "match")]
    match_state: &'a Match,
    human: HudHuman,
    scoreboard: Scoreboard<'a>,
}

/// The room HUD for `state`, or `None` before the scene has its viewer.
pub fn room_hud(state: &RenderState) -> Option<RoomHud<'_>> {
    let viewer = state.viewer()?;
    Some(RoomHud {
        match_state: &state.match_state,
        human: human_json(viewer, state.elapsed),
        scoreboard: Scoreboard::Rendered(&state.tanks),
    })
}

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
    let health = health_bar_state(tank.hp, tank.max_hp);
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
                deaths: tank.deaths,
                team: tank.team,
                kind: tank.kind,
                human: tank.human,
            })),
            Self::Simulated(tanks) => serializer.collect_seq(tanks.iter().map(|tank| HudScore {
                id: tank.id,
                name: &tank.name,
                kills: tank.kills,
                deaths: tank.deaths,
                team: tank.team,
                kind: tank.kind,
                human: tank.human,
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
    deaths: u32,
    team: Team,
    kind: VehicleKind,
    /// A player's tank rather than a bot's, which the battle report marks.
    human: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sloppy_core::sim::{RenderState, Simulation, SimulationSetup, Team};

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

    /// Single player serializes the simulation's tanks and a room its replicated
    /// copies: both give the same scoreboard, in roster order, with names escaped.
    #[test]
    fn local_and_replicated_scoreboards_match() {
        let mut simulation = Simulation::new(4242.0, SimulationSetup::default());
        let tank = &mut simulation.tanks[0];
        tank.name = "Quote \" slash \\ newline\n火".into();
        tank.team = Team::Red;
        tank.kind = VehicleKind::Humvee;
        tank.human = true;
        tank.kills = 12;
        tank.deaths = 3;
        tank.xp = 500.0;
        tank.alive = false;
        let mut state = RenderState::default();
        simulation.fill_render_state(&mut state, None);
        let text = serde_json::to_string(&Scoreboard::Simulated(&simulation.tanks)).unwrap();
        assert_eq!(
            text,
            serde_json::to_string(&Scoreboard::Rendered(&state.tanks)).unwrap(),
        );
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let tanks = &simulation.tanks;
        assert_eq!(
            value[0],
            json!({
                "id": tanks[0].id,
                "name": tanks[0].name,
                "kills": 12,
                "deaths": 3,
                "team": 1,
                "kind": "humvee",
                "human": true
            })
        );
        let ids: Vec<_> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|score| score["id"].as_u64().unwrap())
            .collect();
        let roster: Vec<_> = tanks.iter().map(|tank| u64::from(tank.id)).collect();
        assert_eq!(ids, roster);
        assert_eq!(
            serde_json::to_string(&Scoreboard::Simulated(&[])).unwrap(),
            "[]"
        );
        assert_eq!(
            serde_json::to_string(&Scoreboard::Rendered(&[])).unwrap(),
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
        // A Humvee fires its TOW, which no ammo slot offers.
        assert_eq!(
            equipped_weapon_for(
                VehicleKind::Humvee,
                &AmmoInventory::default(),
                Weapon::Standard
            ),
            Weapon::Tow
        );
        let humvee = RenderTank {
            kind: VehicleKind::Humvee,
            alive: false,
            ..RenderTank::default()
        };
        let human = serde_json::to_value(human_json(&humvee, 100.0)).unwrap();
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
