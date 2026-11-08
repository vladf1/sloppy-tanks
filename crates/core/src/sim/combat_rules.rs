//! Shared combat distances are metres; durations are seconds.

pub struct CombatRules {
    /// Only a bound for pooled flight: a round flies until it strikes something or
    /// leaves the arena (`projectiles.rs`), and this lets the slowest one (a Spread
    /// pellet, about 20 m/s) cross the standard arena's 170 m diagonal first.
    pub projectile_lifetime: f64,
    pub spread_angle: f64,
    pub rocket_blast_radius: f64,
    pub rocket_top_speed_multiplier: f64,
    pub rocket_acceleration_seconds: f64,
    /// TOWs can correct toward their marked target, but their turn radius stays readable
    /// and they never snap around like a guided rocket.
    pub tow_turn_rate: f64,
    pub rapid_reload_multiplier: f64,
    /// Leave a gap after a ray contact so the next query does not hit the same face at time zero.
    pub muzzle_clearance: f64,
    pub bounce_clearance: f64,
    pub contact_time_epsilon: f64,
    pub separation_epsilon: f64,
    /// Each shell can bounce/intercept several times in a tick; cap repeated zero-time contacts.
    pub contacts_per_shot: usize,
    pub minimum_blast_damage_fraction: f64,
    pub blast_impulse: f64,
    pub minimum_blast_distance: f64,
    pub cover_blast_allowance: f64,
    pub drum_blast_radius: f64,
    pub drum_damage: f64,
    /// A fuel drum's blast on a structure's standing parts, over its whole blast radius.
    pub drum_structure_blast: StructureBlast,
    /// A tank's death explosion on the structure parts beside it: a milder, closer blast.
    pub death_structure_blast: StructureBlast,
}

/// How a blast on the ground meets a structure's standing parts: `pressure` (in shell
/// damage units) where it goes off, falling off by `falloff` to its edge `reach` metres
/// away, and adding up on a part from blast to blast against its strength. The parts it
/// breaks within `shatter` of it splinter; those farther off come loose whole and fall.
/// `throw` is how hard it throws a structure's fallen pieces, as a multiple of the throw
/// `structure_physics` gives a standard shell's burst.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructureBlast {
    pub pressure: f64,
    pub falloff: BlastFalloff,
    pub reach: f64,
    pub shatter: f64,
    pub throw: f64,
}

/// How a blast's pressure falls off with the distance from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlastFalloff {
    /// Evenly to nothing at the blast's reach: a shell's burst, small beside the members
    /// it meets.
    Even,
    /// Whole out to `core` (m), then as the inverse square of the distance, as a blast
    /// wave spreads over ever more ground and air: a big blast wrecks what it goes off
    /// beside and only scratches what stands a few metres off.
    Spreading { core: f64 },
}

impl BlastFalloff {
    /// The share of a blast's pressure a part `gap` metres from it takes, within `reach`.
    pub fn share(self, gap: f64, reach: f64) -> f64 {
        match self {
            Self::Even => 1.0 - gap / reach,
            Self::Spreading { core } => (core / gap.max(core)).powi(2),
        }
    }
}

const DRUM_BLAST_RADIUS: f64 = 6.0;

pub const COMBAT: CombatRules = CombatRules {
    projectile_lifetime: 10.0,
    spread_angle: 0.19,
    rocket_blast_radius: 5.3,
    rocket_top_speed_multiplier: 2.5,
    rocket_acceleration_seconds: 1.0,
    tow_turn_rate: 1.35,
    rapid_reload_multiplier: 0.5,
    muzzle_clearance: 0.001,
    bounce_clearance: 0.025,
    contact_time_epsilon: 1e-8,
    separation_epsilon: 1e-6,
    contacts_per_shot: 8,
    minimum_blast_damage_fraction: 0.25,
    blast_impulse: 9.0,
    minimum_blast_distance: 0.1,
    cover_blast_allowance: 0.35,
    drum_blast_radius: DRUM_BLAST_RADIUS,
    drum_damage: 75.0,
    drum_structure_blast: StructureBlast {
        pressure: DRUM_STRUCTURE_PRESSURE,
        falloff: BlastFalloff::Spreading { core: BLAST_WAVE_CORE },
        reach: DRUM_BLAST_RADIUS,
        shatter: DRUM_STRUCTURE_SHATTER,
        throw: DRUM_STRUCTURE_THROW,
    },
    death_structure_blast: StructureBlast {
        pressure: DEATH_STRUCTURE_PRESSURE,
        falloff: BlastFalloff::Spreading { core: BLAST_WAVE_CORE },
        reach: DEATH_STRUCTURE_REACH,
        shatter: DEATH_STRUCTURE_SHATTER,
        throw: DEATH_STRUCTURE_THROW,
    },
};

/// A big blast's whole pressure reaches this far (m) from where it goes off.
const BLAST_WAVE_CORE: f64 = 1.0;
/// A drum snaps a watchtower's leg (strength 90) or a cottage's corner post within about
/// 1.3 m of it, a brace, girt or timber post (60) within 1.6 m, and knocks out timber
/// beams and cottage wall bays (about 35) within 2.1 m; a leg 5 m off takes a fourteenth
/// of its strength.
const DRUM_STRUCTURE_PRESSURE: f64 = 160.0;
const DRUM_STRUCTURE_SHATTER: f64 = 1.0;
const DRUM_STRUCTURE_THROW: f64 = 3.0;
/// A tank blowing up against a watchtower's leg breaks it and knocks the timber beams
/// beside it out of their slots; 3 m off it leaves a beam standing.
const DEATH_STRUCTURE_PRESSURE: f64 = 135.0;
const DEATH_STRUCTURE_REACH: f64 = 3.0;
const DEATH_STRUCTURE_SHATTER: f64 = 1.0;
const DEATH_STRUCTURE_THROW: f64 = 1.8;

pub struct MineRules {
    pub damage: f64,
    pub arm_seconds: f64,
    pub lifetime_seconds: f64,
    pub cooldown_seconds: f64,
    pub trigger_radius: f64,
    pub blast_radius: f64,
}

pub const MINE: MineRules = MineRules {
    damage: 100.0,
    arm_seconds: 0.8,
    lifetime_seconds: 25.0,
    cooldown_seconds: 7.0,
    trigger_radius: 2.5,
    blast_radius: 5.7,
};
