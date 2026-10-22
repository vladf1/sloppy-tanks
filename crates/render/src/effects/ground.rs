//! What the arena floor throws up when something strikes it: the dust color and
//! the darker clods of each map's ground, so a blast on a village lane kicks
//! brown dirt, on its meadows dark earth and grass, on the harbor's concrete grey
//! grit and in the quarry pale sand.

use sloppy_core::models::is_village_dirt;

use crate::color::hex_to_linear;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ground {
    #[default]
    Village,
    Harbor,
    Quarry,
}

/// The dust raised from a surface and the clods thrown off it, sRGB hex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Soil {
    pub dust: u32,
    pub clods: [u32; 3],
}

const LANE: Soil = Soil {
    dust: 0xc29a6b,
    clods: [0x6b4a2c, 0x8a6440, 0x4d3520],
};
const MEADOW: Soil = Soil {
    dust: 0x8a7764,
    clods: [0x4a3a26, 0x3f5a26, 0x5c4a30],
};
const CONCRETE: Soil = Soil {
    dust: 0xa9a7a0,
    clods: [0x7d7b76, 0x5e5c58, 0x9a978f],
};
const SAND: Soil = Soil {
    dust: 0xd8be92,
    clods: [0xa3845a, 0xc2a273, 0x7d6444],
};

impl Ground {
    pub fn from_theme(theme: &str) -> Self {
        match theme {
            "harbor" => Self::Harbor,
            "quarry" => Self::Quarry,
            _ => Self::Village,
        }
    }

    /// The soil at a point of this map.
    pub fn soil(self, x: f64, z: f64) -> Soil {
        match self {
            Self::Village if is_village_dirt(x, z) => LANE,
            Self::Village => MEADOW,
            Self::Harbor => CONCRETE,
            Self::Quarry => SAND,
        }
    }

    /// Linear dust color at a point.
    pub fn dust(self, x: f64, z: f64) -> [f64; 3] {
        hex_to_linear(self.soil(x, z).dust).map(f64::from)
    }
}
