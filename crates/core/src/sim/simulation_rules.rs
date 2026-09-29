//! Physics and lifecycle tuning, in metres and seconds. Positive gravity points down.

pub const GRAVITY: f64 = 22.0;
pub const MAX_FRAGMENTS: usize = 80;
/// The largest fragment budget a custom level may set; debris instancing is sized for it.
pub const FRAGMENT_CAPACITY: usize = 256;

pub struct SimulationRules {
    pub default_seed: f64,
    pub default_tank_count: usize,
    pub spawn_protection_seconds: f64,
    pub respawn_seconds: f64,
    pub pickup_radius: f64,
    pub recoil_recovery_per_second: f64,
    pub max_pending_events: usize,
    /// A new deterministic stream for each round; multiplication is reduced to uint32.
    pub round_seed_stride: f64,
    pub tank_body_height: f64,
    pub tank_linear_damping: f64,
    pub tank_angular_damping: f64,
}

pub const SIMULATION_RULES: SimulationRules = SimulationRules {
    default_seed: 12345.0,
    default_tank_count: 12,
    spawn_protection_seconds: 2.0,
    respawn_seconds: 3.0,
    pickup_radius: 1.8,
    recoil_recovery_per_second: 6.0,
    max_pending_events: 400,
    round_seed_stride: 2_654_435_769.0, // 0x9e3779b9
    tank_body_height: 0.65,
    tank_linear_damping: 0.35,
    tank_angular_damping: 8.0,
};

pub struct SoloRules {
    pub active_enemies: usize,
    pub enemy_health_multiplier: f64,
    pub enemy_damage_multiplier: f64,
    pub reinforcement_seconds: f64,
    pub spawn_x: f64,
    pub spawn_half_span_z: f64,
}

pub const SOLO: SoloRules = SoloRules {
    active_enemies: 6,
    enemy_health_multiplier: 0.4,
    enemy_damage_multiplier: 0.4,
    reinforcement_seconds: 1.0,
    spawn_x: 53.0,
    spawn_half_span_z: 46.0,
};

pub struct SpawnScoring {
    pub maximum_enemy_distance: f64,
    pub visible_enemy_penalty: f64,
    pub ally_clearance: f64,
    pub ally_proximity_penalty: f64,
}

pub const SPAWN_SCORING: SpawnScoring = SpawnScoring {
    maximum_enemy_distance: 50.0,
    visible_enemy_penalty: 12.0,
    ally_clearance: 5.0,
    ally_proximity_penalty: 8.0,
};
