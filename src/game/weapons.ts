import RAPIER from "@dimforge/rapier3d-compat";
import { WEAPONS, PICKUPS, VEHICLES, TEAM_COLORS, distance } from "./data";
import type { Simulation } from "./simulation";
import type { Tank, Pickup } from "./types";
export function fireWeapon(s: Simulation, t: Tank) {
  if (!t.alive || t.cooldown > 0) return;
  t.protection = 0;
  t.cooldown = WEAPONS[t.weapon].interval;
  t.recoil = 1;
  const p = t.body.translation(),
    w = WEAPONS[t.weapon];
  for (const offset of t.weapon === "spread" ? [-0.19, 0, 0.19] : [0]) {
    const angle = t.aim + offset;
    s.shots.push({
      id: s.nextId++,
      x: p.x + Math.sin(angle) * 0.6,
      z: p.z + Math.cos(angle) * 0.6,
      vx: Math.sin(angle) * w.speed,
      vz: Math.cos(angle) * w.speed,
      damage: w.damage,
      owner: t.id,
      team: t.team,
      bounces: w.bounces,
      // Preserve travel range while giving players 25% more flight time.
      life: 3.5,
      weapon: t.weapon,
    });
    s.shotsFired++;
  }
  s.events.push({
    type: "shot",
    x:
      p.x +
      Math.sin(t.aim) *
        (t.kind === "scout" ? 2.28 : 2.48) *
        VEHICLES[t.kind].scale,
    z:
      p.z +
      Math.cos(t.aim) *
        (t.kind === "scout" ? 2.28 : 2.48) *
        VEHICLES[t.kind].scale,
    id: t.id,
    team: t.team,
    size: t.weapon === "rocket" ? 1.5 : 1,
    color: TEAM_COLORS[t.team],
  });
}
export function stepProjectiles(s: Simulation, dt: number) {
  for (let i = s.shots.length - 1; i >= 0; i--) {
    const p = s.shots[i];
    p.life -= dt;
    let remaining = dt;
    let remove = p.life <= 0;
    for (let attempt = 0; !remove && remaining > 0 && attempt < 6; attempt++) {
      const speed = Math.hypot(p.vx, p.vz),
        dx = p.vx / speed,
        dz = p.vz / speed;
      const ray = new RAPIER.Ray(
        { x: p.x, y: 1, z: p.z },
        { x: dx, y: 0, z: dz },
      );
      const hit = s.world.castRayAndGetNormal(
        ray,
        speed * remaining,
        true,
        undefined,
        undefined,
        undefined,
        undefined,
        (c) => {
          const tank = s.tanks.find(
            (t) => t.alive && t.collider.handle === c.handle,
          );
          if (tank) return tank.team !== p.team;
          return s.covers.some(
            (o) => o.alive && o.collider.handle === c.handle,
          );
        },
      );
      if (!hit) {
        p.x += p.vx * remaining;
        p.z += p.vz * remaining;
        break;
      }
      p.x += dx * hit.timeOfImpact;
      p.z += dz * hit.timeOfImpact;
      remaining -= hit.timeOfImpact / speed;
      const tank = s.tanks.find(
          (t) => t.alive && t.collider.handle === hit.collider.handle,
        ),
        cover = s.covers.find(
          (c) => c.alive && c.collider.handle === hit.collider.handle,
        );
      if (p.weapon === "rocket") {
        s.explode(p, 5.3, p.damage, p.owner, p.team);
        remove = true;
      } else if (tank) {
        s.damageTank(tank, p.damage, p.owner, p.team);
        remove = true;
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
        } else remove = true;
      } else remove = true;
      s.events.push({
        type: "impact",
        x: p.x,
        z: p.z,
        size: 0.6,
        color: wColor(p.weapon),
      });
    }
    if (remove) s.shots.splice(i, 1);
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
  if (kind === "repair") t.hp = Math.min(VEHICLES[t.kind].health, t.hp + 65);
  else if (kind === "shield") t.shield = PICKUPS[kind].duration;
  else if (kind === "speed") t.speed = PICKUPS[kind].duration;
  else {
    t.weapon = kind;
    t.weaponTime = PICKUPS[kind].duration;
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
