import RAPIER from "@dimforge/rapier3d-compat";
import { COMBAT, MINE } from "./combat-rules";
import {
  distance,
  GROUP,
  INTERCEPTION_BLAST_RADIUS,
  INTERCEPTION_RADIUS,
  LASER_DEFENSE,
  MINE_RADIUS,
  PICKUPS,
  WEAPONS,
} from "./data";
import { SHELL_HIT_RADIUS, tankHitTime, tankMuzzle } from "./hitboxes";
import { laserContactTime } from "./laser-defense";
import type { Simulation } from "./simulation";
import type { Mine, Shot, Tank } from "./types";
/** Continuous relative-motion contact, including shots that cross between ticks. */
export function interceptionTime(a: Shot, b: Shot, limit: number): number | null {
  if (a.team === b.team || a.piercedShot === b.id || b.piercedShot === a.id) {
    return null;
  }
  const x = a.x - b.x;
  const z = a.z - b.z;
  const vx = a.vx - b.vx;
  const vz = a.vz - b.vz;
  const c = x * x + z * z - INTERCEPTION_RADIUS ** 2;
  if (c <= 0) {
    return 0;
  }
  const speed2 = vx * vx + vz * vz;
  const approach = x * vx + z * vz;
  if (speed2 === 0 || approach >= 0) {
    return null;
  }
  const discriminant = approach * approach - speed2 * c;
  if (discriminant < 0) {
    return null;
  }
  const time = (-approach - Math.sqrt(discriminant)) / speed2;
  return time <= limit ? time : null;
}

function intercept(simulation: Simulation, a: Shot, b: Shot): void {
  const point = { x: (a.x + b.x) / 2, z: (a.z + b.z) / 2 };
  const radius =
    a.weapon === "rocket" || b.weapon === "rocket"
      ? COMBAT.rocketBlastRadius
      : INTERCEPTION_BLAST_RADIUS;
  simulation.events.push({ type: "explosion", ...point, size: radius, color: 0xfff0b4 });
  // One standard hit, like V-Tanks. Each team receives the opposing shell's
  // damage ownership: both sides can be hurt, without double damage or ally fire.
  // This blast only hits tanks; it does not invent cover/mine chain reactions.
  for (const tank of simulation.tanks) {
    if (!tank.alive || distance(tank.body.translation(), point) >= radius) {
      continue;
    }
    const enemyShot = a.team !== tank.team ? a : b;
    simulation.damageTank(
      tank,
      WEAPONS.standard.damage,
      enemyShot.owner,
      enemyShot.team,
      enemyShot.ownerLife,
      { cause: "interception", origin: point },
    );
  }
}

function mineHitTime(shot: Shot, mine: Mine, limit: number): number | null {
  const x = shot.x - mine.x;
  const z = shot.z - mine.z;
  const c = x * x + z * z - (MINE_RADIUS + SHELL_HIT_RADIUS) ** 2;
  if (c <= 0) {
    return 0;
  }
  const speed2 = shot.vx ** 2 + shot.vz ** 2;
  const approach = x * shot.vx + z * shot.vz;
  if (speed2 === 0 || approach >= 0) {
    return null;
  }
  const discriminant = approach ** 2 - speed2 * c;
  if (discriminant < 0) {
    return null;
  }
  const time = (-approach - Math.sqrt(discriminant)) / speed2;
  return time >= 0 && time <= limit ? time : null;
}

export function stepProjectiles(simulation: Simulation, dt: number, sweepTankMotion = false): void {
  simulation.shots = simulation.shots.filter((shot) => shot.life > 0);
  // Accelerate once per fixed tick, before all continuous collision sweeps.
  // Contact retries within this tick must not apply thrust again.
  const rocketTopSpeed = WEAPONS.rocket.speed * COMBAT.rocketTopSpeedMultiplier;
  const rocketAcceleration =
    (rocketTopSpeed - WEAPONS.rocket.speed) / COMBAT.rocketAccelerationSeconds;
  for (const shot of simulation.shots) {
    if (shot.weapon !== "rocket") {
      continue;
    }
    const speed = Math.hypot(shot.vx, shot.vz);
    if (speed > 0 && speed < rocketTopSpeed) {
      const scale =
        Math.min(rocketTopSpeed, speed + rocketAcceleration * Math.min(dt, shot.life)) / speed;
      shot.vx *= scale;
      shot.vz *= scale;
    }
  }
  let remaining = dt;
  // Resolve the earliest contact across all shells, then query again after any
  // bounce/destruction. A wall or tank hit cannot be undone by a later intercept.
  const defenses = simulation.tanks.filter((tank) => tank.alive && tank.laser > 0).length;
  const budget = simulation.shots.length * (COMBAT.contactsPerShot + defenses) + 1;
  for (
    let event = 0;
    remaining > COMBAT.contactTimeEpsilon && simulation.shots.length && event < budget;
    event++
  ) {
    const { next, time } = findNextContact(
      simulation,
      remaining,
      dt - remaining,
      sweepTankMotion ? dt : 0,
      defenses > 0,
    );
    for (const shot of simulation.shots) {
      shot.x += shot.vx * time;
      shot.z += shot.vz * time;
      shot.life -= time;
    }
    remaining -= time;
    if (!next) {
      break;
    }
    const remove = resolveContact(simulation, next, sweepTankMotion ? (dt - remaining) / dt : 1);
    if (remove) {
      simulation.shots.splice(simulation.shots.indexOf(next.shot), 1);
    }
  }
}
function wColor(weapon: keyof typeof WEAPONS): number {
  return WEAPONS[weapon].color;
}

