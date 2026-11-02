//! Room rosters on the shared simulation (`src/net/multiplayer-simulation.ts`): the seat
//! validation, the twelve (or an extra level's thirty) stable team slots, and seat
//! hand-over between players and fill bots.

use super::protocol::MAX_PLAYER_NAME_LENGTH;
use super::schema::text_length;
use crate::sim::difficulty::Difficulty;
use crate::sim::extra_levels::extra_level;
use crate::sim::map_options::{MapId, is_extra_level};
use crate::sim::simulation::{GameMode, Simulation, SimulationSetup};
use crate::sim::types::{Driver, PlayerAssignment, VehicleCommand, VehicleKind};

pub const MAX_PLAYERS: usize = 8;
pub const TEAM_SLOTS: usize = 6;

/// The room settings a match is built from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MultiplayerOptions {
    pub map_mode: Option<MapId>,
    pub difficulty: Option<Difficulty>,
    pub round: Option<u32>,
    pub humans_only: Option<bool>,
}

/// A team match for the room's seats. Rejects more than eight players, duplicate ids or
/// slots, empty or long names, and chassis a player may not drive.
pub fn create_multiplayer_simulation(
    seed: f64,
    players: &[PlayerAssignment],
    options: MultiplayerOptions,
) -> Result<Simulation, String> {
    if players.len() > MAX_PLAYERS {
        return Err("Room has at most eight players".into());
    }
    let mut roster: Vec<PlayerAssignment> = Vec::with_capacity(players.len());
    for player in players {
        let name = player.name.trim();
        if player.player_id.is_empty()
            || roster.iter().any(|seat| seat.player_id == player.player_id)
            || name.is_empty()
            || text_length(&player.name) > MAX_PLAYER_NAME_LENGTH
            || player.slot >= TEAM_SLOTS
            || !VehicleKind::PLAYABLE.contains(&player.kind)
            || roster
                .iter()
                .any(|seat| seat.team == player.team && seat.slot == player.slot)
        {
            return Err("Invalid or occupied player seat".into());
        }
        roster.push(PlayerAssignment {
            name: name.to_string(),
            ..player.clone()
        });
    }
    let mut setup = SimulationSetup {
        map_mode: options.map_mode,
        difficulty: options.difficulty,
        round: options.round,
        humans_only: options.humans_only,
        game_mode: Some(GameMode::Team),
        round_count: Some(TEAM_SLOTS * 2),
        ..SimulationSetup::default()
    };
    // An extra level brings its own arena, rules and bot roster; human seats stay the same.
    if let Some(level) = options
        .map_mode
        .filter(|map| is_extra_level(*map))
        .and_then(extra_level)
    {
        setup = setup.merged(level);
    }
    setup.players = Some(roster);
    Ok(Simulation::new(seed, setup))
}

/// Bot takeover changes the driver, never the player's tank or balance. The bot stands
/// in under a bot's name with a score of its own, so nobody reads a bot's play as the
/// player's; the player's name returns with them (the host restores their score, which
/// it keeps per seat). Handing over either way starts a new life without a death, so
/// `ownerLife` keeps the stand-in's ordnance, mines included, from ever crediting the
/// player, and the player's from awarding the stand-in XP.
pub fn set_driver(
    simulation: &mut Simulation,
    tank_index: usize,
    driver: Driver,
) -> Result<(), String> {
    if driver == Driver::Human && !simulation.tanks[tank_index].human {
        return Err("A fill bot has no player seat".into());
    }
    // A pilot's tank already shows a bot's name; only a player's is handed over.
    let tank = &simulation.tanks[tank_index];
    let stands_in = driver == Driver::Bot && tank.human && !tank.bot;
    let stand_in_name = stands_in.then(|| simulation.unused_bot_name());
    let tank = &mut simulation.tanks[tank_index];
    if let Some(name) = stand_in_name {
        tank.away_player = Some(std::mem::replace(&mut tank.name, name));
        tank.bot = true;
        tank.kills = 0;
        tank.deaths = 0;
        tank.life += 1;
    } else if driver != Driver::Bot
        && let Some(name) = tank.away_player.take()
    {
        tank.name = name;
        tank.bot = false;
        tank.life += 1;
    }
    tank.driver = driver;
    tank.command = VehicleCommand::idle_aiming(tank.aim);
    tank.brain.decision = 0.0;
    Ok(())
}

/// Reusing a slot starts a fresh life and score without manufacturing a team kill.
pub fn reassign_tank(
    simulation: &mut Simulation,
    tank_index: usize,
    player: Option<&PlayerAssignment>,
) {
    assert!(
        simulation.multiplayer(),
        "Only a multiplayer host can reassign seats"
    );
    if simulation.tanks[tank_index].alive {
        let body = simulation.tanks[tank_index].body;
        simulation.remove_body(body);
    }
    // A fill bot that takes over from a stand-in keeps the stand-in's name, which the
    // field already reads as a bot.
    let tank = &mut simulation.tanks[tank_index];
    let stand_in = tank.away_player.take().is_some();
    let name = match player {
        Some(player) => player.name.clone(),
        None if stand_in => std::mem::take(&mut tank.name),
        None => {
            // The leaving player's name is no longer taken.
            tank.name.clear();
            simulation.unused_bot_name()
        }
    };
    let tank = &mut simulation.tanks[tank_index];
    tank.alive = false;
    tank.life += 1;
    tank.player_id = player.map(|player| player.player_id.clone());
    tank.name = name;
    tank.human = player.is_some();
    tank.bot = player.is_none();
    tank.driver = if player.is_some() {
        Driver::Human
    } else {
        Driver::Bot
    };
    if let Some(player) = player {
        tank.kind = player.kind;
    }
    tank.kills = 0;
    tank.deaths = 0;
    tank.damage_dealt = 0.0;
    tank.best_life_kills = 0;
    tank.highest_rank = 0;
    simulation.respawn(tank_index, None);
}

/// Gives a joining player a tank: a fill bot's slot, or a new tank in a humans-only room
/// (empty human-only seats have no tank or physics body). Returns the tank's index.
pub fn claim_player_tank(simulation: &mut Simulation, player: &PlayerAssignment) -> usize {
    if !simulation.humans_only {
        let index = player.team.index() + player.slot * 2;
        reassign_tank(simulation, index, Some(player));
        return index;
    }
    let index = simulation.add_tank(player.team, true, player.kind, player.slot);
    let tank = &mut simulation.tanks[index];
    tank.player_id = Some(player.player_id.clone());
    tank.name.clone_from(&player.name);
    index
}

/// Hands a departing player's tank back to a bot, or removes it from a humans-only room.
pub fn release_player_tank(simulation: &mut Simulation, tank_id: u32) {
    let Some(index) = simulation.tank_index(tank_id) else {
        return;
    };
    if !simulation.humans_only {
        reassign_tank(simulation, index, None);
        return;
    }
    let tank = simulation.tanks.remove(index);
    if tank.alive {
        simulation.remove_body(tank.body);
    }
}
