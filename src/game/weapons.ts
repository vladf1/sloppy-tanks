import RAPIER from "@dimforge/rapier3d-compat";
import { consumeAmmo, equippedWeapon } from "./ammunition";
import { COMBAT } from "./combat-rules";
import { GROUP, PLAYER_FIRE_RATE_MULTIPLIER, TEAM_COLORS, WEAPONS } from "./data";
import { tankHitTime, tankMuzzle } from "./hitboxes";
import type { Simulation } from "./simulation";
import type { Shot, Tank } from "./types";
import { rankStats } from "./veterancy";
// Preserve the combat API used by existing test and browser harnesses.
export { placeMine, stepMines } from "./mines";
export { collectPickup } from "./pickups";
export { interceptionTime, stepProjectiles } from "./projectiles";
export function fireWeapon(simulation: Simulation, tank: Tank): void {
  if (!tank.alive || tank.cooldown > 0) {
    return;
  }
  tank.protection = 0;
  tank.lastCombat = simulation.elapsed;
  tank.cooldown = weaponInterval(tank);
  tank.recoil = 1;
  const position = tank.body.translation();
  const weapon = equippedWeapon(tank);
  const w = WEAPONS[weapon];
  const muzzle = tankMuzzle(tank.kind);
  const direction = { x: Math.sin(tank.aim), y: 0, z: Math.cos(tank.aim) };
  // Trace to the muzzle so a barrel poking into cover or a tank cannot shoot through it.
  let spawnDistance = muzzle.z;
  const coverHit = simulation.world.castRay(
    new RAPIER.Ray({ x: position.x, y: 1, z: position.z }, direction),
    spawnDistance,
    true,
    undefined,
    GROUP.coverQuery,
  );
  if (coverHit) {
    spawnDistance = Math.min(spawnDistance, coverHit.timeOfImpact);
  }
  const probe: Shot = {
    id: 0,
    x: position.x,
    z: position.z,
    vx: direction.x,
    vz: direction.z,
    owner: tank.id,
    team: tank.team,
    damage: 0,
    bounces: 0,
    life: 0,
    weapon,
    piercing: 0,
  };
  for (const target of simulation.tanks) {
    const hit = tankHitTime(probe, target, spawnDistance);
    if (hit !== null) {
      spawnDistance = Math.min(spawnDistance, hit);
    }
  }
  if (spawnDistance < muzzle.z) {
    spawnDistance = Math.max(0, spawnDistance - COMBAT.muzzleClearance);
  }
  for (const offset of weapon === "spread" ? [-COMBAT.spreadAngle, 0, COMBAT.spreadAngle] : [0]) {
    const angle = tank.aim + offset;
    simulation.shots.push({
      id: simulation.nextId++,
      x: position.x + direction.x * spawnDistance,
      z: position.z + direction.z * spawnDistance,
      y: position.y - 0.4 + muzzle.y,
      vx: Math.sin(angle) * w.speed,
      vz: Math.cos(angle) * w.speed,
      damage: w.damage * rankStats(tank).damage,
      owner: tank.id,
      ownerLife: tank.deaths,
      team: tank.team,
      bounces: w.bounces,
      piercing: weapon === "piercing" ? 1 : 0,
      // Rockets accelerate during flight; all rounds share the expiry limit.
      life: COMBAT.projectileLifetime,
      weapon,
    });
    simulation.shotsFired++;
  }
  consumeAmmo(tank, weapon);
  if (tank.human && weapon !== "standard" && tank.selectedAmmo === "standard") {
    simulation.events.push({
      type: "notice",
      id: tank.id,
      x: position.x,
      z: position.z,
      label: `${w.label} EMPTY — switched to STANDARD (unlimited)`,
    });
  }
  simulation.events.push({
    type: "shot",
    weapon,
    x: position.x + direction.x * muzzle.z,
    z: position.z + direction.z * muzzle.z,
    id: tank.id,
    team: tank.team,
    size: weapon === "rocket" ? 1.5 : 1,
    color: TEAM_COLORS[tank.team],
  });
}
export function weaponInterval(tank: Tank): number {
  return (
    (WEAPONS[equippedWeapon(tank)].interval * (tank.rapid > 0 ? COMBAT.rapidReloadMultiplier : 1)) /
    ((tank.human ? PLAYER_FIRE_RATE_MULTIPLIER : 1) * rankStats(tank).fireRate)
  );
}
