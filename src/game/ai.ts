import { botProfile, botReload, combatMovement, equippedWeapon } from "./bot-personalities";
import RAPIER from "@dimforge/rapier3d-compat";
import { distance, VEHICLES, WEAPONS, angleDelta } from "./data";
import type { Simulation } from "./simulation";
import { idleCommand, type Tank, type Vec2 } from "./types";
export function botCommand(s: Simulation, t: Tank, dt: number) {
  const role = Math.floor(s.tanks.indexOf(t) / 2);
  const profile = botProfile(t);
  const easy = s.isEasyEnemy(t);
  const aggressive = !easy && t.brain.ultraAggressive;
  const turnSpeed = easy ? 1.5 : aggressive ? Math.max(5.2, profile.turn * 1.4) : profile.turn;
  const turn = (desired: number) => t.aim + Math.max(-turnSpeed * dt,
    Math.min(turnSpeed * dt, angleDelta(t.aim, desired)));
  const weapon = equippedWeapon(t);
  const b = t.brain,
    p = t.body.translation();
  b.decision -= dt;
  b.reaction -= dt;
  b.fireDelay = Math.max(0, b.fireDelay - dt);
  b.memory -= dt;
  if (b.decision <= 0) {
    b.decision = s.rng.range(0.22, 0.42);
    const threats: Tank[] = [];
    // Rapier broad phase gathers local actors; team and perception rules are controller-level filters.
    s.world.intersectionsWithShape(
      p,
      { x: 0, y: 0, z: 0, w: 1 },
      new RAPIER.Ball(profile.sight),
      (c) => {
        const e = s.tanks.find(
          (a) => a.alive && a.team !== t.team && a.collider.handle === c.handle,
        );
        if (e && (aggressive || s.visible(p, e.body.translation()))) threats.push(e);
        return true;
      },
    );
    threats.sort(
      (a, c) =>
        distance(p, a.body.translation()) - distance(p, c.body.translation()),
    );
    const target = threats[0];
    if (target) {
      if (target.id !== b.target) b.reaction = easy ? s.rng.range(1, 1.6) : aggressive ? s.rng.range(0.3, 0.5) : s.rng.range(0.4, 0.8);
      b.target = target.id;
      b.memory = aggressive ? 3 : 1.5;
      b.goal = {
        x: target.body.translation().x,
        z: target.body.translation().z,
      };
      b.lastSeen = { ...b.goal };
      b.mode = "fight";
    } else if (b.memory <= 0) {
      b.target = 0;
      b.mode = "advance";
    }
    b.aimError = s.rng.range(-profile.aimError, profile.aimError) + (easy ? s.rng.range(-0.2, 0.2) : 0);
    const useful = s.pickups.filter(
      (q) =>
        q.available &&
        (q.kind !== "repair" || t.hp < s.maxHealth(t) * 0.8) &&
        (q.kind !== "rapid" || t.rapid < 2) &&
        (q.kind !== "ricochet" || t.ricochet < 2) &&
        (q.kind !== "speed" || t.speed < 2) &&
        (q.kind !== "shield" || t.shield < 2 || t.shieldPoints < 40),
    );
    useful.sort((a, c) => distance(p, a) - distance(p, c));
    const hurt = t.hp < s.maxHealth(t) * 0.4;
    const repair = useful.find((q) => q.kind === "repair");
    if (hurt && repair) {
      b.goal = { ...repair };
      b.mode = "retreat";
    } else if (
      useful[0] &&
      distance(p, useful[0]) < (profile.stationary && target ? 5 : aggressive ? 7 : 12) &&
      (!target || (t.spread === 0 && t.rocket === 0))
    ) {
      b.goal = { x: useful[0].x, z: useful[0].z };
      b.mode = "pickup";
    } else if (!b.target && b.memory <= 0) {
      // Distributed flank waypoints are symmetric and independent of the human.
      b.goal = {
        x: t.team === 0 ? 20 : -20,
        z: [-38, 0, 38][role % 3] * (t.team === 0 ? 1 : -1),
      };
      if (distance(p, b.goal) < 4)
        b.goal = { x: t.team === 0 ? 46 : -46, z: s.rng.range(-44, 44) };
    }
    if (!easy && !target && b.mode === "advance" && b.personality === "support") {
      const allies = s.tanks.filter((a) => a.alive && a.team === t.team && a !== t
        && a.brain.personality !== "support");
      allies.sort((a, c) => distance(p, a.body.translation()) - distance(p, c.body.translation()));
      if (allies[0]) {
        const ally = allies[0].body.translation();
        b.goal = { x: ally.x + (t.team === 0 ? -4 : 4), z: ally.z };
        b.mode = "escort";
      }
    }
    if (easy && !target && b.mode === "advance" && s.human.alive) {
      const human = s.human.body.translation();
      b.goal = { x: human.x, z: human.z };
    }
    if (
      b.navVersion !== s.nav.version ||
      !b.path.length ||
      distance(b.path[b.path.length - 1], b.goal) > 4
    ) {
      b.path = s.nav.find(p, b.goal);
      b.navVersion = s.nav.version;
      s.botReroutes++;
    }
    if (distance(p, b.last) < 0.6 && Math.hypot(t.command.moveX, t.command.moveZ) > 0.1) {
      b.stuck += b.decision;
      if (b.stuck > 1.1) {
        const side = role % 2 ? 1 : -1;
        b.path = s.nav.find(p, { x: p.x + side * 5, z: p.z + 5 });
        b.stuck = 0;
        s.botReroutes++;
      }
    } else b.stuck = 0;
    b.last = { x: p.x, z: p.z };
  }
  const c = idleCommand();
  let waypoint: Vec2 = b.path[0] ?? b.goal;
  while (b.path.length && distance(p, b.path[0]) < 1.25) b.path.shift();
  waypoint = b.path[0] ?? b.goal;
  let mx = waypoint.x - p.x,
    mz = waypoint.z - p.z;
  const target = s.tanks.find((e) => e.id === b.target && e.alive);
  if (target && b.memory > 0) {
    const actual = target.body.translation();
    const seen = s.visible(p, actual);
    if (seen || aggressive) b.lastSeen = { x: actual.x, z: actual.z };
    const q = seen || aggressive ? actual : b.lastSeen;
    const v = seen ? target.body.linvel() : { x: 0, z: 0 };
    const d = distance(p, q);
    const desired = Math.atan2(
      q.x + v.x * d * (easy ? 0.1 : 0.65) / WEAPONS[weapon].speed - p.x,
      q.z + v.z * d * (easy ? 0.1 : 0.65) / WEAPONS[weapon].speed - p.z,
    ) + b.aimError;
    c.aim = turn(desired);
    c.fire = seen && d <= profile.sight && b.reaction <= 0
      && Math.abs(angleDelta(c.aim, desired)) < (profile.stationary ? 0.13 : 0.2);
    if (b.mode === "fight" && seen) {
      const movement = combatMovement(t, q.x - p.x, q.z - p.z, role % 2 ? 1 : -1);
      mx = movement.x; mz = movement.z;
    }
    c.mine = !easy && b.personality === "minelayer" && d < 17 && t.mineCooldown <= 0;
  } else {
    c.aim = turn(Math.atan2(mx, mz));
  }
  // Deliberately clear nearby weak timber and towers that obstruct a useful route.
  if (!c.fire) {
    const weak = s.covers.find(
      (o) =>
        o.alive &&
        o.destructible &&
        o.kind !== "drum" &&
        distance(p, o) < 14 &&
        Math.abs(
          angleDelta(
            Math.atan2(b.goal.x - p.x, b.goal.z - p.z),
            Math.atan2(o.x - p.x, o.z - p.z),
          ),
        ) < 0.5,
    );
    if (weak) {
      const desired = Math.atan2(weak.x - p.x, weak.z - p.z);
      c.aim = turn(desired);
      c.fire = Math.abs(angleDelta(c.aim, desired)) < 0.15;
      if (c.fire && t.cooldown === 0 && b.fireDelay === 0) s.botBreachShots++;
    }
  }
  // Personality cadence also applies when breaching; human weapon cadence is separate.
  if (b.fireDelay > 0) c.fire = false;
  else if (c.fire && t.cooldown === 0)
    b.fireDelay = easy ? s.rng.range(2, 3) : botReload(t, s.rng.range(0.1, 0.25));
  const mag = Math.hypot(mx, mz) || 1;
  mx /= mag;
  mz /= mag;
  // Retreats and strafes must not drive blindly into the edge of a firing lane.
  if (s.nav.blocked[s.nav.index({ x: p.x + mx * 3, z: p.z + mz * 3 })]) {
    const pathX = waypoint.x - p.x, pathZ = waypoint.z - p.z;
    const length = Math.hypot(pathX, pathZ) || 1;
    mx = pathX / length; mz = pathZ / length;
  }
  // Nearby allies and enemies both participate in congestion avoidance.
  const near: Tank[] = [];
  s.world.intersectionsWithShape(
    p,
    { x: 0, y: 0, z: 0, w: 1 },
    new RAPIER.Ball(3),
    (collider) => {
      const other = s.tanks.find(
        (a) => a.alive && a !== t && a.collider.handle === collider.handle,
      );
      if (other) near.push(other);
      return true;
    },
  );
  for (const other of near) {
    const q = other.body.translation(),
      d = distance(p, q);
    if (d < 2.8 && d > 0.01) {
      mx += ((p.x - q.x) / d) * (2.8 - d) * 1.1;
      mz += ((p.z - q.z) / d) * (2.8 - d) * 1.1;
    }
  }
  const movementScale = easy ? 0.65 : aggressive ? Math.min(1, profile.speed * 1.35 + 0.15) : profile.speed;
  const length = Math.max(1, Math.hypot(mx, mz));
  c.moveX = mx / length * movementScale;
  c.moveZ = mz / length * movementScale;
  return c;
}