type Contact =
  | { kind: "world"; shot: Shot; hit: RAPIER.RayColliderIntersection }
  | { kind: "tank"; shot: Shot; tank: Tank }
  | { kind: "mine"; shot: Shot; mine: Mine }
  | { kind: "pair"; shot: Shot; other: Shot }
  | { kind: "laser"; shot: Shot; tank: Tank }
  | { kind: "expiry"; shot: Shot };

/** Query without moving entities; equal-time contacts preserve the original priority order. */
function findNextContact(
  simulation: Simulation,
  limit: number,
  elapsed: number,
  tankFrameDelta: number,
  defenses: boolean,
): { next: Contact | null; time: number } {
  let next: Contact | null = null;
  let time = limit;
  for (const shot of simulation.shots) {
    if (shot.life <= time) {
      time = shot.life;
      next = { kind: "expiry", shot: shot };
    }
    const speed = Math.hypot(shot.vx, shot.vz);
    const hit =
      speed > 0
        ? simulation.world.castRayAndGetNormal(
            new RAPIER.Ray(
              { x: shot.x, y: 1, z: shot.z },
              { x: shot.vx / speed, y: 0, z: shot.vz / speed },
            ),
            speed * time,
            true,
            undefined,
            GROUP.coverQuery,
          )
        : null;
    if (hit && hit.timeOfImpact / speed <= time) {
      time = hit.timeOfImpact / speed;
      next = { kind: "world", shot: shot, hit };
    }
    for (const tank of simulation.tanks) {
      const contact = tankHitTime(shot, tank, time, elapsed, tankFrameDelta);
      if (contact !== null && (contact < time || !next)) {
        time = contact;
        next = { kind: "tank", shot: shot, tank };
      }
      if (defenses && tank.laser > 0) {
        const laser = laserContactTime(simulation, shot, tank, time, elapsed, tankFrameDelta);
        if (laser !== null && (laser < time || !next)) {
          time = laser;
          next = { kind: "laser", shot: shot, tank };
        }
      }
    }
    for (const mine of simulation.mines) {
      const contact = mineHitTime(shot, mine, time);
      if (contact !== null && (contact < time || !next)) {
        time = contact;
        next = { kind: "mine", shot: shot, mine };
      }
    }
  }
  for (let i = 0; i < simulation.shots.length; i++) {
    for (let j = i + 1; j < simulation.shots.length; j++) {
      const a = simulation.shots[i];
      const b = simulation.shots[j];
      const contact = interceptionTime(a, b, time);
      if (contact !== null && (contact < time || !next)) {
        // Generous shell contact radii must not reach through thin cover.
        const ax = a.x + a.vx * contact;
        const az = a.z + a.vz * contact;
        const bx = b.x + b.vx * contact;
        const bz = b.z + b.vz * contact;
        const separation = Math.hypot(bx - ax, bz - az);
        if (
          separation > COMBAT.separationEpsilon &&
          simulation.world.castRay(
            new RAPIER.Ray(
              { x: ax, y: 1, z: az },
              { x: (bx - ax) / separation, y: 0, z: (bz - az) / separation },
            ),
            separation,
            true,
            undefined,
            GROUP.coverQuery,
          )
        ) {
          continue;
        }
        time = contact;
        next = { kind: "pair", shot: a, other: b };
      }
    }
  }
  return { next, time };
}

