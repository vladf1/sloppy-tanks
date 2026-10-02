//! Gameplay balance uses metres, seconds, radians and hit points unless stated otherwise.

use super::types::{PickupKind, VehicleKind, Weapon};

pub const STEP: f64 = 1.0 / 60.0;
pub const ARENA: f64 = 60.0;
pub const ROUND_TIME: f64 = 300.0;
pub const SOLO_TIME: f64 = 600.0;
pub const SCORE_LIMIT: u32 = 100;
const BASE_SPEED_MULTIPLIER: f64 = 1.13;
const BASE_STANDARD_SHELL_SPEED: f64 = 19.2;
const REFERENCE_TANK_SPEED: f64 = 184.0;
const REFERENCE_SHELL_SPEED: f64 = 535.0;
const TANK_PACING_MULTIPLIER: f64 = 1.2;
pub const KMH_PER_METRE_PER_SECOND: f64 = 3.6;

// Simulation distances use world metres; the selector rounds speeds to km/h.
// V-Tanks 570bf8d: Vanguard 184, standard shell 535; preserve that dodge
// ratio at our existing 19.2 m/s shell speed, then apply chassis ratios
// and the tank-only 20% increase, followed by the shared 13% pacing increase.
const fn reference_speed(multiplier: f64) -> f64 {
    ((BASE_STANDARD_SHELL_SPEED * REFERENCE_TANK_SPEED) / REFERENCE_SHELL_SPEED)
        * multiplier
        * TANK_PACING_MULTIPLIER
        * BASE_SPEED_MULTIPLIER
}

/// `Math.round` for the positive speeds below, usable in constants.
const fn round_positive(value: f64) -> f64 {
    (value + 0.5) as i64 as f64
}

pub const MOVE_ACCELERATION: f64 = 100.0;
pub const HULL_TURN_SPEED: f64 = 3.5; // A quarter turn takes about 0.45 seconds.
pub const REVERSE_SPEED: f64 = 0.8;
pub const PLAYER_FIRE_RATE_MULTIPLIER: f64 = 1.2;
pub const SHIELD_CAPACITY: f64 = 120.0; // Three standard 40-damage shells.
pub const INTERCEPTION_RADIUS: f64 = 0.8;
pub const INTERCEPTION_BLAST_RADIUS: f64 = 3.0;
pub const MINE_RADIUS: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleStats {
    pub name: &'static str,
    pub tag: &'static str,
    pub health: f64,
    pub speed: f64,
    pub speed_kmh: f64,
    pub mass: f64,
    pub scale: f64,
    pub weapon: Weapon,
}

const fn vehicle_stats(
    name: &'static str,
    tag: &'static str,
    health: f64,
    speed_multiplier: f64,
    mass: f64,
    scale: f64,
    weapon: Weapon,
) -> VehicleStats {
    let speed = reference_speed(speed_multiplier);
    VehicleStats {
        name,
        tag,
        health,
        speed,
        speed_kmh: round_positive(speed * KMH_PER_METRE_PER_SECOND),
        mass,
        scale,
        weapon,
    }
}

// Comparable game sizes; Bruiser anchors the fleet at 1.95 units wide.
const SCOUT: VehicleStats = vehicle_stats(
    "SKIPPER",
    "Light scout",
    80.0,
    1.24,
    1.0,
    (3.59 / 2.3) * (1.95 / 3.66),
    Weapon::Standard,
);
const BALANCED: VehicleStats = vehicle_stats(
    "BRUISER",
    "Balanced tank",
    100.0,
    1.0,
    1.45,
    1.95 / 2.42,
    Weapon::Standard,
);
const HEAVY: VehicleStats = vehicle_stats(
    "BIG RIG",
    "Heavy tank",
    140.0,
    0.76,
    2.5,
    (3.5 / 2.5) * (1.95 / 3.66) * 1.15,
    Weapon::Standard,
);
const HUMVEE: VehicleStats =
    vehicle_stats("HUNTER", "TOW Humvee", 35.0, 1.52, 0.72, 0.9, Weapon::Tow);

