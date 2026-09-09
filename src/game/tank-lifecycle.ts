import RAPIER from "@dimforge/rapier3d-compat";
import { clearAmmo, emptyAmmo } from "./ammunition";
import { spawnPositions } from "./arena";
import { BOT_PROFILES, botAssignment } from "./bot-personalities";
import { GROUP, STEP, VEHICLES, bestBy } from "./data";
import { tankContactCollider } from "./hitboxes";
import type { Simulation } from "./simulation";
import { SIMULATION_RULES, SOLO } from "./simulation-rules";
import type { Tank, Team, Vec2, VehicleKind } from "./types";
import { idleCommand } from "./types";

// The body and contact hull are recreated for each life; identity and score survive respawn.
export function createTankBody(world: RAPIER.World, kind: VehicleKind, position: Vec2) {
  const stats = VEHICLES[kind];
  const body = world.createRigidBody(
    RAPIER.RigidBodyDesc.dynamic()
      .setTranslation(position.x, SIMULATION_RULES.tankBodyHeight, position.z)
      .enabledRotations(false, true, false)
      .setLinearDamping(SIMULATION_RULES.tankLinearDamping)
      .setAngularDamping(SIMULATION_RULES.tankAngularDamping)
      .setCcdEnabled(true)
      .setSoftCcdPrediction(stats.speed * 1.5 * STEP * 2),
  );
  const collider = world.createCollider(
    tankContactCollider(kind)
      .setMass(stats.mass)
      .setCollisionGroups(GROUP.tank)
      .setFriction(0.05)
      .setRestitution(0.1),
    body,
  );
  world.createCollider(tankContactCollider(kind), body);
  return { body, collider };
}
export function spawnTank(
  simulation: Simulation,
  botNames: readonly string[],
  team: Team,
  human: boolean,
  kind: VehicleKind,
  slot = 0,
) {
  const position =
    simulation.gameMode === "solo" && !human
      ? {
          x: team === 0 ? -SOLO.spawnX : SOLO.spawnX,
          z:
            -SOLO.spawnHalfSpanZ +
            ((slot % simulation.activeEnemyLimit) * (SOLO.spawnHalfSpanZ * 2)) /
              (simulation.activeEnemyLimit - 1),
        }
      : spawnPositions(team)[slot % 5];
  const offset = simulation.gameMode === "solo" ? 0 : Math.floor(slot / 5) * 3;
  const ordinal = simulation.tanks.filter((tank) => !tank.human).length;
  const assignment = botAssignment(slot, team, ordinal);
  if (!human) {
    kind = BOT_PROFILES[assignment.personality].chassis;
  }
  const desc = VEHICLES[kind];
  const { body, collider } = createTankBody(simulation.world, kind, {
    x: position.x + (team === 0 ? offset : -offset),
    z: position.z,
  });
  const tank: Tank = {
    id: simulation.nextId++,
    name: human
      ? "YOU"
      : botNames[ordinal % botNames.length] +
        (ordinal >= botNames.length ? ` ${Math.floor(ordinal / botNames.length) + 1}` : ""),
    team,
    human,
    kind,
    body,
    collider,
    hp: desc.health,
    alive: true,
    respawn: 0,
    protection: SIMULATION_RULES.spawnProtectionSeconds,
    selectedAmmo: "standard",
    ammo: emptyAmmo(),
    shield: 0,
    shieldPoints: 0,
    rapid: 0,
    speed: 0,
    laser: 0,
    cooldown: 0,
    mineCooldown: 0,
    aim: team === 0 ? Math.PI / 2 : -Math.PI / 2,
    heading: 0,
    previous: { ...position },
    recoil: 0,
    kills: 0,
    deaths: 0,
    xp: 0,
    lastCombat: 0,
    command: idleCommand(),
    brain: {
      ...assignment,
      lastSeen: { ...position },
      decision: slot * 0.05,
      target: 0,
      memory: 0,
      reaction: 0.3,
      fireDelay: 0,
      aimError: 0,
      path: [],
      goal: { x: 0, z: 0 },
      last: { ...position },
      stuck: 0,
      recovery: 0,
      recoveryGoal: { ...position },
      recoveries: 0,
      avoidance: { x: 0, z: 0 },
      avoidanceTime: 0,
      pickupTarget: 0,
      navVersion: 0,
      mode: "advance",
    },
  };
  tank.hp = simulation.maxHealth(tank);
  if (simulation.isEasyEnemy(tank)) {
    tank.brain.ultraAggressive = false;
  }
  simulation.tanks.push(tank);
  return tank;
}
export function respawnTank(simulation: Simulation, tank: Tank, position?: Vec2): void {
  const kind = tank.human ? simulation.humanKind : tank.kind;
  tank.kind = kind;
  const enemies = simulation.tanks.filter((enemy) => enemy.alive && enemy.team !== tank.team);
  const friends = simulation.tanks.filter(
    (enemy) => enemy.alive && enemy.team === tank.team && enemy !== tank,
  );
  const p =
    position ??
    bestBy(spawnPositions(tank.team), (position) =>
      simulation.spawnScore(position, enemies, friends),
    )!;
  Object.assign(tank, createTankBody(simulation.world, kind, p));
  tank.xp = 0;
  tank.lastCombat = simulation.elapsed;
  tank.hp = simulation.maxHealth(tank);
  tank.alive = true;
  tank.protection = SIMULATION_RULES.spawnProtectionSeconds;
  clearAmmo(tank);
  tank.shield = 0;
  tank.shieldPoints = 0;
  tank.rapid = 0;
  tank.speed = 0;
  tank.laser = 0;
  tank.cooldown = 0;
  tank.mineCooldown = 0;
  tank.previous = { ...p };
  tank.brain.path = [];
  tank.brain.decision = 0;
  tank.brain.fireDelay = 0;
  tank.brain.target = 0;
  tank.brain.memory = 0;
  tank.brain.reaction = 0.3;
  tank.brain.lastSeen = { ...p };
  tank.brain.last = { ...p };
  tank.brain.stuck = tank.brain.recovery = tank.brain.avoidanceTime = 0;
  tank.brain.recoveries = tank.brain.pickupTarget = 0;
  tank.brain.avoidance = { x: 0, z: 0 };
  tank.brain.recoveryGoal = { ...p };
  simulation.events.push({ type: "respawn", ...p, id: tank.id });
}
