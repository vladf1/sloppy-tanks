//! The menu's per-visit game choices, without the DOM.

use serde::{Deserialize, Serialize};

use super::difficulty::Difficulty;
use super::map_options::{MapId, is_extra_level};
use super::math::Random;
use super::simulation::{GameMode, Simulation};
use super::types::{Team, VehicleKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameOptions {
    pub human_kind: VehicleKind,
    pub human_team: Team,
    pub game_mode: GameMode,
    pub map_mode: MapId,
    pub difficulty: Difficulty,
}

/// The page requests a link's `?map=`, or else the player's last map, so a returning
/// player's GO needs no rebuild. Extra levels are offered only with `?debug`.
pub fn initial_game_options(
    seed: f64,
    requested_map: Option<MapId>,
    extra_levels: bool,
    difficulty: Option<Difficulty>,
) -> GameOptions {
    GameOptions {
        human_kind: VehicleKind::Balanced,
        human_team: if Random::new(seed).next() < 0.5 {
            Team::Blue
        } else {
            Team::Red
        },
        game_mode: GameMode::Team,
        map_mode: requested_map
            .filter(|&option| extra_levels || !is_extra_level(option))
            .unwrap_or(MapId::Village),
        difficulty: difficulty.unwrap_or_default(),
    }
}

impl GameOptions {
    /// These choices as Battle Setup plays them. An extra level stands only where extra
    /// levels are offered (`?debug`), else the village does, and it plays its own team
    /// battle whatever format the player keeps for the standard maps
    /// (`bindGameOptions`).
    pub fn offered(self, extra_levels: bool) -> GameOptions {
        let map_mode = if extra_levels || !is_extra_level(self.map_mode) {
            self.map_mode
        } else {
            MapId::Village
        };
        let game_mode = if is_extra_level(map_mode) {
            GameMode::Team
        } else {
            self.game_mode
        };
        GameOptions {
            map_mode,
            game_mode,
            ..self
        }
    }
}

/// Only the menu choices, for copying from a simulation that carries them.
pub fn game_choices(simulation: &Simulation) -> GameOptions {
    GameOptions {
        human_kind: simulation.human_kind,
        human_team: simulation.human_team,
        game_mode: simulation.game_mode,
        map_mode: simulation.map_mode,
        difficulty: simulation.difficulty,
    }
}
