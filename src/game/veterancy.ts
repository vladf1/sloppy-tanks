import type { Simulation } from "./simulation";
import type { Tank } from "./types";

/** Per-life combat experience; bonuses apply equally to humans and bots. */
export const RANKS = [
  { name: "Rookie", xp: 0, damage: 1, fireRate: 1, health: 1, repair: 0 },
  { name: "Veteran", xp: 300, damage: 1.1, fireRate: 1.1, health: 1.1, repair: 0 },
  { name: "Elite", xp: 750, damage: 1.2, fireRate: 1.15, health: 1.15, repair: 0.01 },
  { name: "Heroic", xp: 1500, damage: 1.3, fireRate: 1.2, health: 1.2, repair: 0.02 },
] as const;
export const KILL_XP = 50;
export const REPAIR_DELAY = 5;

export function rankIndex(tank: Pick<Tank, "xp">): number {
  for (let i = RANKS.length - 1; i > 0; i--) {
    if (tank.xp >= RANKS[i].xp) {
      return i;
    }
  }
  return 0;
}
export function rankStats(tank: Pick<Tank, "xp">) {
  return RANKS[rankIndex(tank)];
}

export function earnExperience(
  simulation: Simulation,
  tank: Tank,
  amount: number,
  ownerLife?: number,
): void {
  // A mine/shell from a destroyed tank must not promote its replacement.
  if (!tank.alive || amount <= 0 || (ownerLife !== undefined && ownerLife !== tank.deaths)) {
    return;
  }
  const before = rankIndex(tank);
  const oldMax = simulation.maxHealth(tank);
  tank.xp = Math.min(RANKS.at(-1)!.xp, tank.xp + amount);
  const after = rankIndex(tank);
  if (after === before) {
    return;
  }
  // Preserve the hull percentage: promotion is a capacity upgrade, not a full repair.
  tank.hp = Math.min(simulation.maxHealth(tank), (tank.hp / oldMax) * simulation.maxHealth(tank));
  const reloadScale = RANKS[before].fireRate / RANKS[after].fireRate;
  tank.cooldown *= reloadScale;
  tank.brain.fireDelay *= reloadScale;
  const position = tank.body.translation();
  simulation.events.push({
    type: "promotion",
    id: tank.id,
    team: tank.team,
    x: position.x,
    z: position.z,
    label: `PROMOTED TO ${RANKS[after].name.toUpperCase()}`,
    color: 0xffd477,
  });
}

export function repairVeteran(simulation: Simulation, tank: Tank, dt: number): void {
  const rate = rankStats(tank).repair;
  if (!tank.alive || !rate || simulation.elapsed - tank.lastCombat < REPAIR_DELAY) {
    return;
  }
  const max = simulation.maxHealth(tank);
  tank.hp = Math.min(max, tank.hp + max * rate * dt);
}
