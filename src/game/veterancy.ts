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

export function rankIndex(t: Pick<Tank, "xp">) {
  for (let i = RANKS.length - 1; i > 0; i--) if (t.xp >= RANKS[i].xp) return i;
  return 0;
}
export function rankStats(t: Pick<Tank, "xp">) { return RANKS[rankIndex(t)]; }

export function earnExperience(s: Simulation, t: Tank, amount: number, ownerLife?: number) {
  // A mine/shell from a destroyed tank must not promote its replacement.
  if (!t.alive || amount <= 0 || (ownerLife !== undefined && ownerLife !== t.deaths)) return;
  const before = rankIndex(t), oldMax = s.maxHealth(t);
  t.xp = Math.min(RANKS.at(-1)!.xp, t.xp + amount);
  const after = rankIndex(t);
  if (after === before) return;
  // Preserve the hull percentage: promotion is a capacity upgrade, not a full repair.
  t.hp = Math.min(s.maxHealth(t), t.hp / oldMax * s.maxHealth(t));
  const reloadScale = RANKS[before].fireRate / RANKS[after].fireRate;
  t.cooldown *= reloadScale;
  t.brain.fireDelay *= reloadScale;
  const p = t.body.translation();
  s.events.push({ type: "promotion", id: t.id, team: t.team, x: p.x, z: p.z,
    label: `PROMOTED TO ${RANKS[after].name.toUpperCase()}`, color: 0xffd477 });
}

export function repairVeteran(s: Simulation, t: Tank, dt: number) {
  const rate = rankStats(t).repair;
  if (!t.alive || !rate || s.elapsed - t.lastCombat < REPAIR_DELAY) return;
  const max = s.maxHealth(t);
  t.hp = Math.min(max, t.hp + max * rate * dt);
}
