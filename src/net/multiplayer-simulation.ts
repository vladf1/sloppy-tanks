import { Simulation, type SimulationSetup } from "../game/simulation";
import { idleCommand, type PlayerAssignment, type Tank } from "../game/types";

export const MAX_PLAYERS = 8;
export const TEAM_SLOTS = 6;
export const MAX_PLAYER_NAME_LENGTH = 24;
/** The multiplayer roster is optional, so local play never imports seat validation. */
export function createMultiplayerSimulation(
  seed: number,
  players: readonly PlayerAssignment[],
  options: Pick<SimulationSetup, "mapMode" | "difficulty" | "round" | "humansOnly"> = {},
): Simulation {
  if (players.length > MAX_PLAYERS) {
    throw new Error("Room has at most eight players");
  }
  const ids = new Set<string>();
  const slots = new Set<string>();
  for (const player of players) {
    if (
      typeof player.playerId !== "string" ||
      !player.playerId ||
      ids.has(player.playerId) ||
      typeof player.name !== "string" ||
      !player.name.trim() ||
      player.name.length > MAX_PLAYER_NAME_LENGTH ||
      (player.team !== 0 && player.team !== 1) ||
      !Number.isInteger(player.slot) ||
      player.slot < 0 ||
      player.slot >= TEAM_SLOTS ||
      !["scout", "balanced", "heavy"].includes(player.kind) ||
      slots.has(`${player.team}:${player.slot}`)
    ) {
      throw new Error("Invalid or occupied player seat");
    }
    ids.add(player.playerId);
    slots.add(`${player.team}:${player.slot}`);
  }
  return new Simulation(seed, {
    ...options,
    gameMode: "team",
    roundCount: TEAM_SLOTS * 2,
    players: players.map((player) => Object.freeze({ ...player, name: player.name.trim() })),
  });
}

/** Bot takeover changes the driver, never the player's tank, balance, score or life. */
export function setDriver(tank: Tank, driver: Tank["driver"]): void {
  if (driver === "human" && !tank.human) {
    throw new Error("A fill bot has no player seat");
  }
  tank.driver = driver;
  tank.command = { ...idleCommand(), aim: tank.aim };
  tank.brain.decision = 0;
}

/** Reusing a slot starts a fresh life and score without manufacturing a team kill. */
export function reassignTank(simulation: Simulation, tank: Tank, player?: PlayerAssignment): void {
  if (!simulation.multiplayer) {
    throw new Error("Only a multiplayer host can reassign seats");
  }
  if (tank.alive) {
    simulation.world.removeRigidBody(tank.body);
  }
  tank.alive = false;
  tank.life++;
  tank.playerId = player?.playerId;
  tank.name = player?.name ?? "BOT";
  tank.human = !!player;
  tank.driver = player ? "human" : "bot";
  tank.kind = player?.kind ?? tank.kind;
  tank.kills = tank.deaths = tank.damageDealt = tank.bestLifeKills = tank.highestRank = 0;
  simulation.respawn(tank);
}

/** Empty human-only seats have no tank or physics body until somebody joins. */
export function claimPlayerTank(simulation: Simulation, player: PlayerAssignment): Tank {
  if (!simulation.humansOnly) {
    const tank = simulation.tanks[player.team + player.slot * 2];
    reassignTank(simulation, tank, player);
    return tank;
  }
  const tank = simulation.addTank(player.team, true, player.kind, player.slot);
  tank.playerId = player.playerId;
  tank.name = player.name;
  return tank;
}

export function releasePlayerTank(simulation: Simulation, tank: Tank): void {
  if (!simulation.humansOnly) {
    reassignTank(simulation, tank);
    return;
  }
  if (tank.alive) {
    simulation.world.removeRigidBody(tank.body);
  }
  tank.alive = false;
  simulation.tanks = simulation.tanks.filter((candidate) => candidate !== tank);
}
