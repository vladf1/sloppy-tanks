//! Authoritative gameplay on Rapier: the fixed-step simulation, bots, navigation, combat,
//! maps and levels. Modules follow the former TypeScript files one-to-one.
//!
//! Contracts (see AGENTS.md): a fixed `STEP` of 1/60 s with an intentional update order in
//! `Simulation::step`; the seeded `Random` stream and its draw order are gameplay state;
//! game math is f64 like the former JS numbers and narrows to f32 only at the Rapier
//! boundary; humans and bots drive through the same `VehicleCommand`.

pub mod ai;
pub mod ammunition;
pub mod arena;
pub mod barrel_physics;
pub mod bot_movement;
pub mod bot_personalities;
pub mod bot_strategy;
pub mod combat_record;
pub mod combat_rules;
pub mod damage;
pub mod data;
pub mod debris_cleanup;
pub mod debris_physics;
pub mod difficulty;
pub mod extra_levels;
pub mod fragments;
pub mod game_options;
pub mod harbor_layout;
pub mod hitboxes;
pub mod humvee_tactics;
pub mod laser_defense;
pub mod level_rules;
pub mod map_options;
pub mod maps;
pub mod match_state;
pub mod math;
pub mod mines;
pub mod movable_cover;
pub mod navigation;
pub mod physics;
pub mod pickups;
pub mod projectiles;
pub mod quarry_barrier_shapes;
pub mod quarry_layout;
pub mod quarry_rock_shape;
pub mod render_state;
pub mod round_recap;
pub mod scenery_pieces;
pub mod simulation;
pub mod simulation_rules;
pub mod speed_tuning;
pub mod stress_test_level;
pub mod superstress_level;
pub mod tank_destruction;
pub mod tank_dimensions;
pub mod tank_driving;
pub mod tank_lifecycle;
pub mod timber_layout;
pub mod tower_layout;
pub mod tree_proportions;
pub mod types;
pub mod veterancy;
pub mod weapons;
pub mod wrecks;

pub use render_state::RenderState;
pub use simulation::{GameMode, Simulation, SimulationSetup, Snapshot};
pub use types::*;
