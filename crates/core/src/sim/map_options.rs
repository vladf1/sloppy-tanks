//! The maps offered by Battle Setup, room settings and `?map=` links.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MapId {
    #[default]
    Village,
    Harbor,
    Quarry,
    StressTest,
    Superstress,
}

impl MapId {
    pub const ALL: [MapId; 5] = [
        MapId::Village,
        MapId::Harbor,
        MapId::Quarry,
        MapId::StressTest,
        MapId::Superstress,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            MapId::Village => "village",
            MapId::Harbor => "harbor",
            MapId::Quarry => "quarry",
            MapId::StressTest => "stress-test",
            MapId::Superstress => "superstress",
        }
    }

    pub fn parse(value: &str) -> Option<MapId> {
        MapId::ALL.into_iter().find(|id| id.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapOption {
    pub id: MapId,
    pub name: &'static str,
    pub description: &'static str,
    /// Offered only with `?debug`; brings its own arena, roster and rules.
    pub extra: bool,
    /// Tanks per team for an extra level's roster.
    pub team_tanks: Option<usize>,
}

// Extra levels are offered only with `?debug`. Each brings its own arena, bot
// roster and rules from `extra_levels`, which the browser loads once one is chosen.
pub const MAP_OPTIONS: [MapOption; 5] = [
    MapOption {
        id: MapId::Village,
        name: "Pine Village",
        description: "A quiet little village. Bring the noise.",
        extra: false,
        team_tanks: None,
    },
    MapOption {
        id: MapId::Harbor,
        name: "Harbor Havoc",
        description: "Salt air. Hot steel. Dockside mayhem.",
        extra: false,
        team_tanks: None,
    },
    MapOption {
        id: MapId::Quarry,
        name: "Dusty Dig",
        description: "Open ground. Weathered stone. Dig your own shortcut.",
        extra: false,
        team_tanks: None,
    },
    MapOption {
        id: MapId::StressTest,
        name: "Stress Grid",
        description: "30 tanks · 75 destructibles · permanent buildings and barriers",
        extra: true,
        team_tanks: Some(15),
    },
    MapOption {
        id: MapId::Superstress,
        name: "Scrap Yard",
        description: "Compact yard · 30 tanks · cover rebuilds and debris lingers",
        extra: true,
        team_tanks: Some(15),
    },
];

pub fn map_option_for(id: MapId) -> &'static MapOption {
    MAP_OPTIONS
        .iter()
        .find(|option| option.id == id)
        .expect("every map id has an option")
}

pub fn is_extra_level(id: MapId) -> bool {
    map_option_for(id).extra
}
