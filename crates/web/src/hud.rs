//! Shared page HUD records and ammo, repair and scoreboard rules for local and room play.
//!
//! The page reads them as `hud_json`; the desktop shell reads the same values typed.
//! Read, they borrow the simulation or the displayed scene, so the page's frequent
//! `hud_json` copies nothing on the way to its text; [`Hud::into_owned`] keeps one between
//! reads.

use std::borrow::Cow;

use serde::{Serialize, Serializer};
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

use crate::recap::RoundRecap;

/// `Game.hud_json`: the page HUD's view of the match and the human's tank.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hud<'a> {
    #[serde(rename = "match")]
    pub match_state: Match,
    pub elapsed: f64,
    pub game_mode: GameMode,
    pub endless_match: bool,
    pub map_mode: MapId,
    pub map_name: String,
    pub difficulty: Difficulty,
    pub human_team: Team,
    pub score_limit: u32,
    pub team_names: [&'static str; 2],
    pub active_enemies: usize,
    pub speed_tuning: SpeedTuning,
    pub human: HudHuman,
    pub scoreboard: Scoreboard<'a>,
    /// The finished round's recap, while results show.
    pub recap: Option<Cow<'a, RoundRecap>>,
}

impl Hud<'_> {
    /// A copy that outlives the game it was read from.
    pub fn into_owned(self) -> Hud<'static> {
        Hud {
            match_state: self.match_state,
            elapsed: self.elapsed,
            game_mode: self.game_mode,
            endless_match: self.endless_match,
            map_mode: self.map_mode,
            map_name: self.map_name,
            difficulty: self.difficulty,
            human_team: self.human_team,
            score_limit: self.score_limit,
            team_names: self.team_names,
            active_enemies: self.active_enemies,
            speed_tuning: self.speed_tuning,
            human: self.human,
            scoreboard: self.scoreboard.into_owned(),
            recap: self.recap.map(|recap| Cow::Owned(recap.into_owned())),
        }
    }
}

/// Single player's HUD. The human's tank as the HUD reads it is refilled into `tank`
/// from the simulation, not the last frame's state: debug hooks change tanks between
/// frames. `recap` is the finished round's, while results show.
pub fn simulation_hud<'a>(
    sim: &'a Simulation,
    tank: &mut RenderTank,
    recap: Option<&'a RoundRecap>,
) -> Hud<'a> {
    sim.fill_tank(tank, sim.human());
    Hud {
        match_state: sim.match_state.clone(),
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
        speed_tuning: sim.speed_tuning,
        human: human_json(tank, sim.elapsed),
        scoreboard: Scoreboard::Simulated(&sim.tanks),
        recap: recap.map(Cow::Borrowed),
    }
}

/// `NetGame.hud_json`: a room's HUD for the displayed scene; the room lobby has the
/// rest.
#[derive(Clone, Debug, Serialize)]
pub struct RoomHud<'a> {
    #[serde(rename = "match")]
    pub match_state: &'a Match,
    pub human: HudHuman,
    pub scoreboard: Scoreboard<'a>,
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
/// tanks, or rows kept between reads.
#[derive(Clone, Debug)]
pub enum Scoreboard<'a> {
    Rendered(&'a [RenderTank]),
    Simulated(&'a [Tank]),
    Kept(Vec<HudScore<'static>>),
}

impl Scoreboard<'_> {
    /// Every tank's row, in roster order.
    pub fn rows(&self) -> impl Iterator<Item = HudScore<'_>> {
        // One chain over the three sources, two of them empty, rather than a boxed
        // iterator: the page serializes this every few frames.
        let (rendered, simulated, kept): (&[RenderTank], &[Tank], &[HudScore]) = match self {
            Self::Rendered(tanks) => (*tanks, &[], &[]),
            Self::Simulated(tanks) => (&[], *tanks, &[]),
            Self::Kept(rows) => (&[], &[], rows.as_slice()),
        };
        rendered
            .iter()
            .map(|tank| HudScore {
                id: tank.id,
                name: Cow::Borrowed(&tank.name),
                kills: tank.kills,
                deaths: tank.deaths,
                team: tank.team,
                kind: tank.kind,
                human: tank.human,
            })
            .chain(simulated.iter().map(|tank| HudScore {
                id: tank.id,
                name: Cow::Borrowed(&tank.name),
                kills: tank.kills,
                deaths: tank.deaths,
                team: tank.team,
                kind: tank.kind,
                human: tank.human,
            }))
            .chain(kept.iter().map(|row| HudScore {
                id: row.id,
                name: Cow::Borrowed(&row.name),
                kills: row.kills,
                deaths: row.deaths,
                team: row.team,
                kind: row.kind,
                human: row.human,
            }))
    }

    /// The tank `id`'s row.
    pub fn row(&self, id: u32) -> Option<HudScore<'_>> {
        self.rows().find(|row| row.id == id)
    }

    /// Rows that outlive the tanks they were read from.
    pub fn into_owned(self) -> Scoreboard<'static> {
        match self {
            Self::Kept(rows) => Scoreboard::Kept(rows),
            borrowed => Scoreboard::Kept(borrowed.rows().map(HudScore::into_owned).collect()),
        }
    }
}

impl Serialize for Scoreboard<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.rows())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudHuman {
    pub id: u32,
    pub kind: VehicleKind,
    pub vehicle_name: &'static str,
    pub alive: bool,
    pub hp: f64,
    pub max_hp: f64,
    pub health_ratio: f64,
    pub health_color: u32,
    pub rank: usize,
    pub rank_name: &'static str,
    pub rank_damage: f64,
    pub rank_fire_rate: f64,
    pub rank_health: f64,
    pub rank_repair: f64,
    /// Seconds out of combat before a ranked hull repairs itself.
    pub repair_delay: f64,
    /// One slot per entry of `AMMO_ORDER`.
    pub ammo: [HudAmmo; AMMO_ORDER.len()],
    pub mine_cooldown: f64,
    pub protection: f64,
    pub shield: f64,
    pub shield_points: f64,
    pub rapid: f64,
    pub speed: f64,
    pub laser: f64,
    pub respawn: f64,
    pub kills: u32,
    pub deaths: u32,
    pub self_repair: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct HudAmmo {
    pub weapon: Weapon,
    /// `None` for unlimited Standard shells.
    pub count: Option<f64>,
    pub selected: bool,
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HudScore<'a> {
    pub id: u32,
    pub name: Cow<'a, str>,
    pub kills: u32,
    pub deaths: u32,
    pub team: Team,
    pub kind: VehicleKind,
    /// A player's tank rather than a bot's, which the battle report marks.
    pub human: bool,
}

impl HudScore<'_> {
    pub fn into_owned(self) -> HudScore<'static> {
        HudScore {
            id: self.id,
            name: Cow::Owned(self.name.into_owned()),
            kills: self.kills,
            deaths: self.deaths,
            team: self.team,
            kind: self.kind,
            human: self.human,
        }
    }
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
