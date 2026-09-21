import { distance } from "./data";
import type { Simulation } from "./simulation";
import type { Tank, Vec2 } from "./types";

const RANGE = 28;
const MIN_RANGE = 16;
const ARRIVAL = 2;
const REPLAN_SECONDS = 1;
const WITHDRAW_SECONDS = 6;
const RELOAD_PAUSE = 1;
const SEARCH_SECONDS = 5;
const REPOSITION_DISTANCE = 5;
export const HUMVEE_AIM_SECONDS = 0.9;
export const HUMVEE_DEPARTURE_SECONDS = 0.65;

export interface HumveeTactics {
  phase: "attack" | "withdraw" | "hide";
  escape: Vec2;
  firingPoint: Vec2;
  readyAt: number;
  replanAt: number;
  deadline: number;
  lastShot?: Vec2;
  plannedThreat?: Vec2;
  flank: number;
  aimSeconds: number;
  aimTarget?: number;
  departureAt: number;
}

function setGoal(simulation: Simulation, tank: Tank, goal: Vec2): void {
  const brain = tank.brain;
  if (distance(brain.goal, goal) > 1 || brain.navVersion !== simulation.nav.version) {
    brain.goal = goal;
    brain.path = simulation.nav.find(tank.body.translation(), goal);
    brain.navVersion = simulation.nav.version;
    simulation.botReroutes++;
  }
}

/** Prefer nearby concealment; in open terrain, withdraw away from the target. */
function escapePoint(simulation: Simulation, from: Vec2, threat: Vec2): Vec2 | undefined {
  const away = Math.atan2(from.x - threat.x, from.z - threat.z);
  const candidates: { point: Vec2; score: number }[] = [];
  for (const radius of [8, 14]) {
    for (let i = 0; i < 12; i++) {
      const angle = away + (i * Math.PI * 2) / 12;
      const point = simulation.nav.point(
        simulation.nav.index({
          x: from.x + Math.sin(angle) * radius,
          z: from.z + Math.cos(angle) * radius,
        }),
      );
      if (
        simulation.nav.blocked[simulation.nav.index(point)] ||
        distance(point, threat) < MIN_RANGE
      ) {
        continue;
      }
      const hidden = !simulation.visible(point, threat);
      const gain = distance(point, threat) - distance(from, threat);
      if (!hidden && gain < 5) {
        continue;
      }
      candidates.push({ point, score: (hidden ? 40 : 0) + gain - distance(from, point) });
    }
  }
  candidates.sort((a, b) => b.score - a.score);
  for (const { point } of candidates) {
    // A clear escape leg cannot detour toward or through the target.
    if (simulation.nav.clearLine(from, point)) {
      return point;
    }
  }
  return undefined;
}

