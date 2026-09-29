//! The rules a standard map plays with, and how single player adopts an extra level.

use super::simulation::{GameMode, SimulationSetup};
use super::simulation_rules::{MAX_FRAGMENTS, SIMULATION_RULES};

/// The rules a standard map plays with. Applying them to a simulation clears whatever an
/// extra level set before, so one arena can switch between the two.
pub fn standard_rules() -> SimulationSetup {
    SimulationSetup {
        custom_map: Some(None),
        endless_match: Some(false),
        round_count: Some(SIMULATION_RULES.default_tank_count),
        human_health_multiplier: Some(1.0),
        power_up_duration_multiplier: Some(1.0),
        ammo_crate_multiplier: Some(1.0),
        max_fragments: Some(MAX_FRAGMENTS),
        after_step: Some(None),
        ..SimulationSetup::default()
    }
}

/// Single player fights an extra level as one endless team battle with its whole roster.
pub fn single_player_rules(level: SimulationSetup) -> SimulationSetup {
    standard_rules().merged(level).merged(SimulationSetup {
        game_mode: Some(GameMode::Team),
        endless_match: Some(true),
        ..SimulationSetup::default()
    })
}
