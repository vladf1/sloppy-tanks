import { botProfile, botReload, combatMovement, preferredAmmo, BOT_AMMO } from "./bot-personalities";
import { isSpecialAmmo, canCollectAmmo } from "./ammunition";
import { routeDirection, steerBot, recoverBot } from "./bot-movement";
import RAPIER from "@dimforge/rapier3d-compat";
import { distance, bestBy, WEAPONS, angleDelta } from "./data";
import type { Simulation } from "./simulation";
import { idleCommand, type Tank } from "./types";
export function botCommand(s: Simulation, t: Tank, dt: number) {
  const role = Math.floor(s.tanks.indexOf(t) / 2);
  const profile = botProfile(t);
  const easy = s.isEasyEnemy(t);
  const aggressive = !easy && t.brain.ultraAggressive;
  const turnSpeed = easy ? 1.5 : aggressive ? Math.max(5.2, profile.turn * 1.4) : profile.turn;
  const turn = (desired: number) => t.aim + Math.max(-turnSpeed * dt,
    Math.min(turnSpeed * dt, angleDelta(t.aim, desired)));
  const weapon = preferredAmmo(t);
  const b = t.brain,
    p = t.body.translation();
  b.decision -= dt;
  b.reaction -= dt;
  b.fireDelay = Math.max(0, b.fireDelay - dt);
  b.memory -= dt;
  if (b.decision <= 0) {
    const previousMode = b.mode;
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
    const target = bestBy(threats, a => -distance(p, a.body.translation()) * (a.id === b.target ? 0.75 : 1));
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
        (!isSpecialAmmo(q.kind) || canCollectAmmo(t, q.kind)) &&
        (q.kind !== "speed" || t.speed < 2) &&
        (q.kind !== "laser" || t.laser < 2) &&
        (q.kind !== "shield" || t.shield < 2 || t.shieldPoints < 40),
    );
    const nearest = useful.find(q => q.id === b.pickupTarget) ?? bestBy(useful, q => -distance(p, q)
      + (q.kind === BOT_AMMO[b.personality] ? 3 : 0));
    const hurt = t.hp < s.maxHealth(t) * 0.4;
    const repair = bestBy(useful, q => q.kind === "repair" ? -distance(p, q) : -Infinity);
    b.pickupTarget = 0;
    if (hurt && repair) {
      b.pickupTarget = repair.id;
      b.goal = { ...repair };
      b.mode = "retreat";
    } else if (
      nearest &&
      distance(p, nearest) < (profile.stationary && target ? 5 : aggressive ? 7 : 12) &&
      (!target || weapon === "standard")
    ) {
      b.pickupTarget = nearest.id;
      b.goal = { x: nearest.x, z: nearest.z };
      b.mode = "pickup";
    } else if (!b.target && b.memory <= 0) {
      // Keep the chosen patrol destination until arrival instead of flipping
      // between a flank waypoint and a new random destination every decision.
      if (previousMode !== "advance" || b.navVersion === 0 || distance(p, b.goal) < 2) {
        const flank = { x: t.team === 0 ? 20 : -20,
          z: [-38, 0, 38][role % 3] * (t.team === 0 ? 1 : -1) };
        b.goal = distance(p, flank) < 4
          ? { x: t.team === 0 ? 46 : -46, z: s.rng.range(-44, 44) } : flank;
      }
    }
    if (!easy && !target && b.mode === "advance" && b.personality === "support") {
      const allies = s.tanks.filter((a) => a.alive && a.team === t.team && a !== t
        && a.brain.personality !== "support");
      const closest = bestBy(allies, a => -distance(p, a.body.translation()));
      if (closest) {
        const ally = closest.body.translation();
        b.goal = { x: ally.x + (t.team === 0 ? -4 : 4), z: ally.z };
        b.mode = "escort";
      }
    }
    if (easy && !target && b.mode === "advance" && s.human.alive) {
      const human = s.human.body.translation();
      b.goal = { x: human.x, z: human.z };
    }
    // A patrol/escort point can land inside randomized cover. Finish at its
    // navigable neighbor rather than stopping short of an impossible destination.
    if (s.nav.blocked[s.nav.index(b.goal)])
      b.goal = s.nav.point(s.nav.nearest(s.nav.index(b.goal)));
    const routeGoal = b.recovery > 0 ? b.recoveryGoal : b.goal;
    if (b.navVersion !== s.nav.version ||
      (!b.path.length && distance(p, routeGoal) > 0.7) ||
      (b.recovery <= 0 && b.path.length && distance(b.path[b.path.length - 1], routeGoal) > 4)) {
      b.path = s.nav.find(p, routeGoal);
      b.navVersion = s.nav.version;
      s.botReroutes++;
    }
  }
  const c = idleCommand();
  c.ammoSelection = "standard";
  let { x: mx, z: mz } = routeDirection(s, t);
  const target = s.tanks.find((e) => e.id === b.target && e.alive);
  if (target && b.memory > 0) {
    const actual = target.body.translation();
    const seen = s.visible(p, actual);
    if (seen) c.ammoSelection = weapon;
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
    if (b.mode === "fight" && seen && b.recovery <= 0) {
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
      c.ammoSelection = "standard";
      const desired = Math.atan2(weak.x - p.x, weak.z - p.z);
      c.aim = turn(desired);
      c.fire = Math.abs(angleDelta(c.aim, desired)) < 0.15;
      if (c.fire && t.cooldown === 0 && b.fireDelay === 0) s.botBreachShots++;
    }
  }
  // Personality cadence also applies when breaching; human weapon cadence is separate.
  if (b.fireDelay > 0) c.fire = false;
  else if (c.fire && t.cooldown === 0)
    b.fireDelay = easy ? s.rng.range(2, 3) : botReload(t, s.rng.range(0.1, 0.25), c.ammoSelection);
  recoverBot(s, t, { x: mx, z: mz }, dt);
  if (b.recovery > 0) ({ x: mx, z: mz } = routeDirection(s, t));
  ({ x: mx, z: mz } = steerBot(s, t, { x: mx, z: mz }, dt));
  const movementScale = easy ? 0.65 : aggressive ? Math.min(1, profile.speed * 1.35 + 0.15) : profile.speed;
  const length = Math.max(1, Math.hypot(mx, mz));
  c.moveX = mx / length * movementScale;
  c.moveZ = mz / length * movementScale;
  return c;
}
