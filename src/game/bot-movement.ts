import { angleDelta, distance, GROUP } from "./data";
import type { Simulation } from "./simulation";
import type { Tank, Vec2 } from "./types";

/** Follow a few visible waypoints ahead, then brake as the destination approaches. */
export function routeDirection(s: Simulation, t: Tank): Vec2 {
  const b = t.brain, p = t.body.translation();
  const goal = b.recovery > 0 ? b.recoveryGoal : b.goal;
  while (b.path.length && distance(p, b.path[0]) < 0.55) b.path.shift();
  let skip = 0;
  for (let i = 1; i < Math.min(8, b.path.length); i++) {
    if (!s.nav.clearLine(p, b.path[i])) break;
    skip = i;
  }
  if (skip) b.path.splice(0, skip);
  // An unreachable goal is not permission to drive straight through its obstruction.
  const waypoint = b.path[0] ?? (s.nav.clearLine(p, goal) ? goal : p);
  const dx = waypoint.x - p.x, dz = waypoint.z - p.z, d = Math.hypot(dx, dz);
  if (d < 0.25) return { x: 0, z: 0 };
  const speed = b.path.length > 1 ? 1 : Math.min(1, d / 1.5);
  return { x: dx / d * speed, z: dz / d * speed };
}

/** Check the visible hull, including other tanks, before choosing a steering direction. */
function clearance(s: Simulation, t: Tank, direction: Vec2, length: number) {
  const p = t.collider.translation(), angle = Math.atan2(direction.x, direction.z);
  let clear = length;
  // Check both the current hull and the orientation it is turning toward.
  for (const yaw of [t.heading, t.heading + angleDelta(t.heading, angle)]) {
    const hit = s.world.castShape(p, { x: 0, y: Math.sin(yaw / 2), z: 0, w: Math.cos(yaw / 2) },
      { x: direction.x, y: 0, z: direction.z }, t.collider.shape, 0.08, clear, false,
      undefined, GROUP.steeringQuery, undefined, t.body);
    if (hit) clear = Math.min(clear, hit.time_of_impact);
  }
  return clear;
}

/** Commit briefly to an open side instead of alternating retreat and attack every tick. */
export function steerBot(s: Simulation, t: Tank, desired: Vec2, dt: number): Vec2 {
  const b = t.brain, magnitude = Math.hypot(desired.x, desired.z);
  b.avoidanceTime = Math.max(0, b.avoidanceTime - dt);
  if (magnitude < 0.01) { b.avoidanceTime = 0; return { x: 0, z: 0 }; }
  const direction = { x: desired.x / magnitude, z: desired.z / magnitude };
  const lookahead = 1.5;
  if (b.avoidanceTime > 0 && clearance(s, t, b.avoidance, lookahead) >= lookahead * 0.9)
    return { x: b.avoidance.x * magnitude, z: b.avoidance.z * magnitude };
  if (clearance(s, t, direction, lookahead) >= lookahead * 0.9) return desired;
  let best = { x: 0, z: 0 }, bestScore = -Infinity, bestClearance = 0;
  // Keep right when meeting another tank; the same local rule separates both vehicles.
  for (const angle of [Math.PI / 4, Math.PI / 2, -Math.PI / 4, -Math.PI / 2, Math.PI * 0.75, -Math.PI * 0.75, Math.PI]) {
    const cos = Math.cos(angle), sin = Math.sin(angle);
    const candidate = { x: direction.x * cos + direction.z * sin,
      z: direction.z * cos - direction.x * sin };
    const open = clearance(s, t, candidate, lookahead);
    const continuity = candidate.x * b.avoidance.x + candidate.z * b.avoidance.z;
    const score = Math.min(1, open / lookahead) * 4 + cos + continuity * 0.35;
    if (open > 0.2 && score > bestScore) { best = candidate; bestScore = score; bestClearance = open; }
  }
  b.avoidance = best;
  b.avoidanceTime = 0.55;
  const speed = magnitude * Math.min(1, bestClearance / lookahead);
  return { x: best.x * speed, z: best.z * speed };
}

/** Sustained lack of progress triggers a committed detour, independent of decision timing. */
export function recoverBot(s: Simulation, t: Tank, desired: Vec2, dt: number) {
  const b = t.brain, p = t.body.translation();
  b.recovery = Math.max(0, b.recovery - dt);
  if (distance(p, b.last) > 0.8 || Math.hypot(desired.x, desired.z) < 0.1) {
    b.last = { x: p.x, z: p.z }; b.stuck = 0;
  } else b.stuck += dt;
  if (b.recovery > 0 && distance(p, b.recoveryGoal) < 0.8) b.recovery = 0;
  if (b.stuck < 1.2 || b.recovery > 0) return;
  const angle = Math.atan2(desired.x, desired.z), side = b.recoveries % 2 ? -1 : 1;
  for (const offset of [side * Math.PI / 2, -side * Math.PI / 2, Math.PI, side * Math.PI / 4]) {
    const goal = { x: p.x + Math.sin(angle + offset) * 6, z: p.z + Math.cos(angle + offset) * 6 };
    if (s.nav.blocked[s.nav.index(goal)]) continue;
    const path = s.nav.find(p, goal);
    if (!path.length) continue;
    b.path = path; b.recoveryGoal = goal; b.recovery = 1.8;
    b.navVersion = s.nav.version; b.recoveries++; s.botReroutes++;
    b.avoidanceTime = 0; b.stuck = 0; b.last = { x: p.x, z: p.z };
    return;
  }
  b.stuck = 0; b.decision = 0; // Retry the strategic route if there is no local exit.
}
