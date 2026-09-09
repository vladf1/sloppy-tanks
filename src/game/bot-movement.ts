import { angleDelta, distance, GROUP } from "./data";
import type { Simulation } from "./simulation";
import type { Tank, Vec2 } from "./types";

// World-space clearance and timing, separate from personality-level combat preferences.
const ROUTE = {
  waypointRadius: 0.55,
  lookaheadWaypoints: 8,
  arrivalRadius: 0.25,
  brakingDistance: 1.5,
} as const;
const STEERING = {
  hullMargin: 0.08,
  deadzone: 0.01,
  lookaheadDistance: 1.5,
  clearFraction: 0.9,
  clearanceWeight: 4,
  continuityWeight: 0.35,
  minimumClearance: 0.2,
  commitmentSeconds: 0.55,
} as const;
const RECOVERY = {
  progressDistance: 0.8,
  movementDeadzone: 0.1,
  stuckSeconds: 1.2,
  detourDistance: 6,
  commitmentSeconds: 1.8,
} as const;

/** Follow a few visible waypoints ahead, then brake as the destination approaches. */
export function routeDirection(simulation: Simulation, tank: Tank): Vec2 {
  const brain = tank.brain;
  const position = tank.body.translation();
  const goal = brain.recovery > 0 ? brain.recoveryGoal : brain.goal;
  while (brain.path.length && distance(position, brain.path[0]) < ROUTE.waypointRadius) {
    brain.path.shift();
  }
  let skip = 0;
  for (let i = 1; i < Math.min(ROUTE.lookaheadWaypoints, brain.path.length); i++) {
    if (!simulation.nav.clearLine(position, brain.path[i])) {
      break;
    }
    skip = i;
  }
  if (skip) {
    brain.path.splice(0, skip);
  }
  // An unreachable goal is not permission to drive straight through its obstruction.
  const waypoint = brain.path[0] ?? (simulation.nav.clearLine(position, goal) ? goal : position);
  const dx = waypoint.x - position.x;
  const dz = waypoint.z - position.z;
  const d = Math.hypot(dx, dz);
  if (d < ROUTE.arrivalRadius) {
    return { x: 0, z: 0 };
  }
  const speed = brain.path.length > 1 ? 1 : Math.min(1, d / ROUTE.brakingDistance);
  return { x: (dx / d) * speed, z: (dz / d) * speed };
}

/** Check the visible hull, including other tanks, before choosing a steering direction. */
function clearance(simulation: Simulation, tank: Tank, direction: Vec2, length: number): number {
  const position = tank.collider.translation();
  const angle = Math.atan2(direction.x, direction.z);
  let clear = length;
  // Check both the current hull and the orientation it is turning toward.
  for (const yaw of [tank.heading, tank.heading + angleDelta(tank.heading, angle)]) {
    const hit = simulation.world.castShape(
      position,
      { x: 0, y: Math.sin(yaw / 2), z: 0, w: Math.cos(yaw / 2) },
      { x: direction.x, y: 0, z: direction.z },
      tank.collider.shape,
      STEERING.hullMargin,
      clear,
      false,
      undefined,
      GROUP.steeringQuery,
      undefined,
      tank.body,
    );
    if (hit) {
      clear = Math.min(clear, hit.time_of_impact);
    }
  }
  return clear;
}

/** Commit briefly to an open side instead of alternating retreat and attack every tick. */
export function steerBot(simulation: Simulation, tank: Tank, desired: Vec2, dt: number): Vec2 {
  const brain = tank.brain;
  const magnitude = Math.hypot(desired.x, desired.z);
  brain.avoidanceTime = Math.max(0, brain.avoidanceTime - dt);
  if (magnitude < STEERING.deadzone) {
    brain.avoidanceTime = 0;
    return { x: 0, z: 0 };
  }
  const direction = { x: desired.x / magnitude, z: desired.z / magnitude };
  const lookahead = STEERING.lookaheadDistance;
  if (
    brain.avoidanceTime > 0 &&
    clearance(simulation, tank, brain.avoidance, lookahead) >= lookahead * STEERING.clearFraction
  ) {
    return { x: brain.avoidance.x * magnitude, z: brain.avoidance.z * magnitude };
  }
  if (clearance(simulation, tank, direction, lookahead) >= lookahead * STEERING.clearFraction) {
    return desired;
  }
  let best = { x: 0, z: 0 };
  let bestScore = -Infinity;
  let bestClearance = 0;
  // Keep right when meeting another tank; the same local rule separates both vehicles.
  for (const angle of [
    Math.PI / 4,
    Math.PI / 2,
    -Math.PI / 4,
    -Math.PI / 2,
    Math.PI * 0.75,
    -Math.PI * 0.75,
    Math.PI,
  ]) {
    const cos = Math.cos(angle);
    const sin = Math.sin(angle);
    const candidate = {
      x: direction.x * cos + direction.z * sin,
      z: direction.z * cos - direction.x * sin,
    };
    const open = clearance(simulation, tank, candidate, lookahead);
    const continuity = candidate.x * brain.avoidance.x + candidate.z * brain.avoidance.z;
    const score =
      Math.min(1, open / lookahead) * STEERING.clearanceWeight +
      cos +
      continuity * STEERING.continuityWeight;
    if (open > STEERING.minimumClearance && score > bestScore) {
      best = candidate;
      bestScore = score;
      bestClearance = open;
    }
  }
  brain.avoidance = best;
  brain.avoidanceTime = STEERING.commitmentSeconds;
  const speed = magnitude * Math.min(1, bestClearance / lookahead);
  return { x: best.x * speed, z: best.z * speed };
}

/** Sustained lack of progress triggers a committed detour, independent of decision timing. */
export function recoverBot(simulation: Simulation, tank: Tank, desired: Vec2, dt: number): void {
  const brain = tank.brain;
  const position = tank.body.translation();
  brain.recovery = Math.max(0, brain.recovery - dt);
  if (
    distance(position, brain.last) > RECOVERY.progressDistance ||
    Math.hypot(desired.x, desired.z) < RECOVERY.movementDeadzone
  ) {
    brain.last = { x: position.x, z: position.z };
    brain.stuck = 0;
  } else {
    brain.stuck += dt;
  }
  if (brain.recovery > 0 && distance(position, brain.recoveryGoal) < RECOVERY.progressDistance) {
    brain.recovery = 0;
  }
  if (brain.stuck < RECOVERY.stuckSeconds || brain.recovery > 0) {
    return;
  }
  const angle = Math.atan2(desired.x, desired.z);
  const side = brain.recoveries % 2 ? -1 : 1;
  for (const offset of [
    (side * Math.PI) / 2,
    (-side * Math.PI) / 2,
    Math.PI,
    (side * Math.PI) / 4,
  ]) {
    const goal = {
      x: position.x + Math.sin(angle + offset) * RECOVERY.detourDistance,
      z: position.z + Math.cos(angle + offset) * RECOVERY.detourDistance,
    };
    if (simulation.nav.blocked[simulation.nav.index(goal)]) {
      continue;
    }
    const path = simulation.nav.find(position, goal);
    if (!path.length) {
      continue;
    }
    brain.path = path;
    brain.recoveryGoal = goal;
    brain.recovery = RECOVERY.commitmentSeconds;
    brain.navVersion = simulation.nav.version;
    brain.recoveries++;
    simulation.botReroutes++;
    brain.avoidanceTime = 0;
    brain.stuck = 0;
    brain.last = { x: position.x, z: position.z };
    return;
  }
  brain.stuck = 0;
  brain.decision = 0; // Retry the strategic route if there is no local exit.
}
