import type { Simulation } from "./simulation";
import type { DamageSource, Tank } from "./types";

export function newCombatRecord() {
  return {
    lifeStarted: 0,
    longestLife: 0,
    recentKills: [] as number[],
    busiestMinute: 0,
    multikill: 0,
    revengeTarget: -1,
    revengeKills: 0,
    clutchKills: 0,
    posthumousKills: 0,
    mineKills: 0,
    coverDestroyed: 0,
    pickups: 0,
    shots: 0,
    directHits: 0,
    damageTaken: 0,
    shieldAbsorbed: 0,
  };
}

export function recordDeath(simulation: Simulation, victim: Tank, owner: number): void {
  if (!simulation.records(victim)) {
    return;
  }
  const stats = simulation.combatRecord;
  stats.longestLife = Math.max(stats.longestLife, simulation.elapsed - stats.lifeStarted);
  stats.revengeTarget = owner === victim.id ? -1 : owner;
}

/** Called only for credited enemy kills; these counters never affect combat or RNG. */
export function recordKill(
  simulation: Simulation,
  killer: Tank,
  victim: Tank,
  ownerLife?: number,
  source?: DamageSource,
): void {
  if (!simulation.records(killer)) {
    return;
  }
  const stats = simulation.combatRecord;
  // Keep only a sliding minute, with a hard safety bound for custom stress worlds.
  stats.recentKills = stats.recentKills.filter((time) => simulation.elapsed - time < 60);
  stats.recentKills.push(simulation.elapsed);
  if (stats.recentKills.length > 4096) {
    stats.recentKills.shift();
  }
  stats.busiestMinute = Math.max(stats.busiestMinute, stats.recentKills.length);
  stats.multikill = Math.max(
    stats.multikill,
    stats.recentKills.filter((time) => simulation.elapsed - time < 5).length,
  );
  const currentLife = killer.alive && (ownerLife === undefined || ownerLife === killer.life);
  if (!currentLife) {
    stats.posthumousKills++;
  } else if (killer.hp <= simulation.maxHealth(killer) * 0.25) {
    stats.clutchKills++;
  }
  if (victim.id === stats.revengeTarget) {
    stats.revengeKills++;
    stats.revengeTarget = -1;
  }
  if (source?.cause === "mine") {
    stats.mineKills++;
  }
}

export function longestLife(simulation: Simulation): number {
  const stats = simulation.combatRecord;
  return Math.max(
    stats.longestLife,
    simulation.human.alive ? simulation.elapsed - stats.lifeStarted : 0,
  );
}
