import { recoverBot, routeDirection, steerBot } from "./bot-movement";
import { botProfile, botReload, combatMovement, preferredAmmo } from "./bot-personalities";
import { updateBotGoal } from "./bot-strategy";
import { angleDelta, distance, WEAPONS } from "./data";
import type { Simulation } from "./simulation";
import { idleCommand, type Tank } from "./types";
const BREACH_RANGE = 14;
const BREACH_ROUTE_ANGLE = 0.5;
const BREACH_FIRE_ANGLE = 0.15;

/** Choose goals on decision ticks, then produce the same input command used by human controls. */
export function botCommand(simulation: Simulation, tank: Tank, dt: number) {
  const role = Math.floor(simulation.tanks.indexOf(tank) / 2);
  const profile = botProfile(tank);
  const easy = simulation.isEasyEnemy(tank);
  const aggressive = !easy && tank.brain.ultraAggressive;
  const turnSpeed = easy ? 1.5 : aggressive ? Math.max(5.2, profile.turn * 1.4) : profile.turn;
  const turn = (desired: number) =>
    tank.aim + Math.max(-turnSpeed * dt, Math.min(turnSpeed * dt, angleDelta(tank.aim, desired)));
  const weapon = preferredAmmo(tank);
  const brain = tank.brain;
  const position = tank.body.translation();
  brain.decision -= dt;
  brain.reaction -= dt;
  brain.fireDelay = Math.max(0, brain.fireDelay - dt);
  brain.memory -= dt;
  if (brain.decision <= 0) {
    updateBotGoal(simulation, tank, role, easy, aggressive, weapon);
  }
  const command = idleCommand();
  command.ammoSelection = "standard";
  let { x: mx, z: mz } = routeDirection(simulation, tank);
  const target = simulation.tanks.find((enemy) => enemy.id === brain.target && enemy.alive);
  if (target && brain.memory > 0) {
    const actual = target.body.translation();
    const seen = simulation.visible(position, actual);
    if (seen) {
      command.ammoSelection = weapon;
    }
    if (seen || aggressive) {
      brain.lastSeen = { x: actual.x, z: actual.z };
    }
    const q = seen || aggressive ? actual : brain.lastSeen;
    const velocity = seen ? target.body.linvel() : { x: 0, z: 0 };
    const d = distance(position, q);
    const desired =
      Math.atan2(
        q.x + (velocity.x * d * (easy ? 0.1 : 0.65)) / WEAPONS[weapon].speed - position.x,
        q.z + (velocity.z * d * (easy ? 0.1 : 0.65)) / WEAPONS[weapon].speed - position.z,
      ) + brain.aimError;
    command.aim = turn(desired);
    command.fire =
      seen &&
      d <= profile.sight &&
      brain.reaction <= 0 &&
      Math.abs(angleDelta(command.aim, desired)) < (profile.stationary ? 0.13 : 0.2);
    if (brain.mode === "fight" && seen && brain.recovery <= 0) {
      const movement = combatMovement(tank, q.x - position.x, q.z - position.z, role % 2 ? 1 : -1);
      mx = movement.x;
      mz = movement.z;
    }
    command.mine = !easy && brain.personality === "minelayer" && d < 17 && tank.mineCooldown <= 0;
  } else {
    command.aim = turn(Math.atan2(mx, mz));
  }
  // Deliberately clear nearby weak timber and towers that obstruct a useful route.
  if (!command.fire) {
    const weak = simulation.covers.find(
      (o) =>
        o.alive &&
        o.destructible &&
        o.kind !== "drum" &&
        distance(position, o) < BREACH_RANGE &&
        Math.abs(
          angleDelta(
            Math.atan2(brain.goal.x - position.x, brain.goal.z - position.z),
            Math.atan2(o.x - position.x, o.z - position.z),
          ),
        ) < BREACH_ROUTE_ANGLE,
    );
    if (weak) {
      command.ammoSelection = "standard";
      const desired = Math.atan2(weak.x - position.x, weak.z - position.z);
      command.aim = turn(desired);
      command.fire = Math.abs(angleDelta(command.aim, desired)) < BREACH_FIRE_ANGLE;
      if (command.fire && tank.cooldown === 0 && brain.fireDelay === 0) {
        simulation.botBreachShots++;
      }
    }
  }
  // Personality cadence also applies when breaching; human weapon cadence is separate.
  if (brain.fireDelay > 0) {
    command.fire = false;
  } else if (command.fire && tank.cooldown === 0) {
    brain.fireDelay = easy
      ? simulation.rng.range(2, 3)
      : botReload(tank, simulation.rng.range(0.1, 0.25), command.ammoSelection);
  }
  recoverBot(simulation, tank, { x: mx, z: mz }, dt);
  if (brain.recovery > 0) {
    ({ x: mx, z: mz } = routeDirection(simulation, tank));
  }
  ({ x: mx, z: mz } = steerBot(simulation, tank, { x: mx, z: mz }, dt));
  const movementScale = easy
    ? 0.65
    : aggressive
      ? Math.min(1, profile.speed * 1.35 + 0.15)
      : profile.speed;
  const length = Math.max(1, Math.hypot(mx, mz));
  command.moveX = (mx / length) * movementScale;
  command.moveZ = (mz / length) * movementScale;
  return command;
}
