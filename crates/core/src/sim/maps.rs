//! Arena maps: authored layouts and the ground/theme the renderer draws them with.

use serde::{Deserialize, Serialize};

use super::arena::{CoverDef, arena_layout};
use super::harbor_layout::harbor_layout;
use super::map_options::MapId;
use super::quarry_layout::quarry_layout;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroundKind {
    DryGrass,
    PackedDirt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MapTheme {
    Village,
    Harbor,
    Quarry,
}

impl MapTheme {
    pub const fn as_str(self) -> &'static str {
        match self {
            MapTheme::Village => "village",
            MapTheme::Harbor => "harbor",
            MapTheme::Quarry => "quarry",
        }
    }
}

pub struct ArenaMap {
    pub id: MapId,
    pub theme: Option<MapTheme>,
    pub floor: Option<GroundKind>,
    pub outer_floor: Option<GroundKind>,
    pub outer_floor_extent: Option<f64>,
    /// A compact yard's size relative to the standard arena; see `Simulation::map_scale`.
    pub scale: Option<f64>,
    pub layout: fn() -> Vec<CoverDef>,
}

const fn standard(id: MapId, theme: MapTheme, layout: fn() -> Vec<CoverDef>) -> ArenaMap {
    ArenaMap {
        id,
        theme: Some(theme),
        floor: None,
        outer_floor: None,
        outer_floor_extent: None,
        scale: None,
        layout,
    }
}

/// The standard maps. Extra levels bring their own map as `custom_map`.
pub static MAPS: [ArenaMap; 3] = [
    standard(MapId::Village, MapTheme::Village, arena_layout),
    standard(MapId::Harbor, MapTheme::Harbor, harbor_layout),
    standard(MapId::Quarry, MapTheme::Quarry, quarry_layout),
];

/// The authored map for a menu choice, or an extra level's own map. It needs no physics
/// world, so the renderer can build scenery before one exists.
pub fn selected_map(map_mode: MapId, custom_map: Option<&'static ArenaMap>) -> &'static ArenaMap {
    custom_map
        .or_else(|| MAPS.iter().find(|candidate| candidate.id == map_mode))
        .unwrap_or_else(|| {
            panic!(
                "The {} level brings its own map; apply its level setup first",
                map_mode.as_str()
            )
        })
}
