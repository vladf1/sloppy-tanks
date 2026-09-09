import { MINE } from "./combat-rules";
import { distance } from "./math";
import type { Simulation } from "./simulation";
import type { Tank } from "./types";
import { rankStats } from "./veterancy";
export function placeMine(simulation: Simulation, tank: Tank): void {
  if (tank.mineCooldown > 0 || !tank.alive) {
    return;
  }
  const position = tank.body.translation();
  simulation.mines.push({
    id: simulation.nextId++,
    owner: tank.id,
    ownerLife: tank.deaths,
    damage: MINE.damage * rankStats(tank).damage,
    team: tank.team,
    x: position.x,
    z: position.z,
    arm: MINE.armSeconds,
    life: MINE.lifetimeSeconds,
  });
  tank.mineCooldown = MINE.cooldownSeconds;
  tank.lastCombat = simulation.elapsed;
}
export function stepMines(simulation: Simulation, dt: number): void {
  // A detonation can recursively remove other mines. Iterate stable identities, not mutable indices.
  for (const m of [...simulation.mines]) {
    if (!simulation.mines.includes(m)) {
      continue;
    }
    m.arm -= dt;
    m.life -= dt;
    if (
      m.arm <= 0 &&
      simulation.tanks.some(
        (tank) =>
          tank.alive &&
          tank.team !== m.team &&
          distance(tank.body.translation(), m) < MINE.triggerRadius,
      )
    ) {
      simulation.mines.splice(simulation.mines.indexOf(m), 1);
      simulation.explode(
        m,
        MINE.blastRadius,
        m.damage ?? MINE.damage,
        m.owner,
        m.team,
        m.ownerLife,
      );
    } else if (m.life <= 0) {
      simulation.mines.splice(simulation.mines.indexOf(m), 1);
    }
  }
}