pub const fn vehicle(kind: VehicleKind) -> &'static VehicleStats {
    match kind {
        VehicleKind::Scout => &SCOUT,
        VehicleKind::Balanced => &BALANCED,
        VehicleKind::Heavy => &HEAVY,
        VehicleKind::Humvee => &HUMVEE,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponStats {
    pub name: &'static str,
    pub interval: f64,
    pub damage: f64,
    pub speed: f64,
    pub bounces: u32,
    pub color: u32,
    pub label: &'static str,
    pub unit: &'static str,
    pub per_crate: f64,
    pub carry_limit: f64,
}

const STANDARD: WeaponStats = WeaponStats {
    label: "STANDARD",
    unit: "SHELLS",
    per_crate: 0.0,
    carry_limit: f64::INFINITY,
    name: "Standard shells",
    interval: 0.85,
    damage: 40.0,
    speed: BASE_STANDARD_SHELL_SPEED * BASE_SPEED_MULTIPLIER,
    bounces: 0,
    color: 0xffdf00,
};
const SPREAD: WeaponStats = WeaponStats {
    label: "SPREAD",
    unit: "SPREAD VOLLEYS",
    per_crate: 18.0,
    carry_limit: 36.0,
    name: "Spread shot",
    interval: 1.1,
    damage: 27.0,
    speed: 17.6 * BASE_SPEED_MULTIPLIER,
    bounces: 0,
    color: 0xff38d4,
};
const ROCKET: WeaponStats = WeaponStats {
    label: "ROCKET",
    unit: "ROCKETS",
    per_crate: 12.0,
    carry_limit: 24.0,
    name: "Breaching rockets",
    interval: 1.3,
    damage: 65.0,
    speed: 13.6 * BASE_SPEED_MULTIPLIER,
    bounces: 0,
    color: 0xff591c,
};
const RICOCHET: WeaponStats = WeaponStats {
    name: "Ricochet shells",
    label: "RICOCHET",
    unit: "RICOCHET SHELLS",
    per_crate: 24.0,
    carry_limit: 48.0,
    interval: 0.85,
    damage: 60.0,
    speed: BASE_STANDARD_SHELL_SPEED * BASE_SPEED_MULTIPLIER,
    bounces: 3,
    color: 0xb19afc,
};
const PIERCING: WeaponStats = WeaponStats {
    name: "Piercing shells",
    label: "PIERCING",
    unit: "PIERCING SHELLS",
    per_crate: 24.0,
    carry_limit: 48.0,
    interval: 0.85,
    damage: 40.0,
    speed: BASE_STANDARD_SHELL_SPEED * BASE_SPEED_MULTIPLIER,
    bounces: 0,
    color: 0x54e6dc,
};
const TOW: WeaponStats = WeaponStats {
    name: "TOW missiles",
    label: "TOW",
    unit: "TOW MISSILES",
    interval: 2.35,
    damage: 75.0,
    speed: 22.5,
    bounces: 0,
    color: 0xffb84d,
    per_crate: 0.0,
    carry_limit: f64::INFINITY,
};

pub const fn weapon(kind: Weapon) -> &'static WeaponStats {
    match kind {
        Weapon::Standard => &STANDARD,
        Weapon::Spread => &SPREAD,
        Weapon::Rocket => &ROCKET,
        Weapon::Ricochet => &RICOCHET,
        Weapon::Piercing => &PIERCING,
        Weapon::Tow => &TOW,
    }
}

pub struct LaserDefense {
    pub chance: f64,
    pub duration: f64,
    pub range: f64,
    pub threat_radius: f64,
    /// Seconds after a zap before the same tank can zap another shell.
    pub recharge: f64,
    pub initial_delay: f64,
    pub respawn: f64,
}

