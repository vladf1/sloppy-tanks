import {
  AMMO_RESPAWN_SECONDS,
  canCollectAmmo,
  hasAdvancedAmmo,
  isSpecialAmmo,
  refillAmmo,
  selectAmmo,
} from "./ammunition";
import { LASER_DEFENSE, PICKUPS, SHIELD_CAPACITY, WEAPONS } from "./data";
import type { Simulation } from "./simulation";
import type { Pickup, Tank } from "./types";
export function collectPickup(simulation: Simulation, tank: Tank, pickup: Pickup): boolean {
  if (!pickup.available || !tank.alive) {
    return false;
  }
  const kind = pickup.kind;
  if (kind === "repair" && tank.hp >= simulation.maxHealth(tank)) {
    return false;
  }
  if (isSpecialAmmo(kind) && !canCollectAmmo(tank, kind)) {
    return false;
  }
  pickup.available = false;
  pickup.cooldown = kind === "laser" ? LASER_DEFENSE.respawn : AMMO_RESPAWN_SECONDS;
  pickup.cooldownDuration = pickup.cooldown;
  let label = PICKUPS[kind].name;
  if (isSpecialAmmo(kind)) {
    const shouldAutoSelect = tank.human && !hasAdvancedAmmo(tank);
    label = `+${refillAmmo(tank, kind)} ${WEAPONS[kind].unit}`;
    if (shouldAutoSelect) {
      selectAmmo(tank, kind);
    }
  } else if (kind === "repair") {
    tank.hp = simulation.maxHealth(tank);
  } else if (kind === "shield") {
    tank.shield = PICKUPS[kind].duration;
    tank.shieldPoints = SHIELD_CAPACITY;
  } else if (kind === "rapid") {
    tank[kind] = PICKUPS[kind].duration;
  } else if (kind === "speed") {
    tank.speed = PICKUPS[kind].duration;
  } else if (kind === "laser") {
    tank.laser = PICKUPS.laser.duration;
    label = "LASER DEFENSE";
  }
  simulation.events.push({
    type: "pickup",
    x: pickup.x,
    z: pickup.z,
    id: tank.id,
    team: tank.team,
    label,
    color: PICKUPS[kind].color,
  });
  return true;
}
