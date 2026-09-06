import { tankHitTime, tankMuzzle, SHELL_HIT_RADIUS } from "./hitboxes";
import { equippedWeapon } from "./bot-personalities";
import RAPIER from "@dimforge/rapier3d-compat";
import {
  WEAPONS, PICKUPS, VEHICLES, TEAM_COLORS, distance,
  SHIELD_CAPACITY, INTERCEPTION_RADIUS, INTERCEPTION_BLAST_RADIUS,
  PLAYER_FIRE_RATE_MULTIPLIER,
  MINE_RADIUS,
} from "./data";
import type { Simulation } from "./simulation";
import type { Tank, Pickup, Shot, Mine } from "./types";
export function fireWeapon(s: Simulation, t: Tank) {
  if (!t.alive || t.cooldown > 0) return;
  t.protection = 0;
  t.cooldown = weaponInterval(t);
  t.recoil = 1;
  const p = t.body.translation(),
    weapon = equippedWeapon(t),
    w = WEAPONS[weapon];
  const muzzle = tankMuzzle(t.kind);
  const direction = { x: Math.sin(t.aim), y: 0, z: Math.cos(t.aim) };
  // Trace to the muzzle so a barrel poking into cover or a tank cannot shoot through it.
  let spawnDistance = muzzle.z;
  const coverHit = s.world.castRay(
    new RAPIER.Ray({ x: p.x, y: 1, z: p.z }, direction), spawnDistance, true,
    undefined, undefined, undefined, undefined,
    (c) => s.covers.some((cover) => cover.alive && cover.collider.handle === c.handle),
  );
  if (coverHit) spawnDistance = Math.min(spawnDistance, coverHit.timeOfImpact);
  const probe: Shot = { id: 0, x: p.x, z: p.z, vx: direction.x, vz: direction.z,
    owner: t.id, team: t.team, damage: 0, bounces: 0, life: 0, weapon };
  for (const target of s.tanks) {
    const hit = tankHitTime(probe, target, spawnDistance);
    if (hit !== null) spawnDistance = Math.min(spawnDistance, hit);
  }
  if (spawnDistance < muzzle.z) spawnDistance = Math.max(0, spawnDistance - 0.001);
  for (const offset of t.spread > 0 ? [-0.19, 0, 0.19] : [0]) {
    const angle = t.aim + offset;
    s.shots.push({
      id: s.nextId++,
      x: p.x + direction.x * spawnDistance,
      z: p.z + direction.z * spawnDistance,
      y: p.y - 0.4 + muzzle.y,
      vx: Math.sin(angle) * w.speed,
      vz: Math.cos(angle) * w.speed,
      damage: w.damage * (t.ricochet > 0 ? 2 : 1),
      owner: t.id,
      team: t.team,
      bounces: w.bounces + (t.ricochet > 0 ? 2 : 0),
      // Preserve travel range while giving players 25% more flight time.
      life: 3.5,
      weapon,
    });
    s.shotsFired++;
  }
  s.events.push({
    type: "shot",
    x: p.x + direction.x * muzzle.z,
    z: p.z + direction.z * muzzle.z,
    id: t.id,
    team: t.team,
    size: weapon === "rocket" ? 1.5 : 1,
    color: TEAM_COLORS[t.team],
  });
}
/** Continuous relative-motion contact, including shots that cross between ticks. */
export function interceptionTime(a: Shot, b: Shot, limit: number): number | null {
  if (a.team === b.team) return null;
  const x = a.x - b.x, z = a.z - b.z;
  const vx = a.vx - b.vx, vz = a.vz - b.vz;
  const c = x * x + z * z - INTERCEPTION_RADIUS ** 2;
  if (c <= 0) return 0;
  const speed2 = vx * vx + vz * vz;
  const approach = x * vx + z * vz;
  if (speed2 === 0 || approach >= 0) return null;
  const discriminant = approach * approach - speed2 * c;
  if (discriminant < 0) return null;
  const time = (-approach - Math.sqrt(discriminant)) / speed2;
  return time <= limit ? time : null;
}