/** Runs on the existing decision cadence, without consuming additional combat RNG. */
export function updateHumveeGoal(simulation: Simulation, tank: Tank): boolean {
  const brain = tank.brain;
  const position = tank.body.translation();
  let tactics = brain.humvee;
  if (!tactics) {
    tactics = brain.humvee = {
      phase: "attack",
      escape: { x: position.x, z: position.z },
      firingPoint: { x: position.x, z: position.z },
      readyAt: 0,
      replanAt: 0,
      deadline: 0,
      flank: tank.team === 0 ? 1 : -1,
      aimSeconds: 0,
      departureAt: 0,
    };
  }
  const threat = brain.lastSeen;
  if (tactics.phase === "attack" && brain.target && distance(position, threat) < MIN_RANGE) {
    tactics.phase = "withdraw";
    tactics.escape = escapePoint(simulation, position, threat) ?? { x: position.x, z: position.z };
    tactics.readyAt = simulation.elapsed + REPLAN_SECONDS;
    tactics.deadline = simulation.elapsed + WITHDRAW_SECONDS;
  }
  if (tactics.phase !== "attack") {
    brain.mode = "retreat";
    let reached = distance(position, tactics.escape) < ARRIVAL;
    if (reached) {
      tactics.phase = "hide";
    }
    // A moving enemy or destroyed cover can invalidate the original hiding place.
    if (
      simulation.elapsed >= tactics.replanAt &&
      ((reached && simulation.visible(position, threat)) || distance(position, threat) < MIN_RANGE)
    ) {
      const escape = escapePoint(simulation, position, threat);
      if (escape) {
        tactics.escape = escape;
        tactics.phase = "withdraw";
        reached = false;
      }
      tactics.replanAt = simulation.elapsed + REPLAN_SECONDS;
    }
    setGoal(simulation, tank, tactics.escape);
    if (
      simulation.elapsed < tactics.readyAt ||
      tank.cooldown > 0 ||
      brain.fireDelay > 0 ||
      (!reached && simulation.elapsed < tactics.deadline)
    ) {
      return true;
    }
    tactics.phase = "attack";
    tactics.plannedThreat = undefined;
    tactics.replanAt = 0;
  }
  if (
    (!brain.target || brain.memory <= 0) &&
    (!tactics.lastShot || simulation.elapsed > tactics.deadline + SEARCH_SECONDS)
  ) {
    return false;
  }
  brain.mode = "fight";
  if (simulation.elapsed < tactics.replanAt) {
    setGoal(simulation, tank, tactics.firingPoint);
    return true;
  }
  tactics.replanAt = simulation.elapsed + REPLAN_SECONDS;
  if (
    tactics.plannedThreat &&
    distance(tactics.plannedThreat, threat) < 4 &&
    !simulation.nav.blocked[simulation.nav.index(tactics.firingPoint)] &&
    simulation.visible(tactics.firingPoint, threat)
  ) {
    setGoal(simulation, tank, tactics.firingPoint);
    return true;
  }
  const angle = Math.atan2(position.x - threat.x, position.z - threat.z);
  const candidates: Vec2[] = [];
  if (
    !tactics.lastShot &&
    distance(position, threat) >= MIN_RANGE &&
    distance(position, threat) <= 32
  ) {
    candidates.push({ x: position.x, z: position.z });
  }
  for (const offset of [0.3, 0.6, 0.9, -0.3, -0.6, 0]) {
    const heading = angle + offset * tactics.flank;
    candidates.push({
      x: threat.x + Math.sin(heading) * RANGE,
      z: threat.z + Math.cos(heading) * RANGE,
    });
  }
  for (const candidate of candidates) {
    const point = simulation.nav.point(simulation.nav.index(candidate));
    if (
      simulation.nav.blocked[simulation.nav.index(point)] ||
      (tactics.lastShot && distance(point, tactics.lastShot) < REPOSITION_DISTANCE) ||
      !simulation.visible(point, threat)
    ) {
      continue;
    }
    const escape = escapePoint(simulation, point, threat);
    if (!escape) {
      continue;
    }
    const path = simulation.nav.find(position, point);
    if (
      (!path.length && distance(position, point) > ARRIVAL) ||
      path.some((waypoint) => distance(waypoint, threat) < MIN_RANGE - 2)
    ) {
      continue;
    }
    tactics.escape = escape;
    tactics.firingPoint = point;
    tactics.plannedThreat = { ...threat };
    brain.goal = point;
    brain.path = path;
    brain.navVersion = simulation.nav.version;
    simulation.botReroutes++;
    return true;
  }
  // No safe firing position: create distance rather than charging the enemy.
  tactics.escape = escapePoint(simulation, position, threat) ?? { x: position.x, z: position.z };
  tactics.phase = "withdraw";
  tactics.readyAt = simulation.elapsed + REPLAN_SECONDS;
  tactics.deadline = simulation.elapsed + WITHDRAW_SECONDS;
  brain.mode = "retreat";
  setGoal(simulation, tank, tactics.escape);
  return true;
}

/** Commit only after an actual launch, not an attempted or ally-blocked shot. */
export function withdrawHumvee(simulation: Simulation, tank: Tank): void {
  const tactics = tank.brain.humvee;
  if (!tactics) {
    return;
  }
  const position = tank.body.translation();
  tactics.lastShot = { x: position.x, z: position.z };
  tactics.flank *= -1;
  tactics.aimSeconds = 0;
  tactics.departureAt = simulation.elapsed + HUMVEE_DEPARTURE_SECONDS;
  tactics.phase = "withdraw";
  tactics.readyAt =
    simulation.elapsed + Math.max(tank.cooldown, tank.brain.fireDelay) + RELOAD_PAUSE;
  tactics.deadline = simulation.elapsed + WITHDRAW_SECONDS;
  tactics.replanAt = simulation.elapsed + REPLAN_SECONDS;
  tank.brain.mode = "retreat";
  tank.brain.recovery = 0;
  tank.brain.avoidanceTime = 0;
  setGoal(simulation, tank, tactics.escape);
}

export function humveeCanFire(tank: Tank): boolean {
  return (
    tank.brain.humvee?.phase === "attack" &&
    distance(tank.body.translation(), tank.brain.lastSeen) >= MIN_RANGE &&
    distance(tank.body.translation(), tank.brain.goal) < 3
  );
}

/** A visible firing pause gives opponents time to line up a counter-shot. */
export function steadyHumveeShot(tank: Tank, canFire: boolean, dt: number): boolean {
  const tactics = tank.brain.humvee;
  if (!tactics) {
    return false;
  }
  if (tactics.aimTarget !== tank.brain.target) {
    tactics.aimSeconds = 0;
    tactics.aimTarget = tank.brain.target;
  }
  if (!canFire || tank.cooldown > 0 || tank.brain.fireDelay > 0) {
    tactics.aimSeconds = 0;
    return false;
  }
  tactics.aimSeconds += dt;
  return tactics.aimSeconds >= HUMVEE_AIM_SECONDS;
}

export function humveeHoldingPosition(simulation: Simulation, tank: Tank): boolean {
  const tactics = tank.brain.humvee;
  return !!tactics && (tactics.aimSeconds > 0 || simulation.elapsed < tactics.departureAt);
}
