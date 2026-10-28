//! The engine's integration tests, one module per area, built as one test binary: as
//! separate binaries each file compiled and linked against the engine and Rapier by
//! itself, which made every test build after a `sloppy-core` edit half again as costly,
//! and their tests ran one binary at a time. Add a new area as a module here rather than
//! a file directly under `tests/`, which Cargo would build as another binary.
//! `cargo test --workspace --test integration navigation::` runs one area.

mod net_support;
mod support;

mod ammunition;
mod bot_movement;
mod cover_destruction;
mod cover_hit_effects;
mod debris_cleanup;
mod destruction_physics;
mod difficulty;
mod dodger_first;
mod driving;
mod game;
mod game_options;
mod hit_registration;
mod house_destruction;
mod humvee;
mod initial_state;
mod laser_defense;
mod map_layout;
mod modes;
mod movable_barrels;
mod navigation;
mod net_client;
mod net_extra_levels;
mod net_golden;
mod net_match_host;
mod net_player_controls;
mod net_replication;
mod net_timing;
mod own_shots;
mod personalities;
mod prediction;
mod quarry;
mod render_state;
mod resting_stones;
mod round_recap;
mod shoot_mines;
mod shooter_first;
mod simulation_setup;
mod stress_test_level;
mod superstress_level;
mod tank_contact;
mod tank_destruction;
mod timber_walls;
mod veterancy;