function intercept(s: Simulation, a: Shot, b: Shot) {
  const point = { x: (a.x + b.x) / 2, z: (a.z + b.z) / 2 };
  const radius = a.weapon === "rocket" || b.weapon === "rocket"
    ? 5.3 : INTERCEPTION_BLAST_RADIUS;
  s.events.push({ type: "explosion", ...point, size: radius, color: 0xfff0b4 });
  // One standard hit, like V-Tanks. Each team receives the opposing shell's
  // damage ownership: both sides can be hurt, without double damage or ally fire.
  // This blast only hits tanks; it does not invent cover/mine chain reactions.
  for (const t of s.tanks) {
    if (!t.alive || distance(t.body.translation(), point) >= radius) continue;
    const enemyShot = a.team !== t.team ? a : b;
    s.damageTank(t, WEAPONS.standard.damage, enemyShot.owner, enemyShot.team);
  }
}

function mineHitTime(shot: Shot, mine: Mine, limit: number): number | null {
  const x = shot.x - mine.x, z = shot.z - mine.z;
  const c = x * x + z * z - (MINE_RADIUS + SHELL_HIT_RADIUS) ** 2;
  if (c <= 0) return 0;
  const speed2 = shot.vx ** 2 + shot.vz ** 2;
  const approach = x * shot.vx + z * shot.vz;
  if (speed2 === 0 || approach >= 0) return null;
  const discriminant = approach ** 2 - speed2 * c;
  if (discriminant < 0) return null;
  const time = (-approach - Math.sqrt(discriminant)) / speed2;
  return time >= 0 && time <= limit ? time : null;
}

