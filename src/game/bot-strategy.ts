import { enemyDifficulty } from "./difficulty";
import RAPIER from "@dimforge/rapier3d-compat";
import { canCollectAmmo, isSpecialAmmo } from "./ammunition";
import { BOT_AMMO, botProfile } from "./bot-personalities";
import { bestBy, distance, WEAPONS } from "./data";
import type { Simulation } from "./simulation";
import type { Tank, Weapon } from "./types";

// Decision cadence is intentionally slower than steering, which runs every simulation tick.
const DECISION_MIN_SECONDS = 0.22;
const DECISION_MAX_SECONDS = 0.42;
const TARGET_STICKINESS = 0.75;
const REPAIR_SEEK_HEALTH_FRACTION = 0.4;
const REPAIR_COLLECT_HEALTH_FRACTION = 0.8;
const EFFECT_REFRESH_SECONDS = 2;
const PREFERRED_AMMO_DISTANCE_BONUS = 3;
/** Choose a target, retreat/pickup/patrol goal and route; called only on decision ticks. */
export function updateBotGoal(
  simulation: Simulation,
  tank: Tank,
  role: number,
  easy: boolean,
  aggressive: boolean,
  weapon: Weapon,
): void {
  const brain = tank.brain;
  const position = tank.body.translation();
  const profile = botProfile(tank);
  const previousMode = brain.mode;
  brain.decision = simulation.rng.range(DECISION_MIN_SECONDS, DECISION_MAX_SECONDS);
  const threats: Tank[] = [];
  // Rapier broad phase gathers local actors; team and perception rules are controller-level filters.
  simulation.world.intersectionsWithShape(
    position,
    { x: 0, y: 0, z: 0, w: 1 },
    new RAPIER.Ball(profile.sight),
    (c) => {
      const enemy = simulation.tanks.find(
        (candidate) =>
          candidate.alive && candidate.team !== tank.team && candidate.collider.handle === c.handle,
      );
      if (enemy && (aggressive || simulation.visible(position, enemy.body.translation()))) {
        threats.push(enemy);
      }
      return true;
    },
  );
  const target = bestBy(
    threats,
    (candidate) =>
      -distance(position, candidate.body.translation()) *
      (candidate.id === brain.target ? TARGET_STICKINESS : 1),
  );
  if (target) {
    if (target.id !== brain.target) {
      brain.reaction = easy
        ? simulation.rng.range(1, 1.6)
        : aggressive
          ? simulation.rng.range(0.3, 0.5)
          : simulation.rng.range(0.4, 0.8);
      brain.reaction *= enemyDifficulty(simulation, tank).reaction;
    }
    brain.target = target.id;
    brain.memory = aggressive ? 3 : 1.5;
    brain.goal = {
      x: target.body.translation().x,
      z: target.body.translation().z,
    };
    brain.lastSeen = { ...brain.goal };
    brain.mode = "fight";
  } else if (brain.memory <= 0) {
    brain.target = 0;
    brain.mode = "advance";
  }
  brain.aimError =
    simulation.rng.range(-profile.aimError, profile.aimError) +
    (easy ? simulation.rng.range(-0.2, 0.2) : 0);
  brain.aimError *= enemyDifficulty(simulation, tank).aimError;
  const useful = simulation.pickups.filter(
    (pickup) =>
      pickup.available &&
      (pickup.kind !== "repair" ||
        tank.hp < simulation.maxHealth(tank) * REPAIR_COLLECT_HEALTH_FRACTION) &&
      (pickup.kind !== "rapid" || tank.rapid < EFFECT_REFRESH_SECONDS) &&
      (!isSpecialAmmo(pickup.kind) || canCollectAmmo(tank, pickup.kind)) &&
      (pickup.kind !== "speed" || tank.speed < EFFECT_REFRESH_SECONDS) &&
      (pickup.kind !== "laser" || tank.laser < EFFECT_REFRESH_SECONDS) &&
      (pickup.kind !== "shield" ||
        tank.shield < EFFECT_REFRESH_SECONDS ||
        tank.shieldPoints < WEAPONS.standard.damage),
  );
  const nearest =
    useful.find((pickup) => pickup.id === brain.pickupTarget) ??
    bestBy(
      useful,
      (pickup) =>
        -distance(position, pickup) +
        (pickup.kind === BOT_AMMO[brain.personality] ? PREFERRED_AMMO_DISTANCE_BONUS : 0),
    );
  const hurt = tank.hp < simulation.maxHealth(tank) * REPAIR_SEEK_HEALTH_FRACTION;
  const repair = bestBy(useful, (pickup) =>
    pickup.kind === "repair" ? -distance(position, pickup) : -Infinity,
  );
  brain.pickupTarget = 0;
  if (hurt && repair) {
    brain.pickupTarget = repair.id;
    brain.goal = { ...repair };
    brain.mode = "retreat";
  } else if (
    nearest &&
    distance(position, nearest) < (profile.stationary && target ? 5 : aggressive ? 7 : 12) &&
    (!target || weapon === "standard")
  ) {
    brain.pickupTarget = nearest.id;
    brain.goal = { x: nearest.x, z: nearest.z };
    brain.mode = "pickup";
  } else if (!brain.target && brain.memory <= 0) {
    // Keep the chosen patrol destination until arrival instead of flipping
    // between a flank waypoint and a new random destination every decision.
    if (
      previousMode !== "advance" ||
      brain.navVersion === 0 ||
      distance(position, brain.goal) < 2
    ) {
      const flank = {
        x: tank.team === 0 ? 20 : -20,
        z: [-38, 0, 38][role % 3] * (tank.team === 0 ? 1 : -1),
      };
      brain.goal =
        distance(position, flank) < 4
          ? { x: tank.team === 0 ? 46 : -46, z: simulation.rng.range(-44, 44) }
          : flank;
    }
  }
  if (!easy && !target && brain.mode === "advance" && brain.personality === "support") {
    const allies = simulation.tanks.filter(
      (candidate) =>
        candidate.alive &&
        candidate.team === tank.team &&
        candidate !== tank &&
        candidate.brain.personality !== "support",
    );
    const closest = bestBy(
      allies,
      (candidate) => -distance(position, candidate.body.translation()),
    );
    if (closest) {
      const ally = closest.body.translation();
      brain.goal = { x: ally.x + (tank.team === 0 ? -4 : 4), z: ally.z };
      brain.mode = "escort";
    }
  }
  if (easy && !target && brain.mode === "advance" && simulation.human.alive) {
    const human = simulation.human.body.translation();
    brain.goal = { x: human.x, z: human.z };
  }
  // A patrol/escort point can land inside randomized cover. Finish at its
  // navigable neighbor rather than stopping short of an impossible destination.
  if (simulation.nav.blocked[simulation.nav.index(brain.goal)]) {
    brain.goal = simulation.nav.point(simulation.nav.nearest(simulation.nav.index(brain.goal)));
  }
  const routeGoal = brain.recovery > 0 ? brain.recoveryGoal : brain.goal;
  if (
    brain.navVersion !== simulation.nav.version ||
    (!brain.path.length && distance(position, routeGoal) > 0.7) ||
    (brain.recovery <= 0 &&
      brain.path.length &&
      distance(brain.path[brain.path.length - 1], routeGoal) > 4)
  ) {
    brain.path = simulation.nav.find(position, routeGoal);
    brain.navVersion = simulation.nav.version;
    simulation.botReroutes++;
  }
}
