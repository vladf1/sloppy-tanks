import RAPIER from "@dimforge/rapier3d-compat";
import { distance, VEHICLES, WEAPONS, angleDelta } from "./data";
import type { Simulation } from "./simulation";
import { idleCommand, type Tank, type Vec2 } from "./types";
export function botCommand(s: Simulation, t: Tank, dt: number) {
  const role = Math.floor(s.tanks.indexOf(t) / 2);
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
      new RAPIER.Ball(28),
      (c) => {
        const e = s.tanks.find(
          (a) => a.alive && a.team !== t.team && a.collider.handle === c.handle,
        );
        if (e && s.visible(p, e.body.translation())) threats.push(e);
        return true;
      },
    );
    threats.sort(
      (a, c) =>
        distance(p, a.body.translation()) - distance(p, c.body.translation()),
    );
    const target = threats[0];
    if (target) {
      if (target.id !== b.target) b.reaction = s.rng.range(0.4, 0.8);
      b.target = target.id;
      b.memory = 1.5;
      b.goal = {
        x: target.body.translation().x,
        z: target.body.translation().z,
      };
      b.mode = "fight";
    } else if (b.memory <= 0) {
      b.target = 0;
      b.mode = "advance";
    }
    b.aimError = s.rng.range(-0.21, 0.21) * (b.preference === "rusher" ? 1.3 : 1);
    const useful = s.pickups.filter(
      (q) =>
        q.available &&
        (q.kind !== "repair" || t.hp < VEHICLES[t.kind].health * 0.8),
    );
    useful.sort((a, c) => distance(p, a) - distance(p, c));
    const hurt = t.hp < VEHICLES[t.kind].health * 0.4;
    const repair = useful.find((q) => q.kind === "repair");
    if (hurt && repair) {
      b.goal = { ...repair };
      b.mode = "retreat";
    } else if (
      useful[0] &&
      distance(p, useful[0]) < (b.preference === "hunter" ? 24 : 10) &&
      (!target || t.weapon === "standard")
    ) {
      b.goal = { ...useful[0] };
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
    if (
      b.navVersion !== s.nav.version ||
      !b.path.length ||
      distance(b.path[b.path.length - 1], b.goal) > 4
    ) {
      b.path = s.nav.find(p, b.goal);
      b.navVersion = s.nav.version;
      s.botReroutes++;
    }
    if (distance(p, b.last) < 0.6) {
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
    const q = target.body.translation(),
      v = target.body.linvel(),
      d = distance(p, q);
    const desired =
      Math.atan2(
        q.x + (v.x * d * 0.65) / WEAPONS[t.weapon].speed - p.x,
        q.z + (v.z * d * 0.65) / WEAPONS[t.weapon].speed - p.z,
      ) + b.aimError;
    c.aim = t.aim + angleDelta(t.aim, desired) * Math.min(1, dt * 5);
    const seen = s.visible(p, q);
    c.fire =
      seen && b.reaction <= 0 && Math.abs(angleDelta(c.aim, desired)) < 0.2;
    if (b.mode === "fight" && seen && d < 17) {
      const side = role % 2 ? 1 : -1;
      mx = (q.z - p.z) * 0.5 * side;
      mz = -(q.x - p.x) * 0.5 * side;
      if (d < 7 || (b.preference === "cautious" && d < 12)) {
        mx += p.x - q.x;
        mz += p.z - q.z;
      }
    }
    c.mine = d < 8 && s.rng.next() < dt * 0.6;
  } else
    c.aim = t.aim + angleDelta(t.aim, Math.atan2(mx, mz)) * Math.min(1, dt * 5);
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
      c.aim = t.aim + angleDelta(t.aim, desired) * Math.min(1, dt * 5);
      c.fire = Math.abs(angleDelta(c.aim, desired)) < 0.15;
      if (c.fire && t.cooldown === 0 && b.fireDelay === 0) s.botBreachShots++;
    }
  }
  // Bots take an extra beat between shots, including when breaching cover.
  // This controller delay leaves the human's weapon handling unchanged.
  if (b.fireDelay > 0) c.fire = false;
  else if (c.fire && t.cooldown === 0)
    b.fireDelay = WEAPONS[t.weapon].interval * 1.4 + s.rng.range(0.1, 0.25);
  const mag = Math.hypot(mx, mz) || 1;
  mx /= mag;
  mz /= mag;
  // Short query-based probes steer away from static cover and other live vehicles.
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
  c.moveX = mx;
  c.moveZ = mz;
  return c;
}
