//! Shared combat distances are metres; durations are seconds.

pub struct CombatRules {
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
}

pub const COMBAT: CombatRules = CombatRules {
    projectile_lifetime: 3.5,
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
    drum_blast_radius: 6.0,
    drum_damage: 75.0,
};

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
