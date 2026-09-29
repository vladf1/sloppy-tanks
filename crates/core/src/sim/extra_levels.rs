//! Every extra level's arena, roster and rules, shared by single player and rooms.

use super::map_options::MapId;
use super::simulation::SimulationSetup;
use super::stress_test_level::stress_test_level;
use super::superstress_level::superstress_level;

/// The setup an extra level brings, or `None` for a standard map.
pub fn extra_level(id: MapId) -> Option<SimulationSetup> {
    match id {
        MapId::StressTest => Some(stress_test_level()),
        MapId::Superstress => Some(superstress_level()),
        MapId::Village | MapId::Harbor | MapId::Quarry => None,
    }
}
