//! Bot roles: chassis, ranges, cadence and preferred ammunition, plus the bot name deck.

use serde::{Deserialize, Serialize};

use super::ammunition::{AMMO_ORDER, equipped_weapon, has_ammo};
use super::data::weapon;
use super::math::{Random, Vec2};
use super::types::{Tank, Team, VehicleKind, Weapon};
use super::veterancy::rank_stats;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BotPersonality {
    #[default]
    Scout,
    Guard,
    Sniper,
    Heavy,
    Minelayer,
    Support,
    Artillery,
}

impl BotPersonality {
    pub const ALL: [BotPersonality; 7] = [
        BotPersonality::Scout,
        BotPersonality::Guard,
        BotPersonality::Sniper,
        BotPersonality::Heavy,
        BotPersonality::Minelayer,
        BotPersonality::Support,
        BotPersonality::Artillery,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            BotPersonality::Scout => "scout",
            BotPersonality::Guard => "guard",
            BotPersonality::Sniper => "sniper",
            BotPersonality::Heavy => "heavy",
            BotPersonality::Minelayer => "minelayer",
            BotPersonality::Support => "support",
            BotPersonality::Artillery => "artillery",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotProfile {
    pub label: &'static str,
    pub chassis: VehicleKind,
    pub range: f64,
    pub sight: f64,
    pub speed: f64,
    pub turn: f64,
    pub reload: f64,
    pub aim_error: f64,
    pub stationary: bool,
}

// Adapted from v-tanks/src/game/enemy-behavior.ts. Range is scaled to our
// village lanes; snipers/artillery relocate until they have a firing lane.
pub const fn bot_profile_for(personality: BotPersonality) -> &'static BotProfile {
    match personality {
        BotPersonality::Scout => &BotProfile {
            label: "SCOUT",
            chassis: VehicleKind::Scout,
            range: 11.0,
            sight: 30.0,
            speed: 1.0,
            turn: 3.3,
            reload: 1.45,
            aim_error: 0.27,
            stationary: false,
        },
        BotPersonality::Guard => &BotProfile {
            label: "GUARD",
            chassis: VehicleKind::Balanced,
            range: 18.0,
            sight: 30.0,
            speed: 0.78,
            turn: 3.3,
            reload: 1.15,
            aim_error: 0.21,
            stationary: false,
        },
        BotPersonality::Sniper => &BotProfile {
            label: "SNIPER",
            chassis: VehicleKind::Balanced,
            range: 24.0,
            sight: 44.0,
            speed: 0.7,
            turn: 1.7,
            reload: 2.3,
            aim_error: 0.15,
            stationary: true,
        },
        BotPersonality::Heavy => &BotProfile {
            label: "HEAVY",
            chassis: VehicleKind::Heavy,
            range: 18.0,
            sight: 30.0,
            speed: 0.7,
            turn: 3.3,
            reload: 2.05,
            aim_error: 0.21,
            stationary: false,
        },
        BotPersonality::Minelayer => &BotProfile {
            label: "MINELAYER",
            chassis: VehicleKind::Scout,
            range: 8.0,
            sight: 30.0,
            speed: 0.85,
            turn: 3.3,
            reload: 1.75,
            aim_error: 0.25,
            stationary: false,
        },
        BotPersonality::Support => &BotProfile {
            label: "SUPPORT",
            chassis: VehicleKind::Balanced,
            range: 22.0,
            sight: 34.0,
            speed: 0.78,
            turn: 3.3,
            reload: 1.15,
            aim_error: 0.21,
            stationary: false,
        },
        BotPersonality::Artillery => &BotProfile {
            label: "ARTILLERY",
            chassis: VehicleKind::Heavy,
            range: 26.0,
            sight: 42.0,
            speed: 0.65,
            turn: 1.7,
            reload: 3.4,
            aim_error: 0.23,
            stationary: true,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BotAssignment {
    pub personality: BotPersonality,
    pub ultra_aggressive: bool,
}

pub fn bot_assignment(slot: usize, team: Team, ordinal: usize) -> BotAssignment {
    // Matching frontline roles on each team, with sniper/artillery alternating
    // across the two backline slots. Count bots, not IDs shared with scenery.
    let roster = [
        BotPersonality::Scout,
        BotPersonality::Guard,
        if team == Team::Blue { BotPersonality::Sniper } else { BotPersonality::Artillery },
        BotPersonality::Heavy,
        BotPersonality::Minelayer,
        BotPersonality::Support,
        if team == Team::Blue { BotPersonality::Artillery } else { BotPersonality::Sniper },
    ];
    BotAssignment {
        personality: roster[slot % roster.len()],
        ultra_aggressive: (ordinal + 1) % 10 == 0,
    }
}

pub fn bot_profile(tank: &Tank) -> &'static BotProfile {
    bot_profile_for(tank.brain.personality)
}

pub const fn bot_ammo(personality: BotPersonality) -> Weapon {
    match personality {
        BotPersonality::Artillery | BotPersonality::Heavy => Weapon::Rocket,
        BotPersonality::Scout | BotPersonality::Minelayer => Weapon::Spread,
        BotPersonality::Sniper => Weapon::Piercing,
        BotPersonality::Guard | BotPersonality::Support => Weapon::Ricochet,
    }
}

pub fn preferred_ammo(tank: &Tank) -> Weapon {
    if tank.kind == VehicleKind::Humvee {
        return Weapon::Tow;
    }
    let preferred = bot_ammo(tank.brain.personality);
    if has_ammo(tank, preferred) {
        return preferred;
    }
    AMMO_ORDER
        .into_iter()
        .find(|&candidate| candidate != Weapon::Standard && has_ammo(tank, candidate))
        .unwrap_or(Weapon::Standard)
}

/// Seconds between a bot's shots with `fired` (its equipped weapon by default).
pub fn bot_reload(tank: &Tank, jitter: f64, fired: Option<Weapon>) -> f64 {
    let fired = fired.unwrap_or_else(|| equipped_weapon(tank));
    let base = bot_profile(tank).reload * if tank.brain.ultra_aggressive { 0.48 } else { 1.0 };
    // Hunters close faster, but never erase the human's matched-weapon advantage.
    ((base + jitter).max(weapon(fired).interval * 1.15) * if tank.rapid > 0.0 { 0.5 } else { 1.0 })
        / rank_stats(tank.xp).fire_rate
}

pub fn combat_movement(tank: &Tank, dx: f64, dz: f64, strafe: f64) -> Vec2 {
    let profile = bot_profile(tank);
    let d = match dx.hypot(dz) {
        0.0 => 1.0,
        length => length,
    };
    let range = if tank.brain.ultra_aggressive { 7.0 } else { profile.range };
    if d < range - 2.0 {
        return Vec2::new(-dx / d, -dz / d);
    }
    if d > range + 2.5 {
        return Vec2::new(dx / d, dz / d);
    }
    if profile.stationary && !tank.brain.ultra_aggressive {
        return Vec2::ZERO;
    }
    Vec2::new((dz / d) * strafe, (-dx / d) * strafe)
}

pub const BOT_NAMES: [&str; 84] = [
    "IRON JACK",
    "SIDEWINDER",
    "NITRO",
    "TREADHEAD",
    "HOTSHOT",
    "RIVET",
    "DUST DEVIL",
    "BULLSEYE",
    "SCRAP KING",
    "VEX",
    "BLACKTOP",
    "WRECKER",
    "FLINT",
    "GRIT",
    "BOLT",
    "ROAD RAGE",
    "CRATER",
    "SMOKESCREEN",
    "LOCKJAW",
    "RUMBLE",
    "CANNONBALL",
    "COPPERHEAD",
    "RUSTY",
    "BADGER",
    "DEADBOLT",
    "HELLCAT",
    "RICOCHET",
    "ROADBLOCK",
    "BUZZSAW",
    "CROWBAR",
    "THUNDERCLAP",
    "FLATLINE",
    "SLEDGE",
    "IRONCLAD",
    "REDLINE",
    "DIESEL",
    "DREADNOUGHT",
    "JUNKYARD",
    "SCORCH",
    "BRASS KNUCKLE",
    "WILDCARD",
    "HARDCASE",
    "RATTLER",
    "GHOST",
    "TOMBSTONE",
    "STEELTOE",
    "DUSTUP",
    "BOOMBOX",
    "HAILSTORM",
    "RAMPAGE",
    "SMOKESTACK",
    "AFTERSHOCK",
    "BACKFIRE",
    "BONEHEAD",
    "JACKHAMMER",
    "TORQUE",
    "WARBIRD",
    "BULLDOZER",
    "DYNAMO",
    "OUTLAW",
    "ROCKET DOG",
    "SIDESWIPE",
    "SPARKPLUG",
    "BARRAGE",
    "TANKBUSTER",
    "METALHEAD",
    "TRIGGER",
    "BLACKOUT",
    "WARPATH",
    "IRON WOLF",
    "SCATTERSHOT",
    "BOILER",
    "FUSE",
    "CRUNCH",
    "NIGHTSHIFT",
    "RUBBLE",
    "HEATWAVE",
    "HATCHET",
    "SHRAPNEL",
    "OVERDRIVE",
    "BULLWHIP",
    "SANDSTORM",
    "GUNSLINGER",
    "RIPSAW",
];

/// A fresh round deck; independent of combat RNG and unique until the pool is exhausted.
pub fn shuffled_bot_names(seed: f64) -> Vec<&'static str> {
    let mut names = BOT_NAMES.to_vec();
    let mut rng = Random::new(seed);
    for i in (1..names.len()).rev() {
        let j = (rng.next() * (i + 1) as f64).floor() as usize;
        names.swap(i, j);
    }
    names
}