pub const LASER_DEFENSE: LaserDefense = LaserDefense {
    chance: 0.5,
    duration: 20.0,
    range: 7.0,
    threat_radius: 3.0,
    recharge: 0.2,
    initial_delay: 25.0,
    respawn: 45.0,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickupStats {
    pub name: &'static str,
    pub icon: &'static str,
    pub color: u32,
    pub duration: f64,
}

pub const fn pickup(kind: PickupKind) -> PickupStats {
    match kind {
        PickupKind::Rapid => PickupStats {
            name: "RAPID FIRE",
            icon: "»",
            color: 0xffcf54,
            duration: 20.0,
        },
        PickupKind::Spread => PickupStats {
            name: "SPREAD AMMO",
            icon: "⋔",
            color: SPREAD.color,
            duration: 0.0,
        },
        PickupKind::Rocket => PickupStats {
            name: "ROCKET AMMO",
            icon: "↑",
            color: ROCKET.color,
            duration: 0.0,
        },
        PickupKind::Ricochet => PickupStats {
            name: "RICOCHET AMMO",
            icon: "↗",
            color: RICOCHET.color,
            duration: 0.0,
        },
        PickupKind::Piercing => PickupStats {
            name: "PIERCING AMMO",
            icon: "↟",
            color: PIERCING.color,
            duration: 0.0,
        },
        PickupKind::Shield => PickupStats {
            name: "SHIELD",
            icon: "◇",
            color: 0x72dbef,
            duration: 20.0,
        },
        PickupKind::Speed => PickupStats {
            name: "SPEED BOOST",
            icon: "ϟ",
            color: 0xbbe574,
            duration: 20.0,
        },
        PickupKind::Repair => PickupStats {
            name: "REPAIR",
            icon: "+",
            color: 0x88ddb0,
            duration: 0.0,
        },
        PickupKind::Laser => PickupStats {
            name: "LASER DEFENSE",
            icon: "✧",
            color: 0x7bfff2,
            duration: LASER_DEFENSE.duration,
        },
    }
}

pub const TEAM_COLORS: [u32; 2] = [0x008cff, 0xff303e];
pub const TEAM_NAMES: [&str; 2] = ["BLUE", "RED"];

/// Collision groups as the physics engine's packed 32-bit value: memberships in the high
/// 16 bits, the filter in the low 16 bits.
pub mod group {
    // All substantial debris shares membership 0x100 and the same contact filter.
    // Membership 0x8 retains terrain/cover contacts; 0x100 also identifies shell targets.
    const SOLID_DEBRIS: u32 = 0x0108_0107;
    /// Query all memberships, accepting cover only.
    pub const COVER_QUERY: u32 = 0xffff_0002;
    /// All substantial debris, tested at the shell's flight height.
    pub const DEBRIS_QUERY: u32 = 0xffff_0100;
    /// Cover and tank-contact hulls, excluding cosmetic debris.
    pub const STEERING_QUERY: u32 = 0xffff_0032;
    /// Main tank hull colliders only.
    pub const TANK_QUERY: u32 = 0xffff_0001;
    /// Hull touches cover, ground, anti-tank footprints and large debris.
    pub const TANK: u32 = 0x0001_002e;
    /// Model-sized hulls touch other tank hulls only.
    pub const TANK_CONTACT: u32 = 0x0010_0010;
    /// Planar tank blocking; excluded from projectile queries.
    pub const TOOTH_CONTACT: u32 = 0x0020_0001;
    /// Same tank-only membership for rooted stump footprints.
    pub const STUMP_CONTACT: u32 = 0x0020_0001;
    pub const COVER: u32 = 0x0002_000b;
    /// Cover membership also contacts the ground.
    pub const MOVABLE_COVER: u32 = 0x0002_000f;
    /// Accept movable cover as well as tanks and debris.
    pub const GROUND: u32 = 0x0004_000b;
    pub const FRAGMENT: u32 = 0x0008_0006;
    pub const PUSHABLE_DEBRIS: u32 = SOLID_DEBRIS;
    pub const TIMBER_DEBRIS: u32 = SOLID_DEBRIS;
    pub const WRECK: u32 = SOLID_DEBRIS;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_match_the_typescript_table() {
        // Values printed by the TS `VEHICLES` and `WEAPONS` tables.
        let expected = [
            (
                VehicleKind::Scout,
                11.103161181308407,
                40.0,
                0.8316108339272986,
            ),
            (
                VehicleKind::Balanced,
                8.954162242990652,
                32.0,
                0.8057851239669421,
            ),
            (
                VehicleKind::Heavy,
                6.805163304672895,
                24.0,
                0.8577868852459014,
            ),
            (VehicleKind::Humvee, 13.61032660934579, 49.0, 0.9),
        ];
        for (kind, speed, speed_kmh, scale) in expected {
            assert_eq!(vehicle(kind).speed, speed);
            assert_eq!(vehicle(kind).speed_kmh, speed_kmh);
            assert_eq!(vehicle(kind).scale, scale);
        }
        assert_eq!(weapon(Weapon::Standard).speed, 21.695999999999998);
        assert_eq!(weapon(Weapon::Spread).speed, 19.887999999999998);
        assert_eq!(weapon(Weapon::Rocket).speed, 15.367999999999999);
    }
}
