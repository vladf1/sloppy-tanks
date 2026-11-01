//! The kinds of things the game's models are built for (teams, vehicles, cover and
//! wreck parts): plain enums that the model builders and the page's geometry cache
//! share with the simulation, kept apart from the entity records of `types` so the
//! builders compile without the simulation (`sloppy_geometry_only`, AGENTS.md).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Serialized as 0 (blue) or 1 (red), like the TS `0 | 1` team.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Team {
    #[default]
    Blue = 0,
    Red = 1,
}

impl Team {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn from_index(index: usize) -> Team {
        if index.is_multiple_of(2) {
            Team::Blue
        } else {
            Team::Red
        }
    }

    pub const fn opponent(self) -> Team {
        match self {
            Team::Blue => Team::Red,
            Team::Red => Team::Blue,
        }
    }
}

impl Serialize for Team {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for Team {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(Team::Blue),
            1 => Ok(Team::Red),
            other => Err(serde::de::Error::custom(format!("invalid team {other}"))),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VehicleKind {
    Scout,
    #[default]
    Balanced,
    Heavy,
    Humvee,
}

impl VehicleKind {
    pub const ALL: [VehicleKind; 4] = [
        VehicleKind::Scout,
        VehicleKind::Balanced,
        VehicleKind::Heavy,
        VehicleKind::Humvee,
    ];
    /// The chassis a player may drive (every kind but the bot-only Humvee).
    pub const PLAYABLE: [VehicleKind; 3] = [
        VehicleKind::Scout,
        VehicleKind::Balanced,
        VehicleKind::Heavy,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            VehicleKind::Scout => "scout",
            VehicleKind::Balanced => "balanced",
            VehicleKind::Heavy => "heavy",
            VehicleKind::Humvee => "humvee",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverKind {
    Rock,
    Teeth,
    Hedgehog,
    Container,
    Cargo,
    House,
    Tree,
    Timber,
    Concrete,
    Drum,
    Tower,
    Rubble,
    #[default]
    Boundary,
}

impl CoverKind {
    pub const ALL: [CoverKind; 13] = [
        CoverKind::Rock,
        CoverKind::Teeth,
        CoverKind::Hedgehog,
        CoverKind::Container,
        CoverKind::Cargo,
        CoverKind::House,
        CoverKind::Tree,
        CoverKind::Timber,
        CoverKind::Concrete,
        CoverKind::Drum,
        CoverKind::Tower,
        CoverKind::Rubble,
        CoverKind::Boundary,
    ];

    /// The TypeScript identifier (the serialized name).
    pub const fn as_str(self) -> &'static str {
        match self {
            CoverKind::Rock => "rock",
            CoverKind::Teeth => "teeth",
            CoverKind::Hedgehog => "hedgehog",
            CoverKind::Container => "container",
            CoverKind::Cargo => "cargo",
            CoverKind::House => "house",
            CoverKind::Tree => "tree",
            CoverKind::Timber => "timber",
            CoverKind::Concrete => "concrete",
            CoverKind::Drum => "drum",
            CoverKind::Tower => "tower",
            CoverKind::Rubble => "rubble",
            CoverKind::Boundary => "boundary",
        }
    }

    /// Drums, dragon's teeth and hedgehogs are dynamic bodies that blasts and shells push.
    pub const fn movable(self) -> bool {
        matches!(
            self,
            CoverKind::Drum | CoverKind::Teeth | CoverKind::Hedgehog
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WreckPart {
    /// Hull and turret together.
    Intact,
    /// The hull alone, with the turret ring cut open.
    Hull,
    /// The turret without its gun.
    Turret,
    /// The turret with its gun.
    TurretBarrel,
    /// The gun alone.
    Barrel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickupKind {
    Spread,
    Rocket,
    Ricochet,
    Piercing,
    Rapid,
    Shield,
    Speed,
    Repair,
    Laser,
}

impl PickupKind {
    pub const ALL: [PickupKind; 9] = [
        PickupKind::Spread,
        PickupKind::Rocket,
        PickupKind::Ricochet,
        PickupKind::Piercing,
        PickupKind::Rapid,
        PickupKind::Shield,
        PickupKind::Speed,
        PickupKind::Repair,
        PickupKind::Laser,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FragmentShape {
    Armor,
    Wheel,
    Track,
    Shard,
    Wood,
    Panel,
    Beam,
    Log,
    DrumShell,
    DrumLid,
}