export function stepProjectiles(s: Simulation, dt: number, sweepTankMotion = false) {
  s.shots = s.shots.filter((p) => p.life > 0);
  let remaining = dt;
  // Resolve the earliest contact across all shells, then query again after any
  // bounce/destruction. A wall or tank hit cannot be undone by a later intercept.
  type Contact =
    | { kind: "world"; shot: Shot; hit: RAPIER.RayColliderIntersection }
    | { kind: "tank"; shot: Shot; tank: Tank }
    | { kind: "mine"; shot: Shot; mine: Mine }
    | { kind: "pair"; shot: Shot; other: Shot }
    | { kind: "expiry"; shot: Shot };
  const budget = s.shots.length * 8 + 1;
  for (let event = 0; remaining > 1e-8 && s.shots.length && event < budget; event++) {
    let next: Contact | null = null;
    let time = remaining;
    for (const p of s.shots) {
      if (p.life <= time) {
        time = p.life;
        next = { kind: "expiry", shot: p };
      }
      const speed = Math.hypot(p.vx, p.vz);
      const hit = speed > 0 ? s.world.castRayAndGetNormal(
        new RAPIER.Ray({ x: p.x, y: 1, z: p.z },
          { x: p.vx / speed, y: 0, z: p.vz / speed }),
        speed * time, true, undefined, undefined, undefined, undefined,
        (c) => s.covers.some((o) => o.alive && o.collider.handle === c.handle),
      ) : null;
      if (hit && hit.timeOfImpact / speed <= time) {
        time = hit.timeOfImpact / speed;
        next = { kind: "world", shot: p, hit };
      }
      for (const tank of s.tanks) {
        const contact = tankHitTime(p, tank, time, dt - remaining, sweepTankMotion ? dt : 0);
        if (contact !== null && (contact < time || !next)) {
          time = contact;
          next = { kind: "tank", shot: p, tank };
        }
      }
      for (const mine of s.mines) {
        const contact = mineHitTime(p, mine, time);
        if (contact !== null && (contact < time || !next)) {
          time = contact;
          next = { kind: "mine", shot: p, mine };
        }
      }
    }
    for (let i = 0; i < s.shots.length; i++) {
      for (let j = i + 1; j < s.shots.length; j++) {
        const a = s.shots[i], b = s.shots[j];
        const contact = interceptionTime(a, b, time);
        if (contact !== null && (contact < time || !next)) {
          // Generous shell contact radii must not reach through thin cover.
          const ax = a.x + a.vx * contact, az = a.z + a.vz * contact;
          const bx = b.x + b.vx * contact, bz = b.z + b.vz * contact;
          const separation = Math.hypot(bx - ax, bz - az);
          if (separation > 1e-6 && s.world.castRay(
            new RAPIER.Ray({ x: ax, y: 1, z: az },
              { x: (bx - ax) / separation, y: 0, z: (bz - az) / separation }),
            separation, true, undefined, undefined, undefined, undefined,
            (c) => s.covers.some((cover) => cover.alive && cover.collider.handle === c.handle),
          )) continue;
          time = contact;
          next = { kind: "pair", shot: a, other: b };
        }
      }
    }
    for (const p of s.shots) {
      p.x += p.vx * time;
      p.z += p.vz * time;
      p.life -= time;
    }
    remaining -= time;
    if (!next) break;
    const p = next.shot;
    let remove = true;
    if (next.kind === "pair") {
      intercept(s, p, next.other);
      s.shots.splice(s.shots.indexOf(next.other), 1);
    } else if (next.kind === "mine") {
      // Remove first so the blast cannot rediscover and detonate this mine twice.
      s.mines.splice(s.mines.indexOf(next.mine), 1);
      s.explode(next.mine, 5.7, 100, p.owner, p.team);
    } else if (next.kind === "tank") {
      if (p.weapon === "rocket") s.explode(p, 5.3, p.damage, p.owner, p.team);
      else s.damageTank(next.tank, p.damage, p.owner, p.team);
      s.events.push({ type: "impact", x: p.x, z: p.z, size: 0.6, color: wColor(p.weapon) });
    } else if (next.kind === "world") {
      const hit = next.hit;
      const cover = s.covers.find((c) => c.alive && c.collider.handle === hit.collider.handle);
      if (p.weapon === "rocket") {
        s.explode(p, 5.3, p.damage, p.owner, p.team);
      } else if (cover) {
        s.damageCover(cover, p.damage, p.owner, p.team);
        if (cover.alive && p.bounces > 0) {
          const dot = p.vx * hit.normal.x + p.vz * hit.normal.z;
          p.vx -= 2 * dot * hit.normal.x;
          p.vz -= 2 * dot * hit.normal.z;
          p.bounces--;
          p.x += hit.normal.x * 0.025;
          p.z += hit.normal.z * 0.025;
          s.events.push({ type: "ricochet", ...p, size: 0.6 });
          remove = false;
        }
      }
      s.events.push({ type: "impact", x: p.x, z: p.z, size: 0.6, color: wColor(p.weapon) });
    }
    if (remove) s.shots.splice(s.shots.indexOf(p), 1);
  }
}
function wColor(w: keyof typeof WEAPONS) {
  return WEAPONS[w].color;
}
export function placeMine(s: Simulation, t: Tank) {
  if (t.mineCooldown > 0 || !t.alive) return;
  const p = t.body.translation();
  s.mines.push({
    id: s.nextId++,
    owner: t.id,
    team: t.team,
    x: p.x,
    z: p.z,
    arm: 0.8,
    life: 25,
  });
  t.mineCooldown = 7;
}
export function stepMines(s: Simulation, dt: number) {
  // A detonation can recursively remove other mines. Iterate stable identities, not mutable indices.
  for (const m of [...s.mines]) {
    if (!s.mines.includes(m)) continue;
    m.arm -= dt;
    m.life -= dt;
    if (
      m.arm <= 0 &&
      s.tanks.some(
        (t) =>
          t.alive &&
          t.team !== m.team &&
          distance(t.body.translation(), m) < 2.5,
      )
    ) {
      s.mines.splice(s.mines.indexOf(m), 1);
      s.explode(m, 5.7, 100, m.owner, m.team);
    } else if (m.life <= 0) s.mines.splice(s.mines.indexOf(m), 1);
  }
}
export function collectPickup(s: Simulation, t: Tank, p: Pickup) {
  if (!p.available) return;
  p.available = false;
  p.cooldown = 13;
  const kind = p.kind;
  if (kind === "repair") t.hp = VEHICLES[t.kind].health;
  else if (kind === "shield") {
    t.shield = PICKUPS[kind].duration;
    t.shieldPoints = SHIELD_CAPACITY;
  } else if (kind === "rapid" || kind === "ricochet") {
    t[kind] = PICKUPS[kind].duration;
    if (kind === "rapid") t.cooldown = Math.min(t.cooldown, weaponInterval(t));
  }
  else if (kind === "speed") t.speed = PICKUPS[kind].duration;
  else {
    t[kind] = PICKUPS[kind].duration;
    t.cooldown = 0;
  }
  s.events.push({
    type: "pickup",
    x: p.x,
    z: p.z,
    id: t.id,
    team: t.team,
    label: PICKUPS[kind].name,
    color: PICKUPS[kind].color,
  });
}

export function weaponInterval(t: Tank) {
  return WEAPONS[equippedWeapon(t)].interval * (t.rapid > 0 ? 0.5 : 1)
    / (t.human ? PLAYER_FIRE_RATE_MULTIPLIER : 1);
}