/** Apply one contact. Return whether its primary shot should be removed. */
function resolveContact(simulation: Simulation, next: Contact, fraction: number): boolean {
  const shot = next.shot;
  let remove = true;
  if (next.kind === "laser") {
    (shot.laserCheckedBy ??= []).push(next.tank.id);
    remove = simulation.rng.next() < LASER_DEFENSE.chance;
    if (remove) {
      const tank = next.tank;
      const end = tank.body.translation();
      simulation.events.push({
        type: "laser",
        x: shot.x,
        z: shot.z,
        height: shot.y ?? 1,
        from: {
          x: tank.previous.x + (end.x - tank.previous.x) * fraction,
          y: end.y - 0.4 + tankMuzzle(tank.kind).y + 0.3,
          z: tank.previous.z + (end.z - tank.previous.z) * fraction,
        },
        id: tank.id,
        team: tank.team,
        color: PICKUPS.laser.color,
        size: 0.35,
      });
    }
    // A successful zap vaporizes the shell without triggering a rocket blast.
  } else if (next.kind === "pair") {
    const other = next.other;
    const aPierces = shot.piercing > 0;
    const bPierces = other.piercing > 0;
    if (aPierces || bPierces) {
      simulation.events.push({
        type: "impact",
        x: (shot.x + other.x) / 2,
        z: (shot.z + other.z) / 2,
        size: 0.35,
        color: WEAPONS.piercing.color,
      });
      if (aPierces) {
        shot.piercing--;
      }
      if (bPierces) {
        other.piercing--;
      }
      if (aPierces && bPierces) {
        shot.piercedShot = other.id;
        other.piercedShot = shot.id;
      }
      remove = !aPierces;
      if (!bPierces) {
        simulation.shots.splice(simulation.shots.indexOf(other), 1);
      }
    } else {
      intercept(simulation, shot, other);
      simulation.shots.splice(simulation.shots.indexOf(other), 1);
    }
  } else if (next.kind === "mine") {
    // Remove first so the blast cannot rediscover and detonate this mine twice.
    simulation.mines.splice(simulation.mines.indexOf(next.mine), 1);
    simulation.explode(
      next.mine,
      MINE.blastRadius,
      next.mine.damage ?? MINE.damage,
      shot.owner,
      shot.team,
      shot.ownerLife,
      "mine",
    );
  } else if (next.kind === "tank") {
    if (shot.weapon === "rocket") {
      simulation.explode(
        shot,
        COMBAT.rocketBlastRadius,
        shot.damage,
        shot.owner,
        shot.team,
        shot.ownerLife,
        "rocket",
      );
    } else {
      const position = next.tank.body.translation();
      const speed = Math.hypot(shot.vx, shot.vz) || 1;
      simulation.damageTank(next.tank, shot.damage, shot.owner, shot.team, shot.ownerLife, {
        cause: shot.weapon,
        origin: { x: position.x - shot.vx / speed, z: position.z - shot.vz / speed },
      });
    }
    simulation.events.push({
      type: "impact",
      x: shot.x,
      z: shot.z,
      size: 0.6,
      color: wColor(shot.weapon),
    });
  } else if (next.kind === "world") {
    const hit = next.hit;
    const cover = simulation.coverByCollider.get(hit.collider.handle);
    if (shot.weapon === "rocket") {
      simulation.explode(
        shot,
        COMBAT.rocketBlastRadius,
        shot.damage,
        shot.owner,
        shot.team,
        shot.ownerLife,
        "rocket",
      );
    } else if (cover) {
      simulation.damageCover(cover, shot.damage, shot.owner, shot.team, shot.ownerLife);
      if (cover.alive && shot.bounces > 0) {
        const dot = shot.vx * hit.normal.x + shot.vz * hit.normal.z;
        shot.vx -= 2 * dot * hit.normal.x;
        shot.vz -= 2 * dot * hit.normal.z;
        shot.bounces--;
        shot.x += hit.normal.x * COMBAT.bounceClearance;
        shot.z += hit.normal.z * COMBAT.bounceClearance;
        simulation.events.push({ type: "ricochet", ...shot, size: 0.6 });
        remove = false;
      }
    }
    const chipped = cover?.alive && ["tree", "timber", "cargo"].includes(cover.kind);
    simulation.events.push({
      type: "impact",
      x: shot.x,
      z: shot.z,
      size: 0.6,
      color: chipped ? cover.color : wColor(shot.weapon),
      coverKind: chipped ? cover.kind : undefined,
      height: chipped ? cover.h : undefined,
    });
  }
  return remove;
}
