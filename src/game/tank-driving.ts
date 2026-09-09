const DRIVE_DEADZONE = 0.05;
const REVERSE_ANGLE_EPSILON = 1e-6;
const SPEED_BOOST_MULTIPLIER = 1.5;
import { HULL_TURN_SPEED, MOVE_ACCELERATION, REVERSE_SPEED, VEHICLES } from "./data";
import { angleDelta } from "./math";
import type { Tank, VehicleCommand } from "./types";

/** Apply track steering as bounded impulses; collisions and knockback retain momentum. */
export function driveTank(tank: Tank, command: VehicleCommand, dt: number): void {
  const inputMagnitude = Math.hypot(command.moveX, command.moveZ);
  const speed = VEHICLES[tank.kind].speed * (tank.speed > 0 ? SPEED_BOOST_MULTIPLIER : 1);
  let drive = 0;
  if (inputMagnitude > DRIVE_DEADZONE) {
    const desired = Math.atan2(command.moveX, command.moveZ);
    // Choose the nearer end of the hull; perpendicular input favors forward.
    const reverse =
      Math.abs(angleDelta(tank.heading, desired)) > Math.PI / 2 + REVERSE_ANGLE_EPSILON;
    const target = desired + (reverse ? Math.PI : 0);
    const turn = angleDelta(tank.heading, target);
    tank.heading += Math.max(-HULL_TURN_SPEED * dt, Math.min(HULL_TURN_SPEED * dt, turn));
    // Unequal track speeds make an arc. Sharp turns shed speed toward a pivot.
    const alignment = Math.max(0, Math.cos(angleDelta(tank.heading, target)));
    drive =
      Math.min(1, inputMagnitude) * speed * alignment * alignment * (reverse ? -REVERSE_SPEED : 1);
  }
  const desiredVelocityX = Math.sin(tank.heading) * drive;
  const desiredVelocityZ = Math.cos(tank.heading) * drive;
  const velocity = tank.body.linvel();
  const velocityDeltaX = desiredVelocityX - velocity.x;
  const velocityDeltaZ = desiredVelocityZ - velocity.z;
  const accelerationFraction = Math.min(
    1,
    (MOVE_ACCELERATION * dt) / (Math.hypot(velocityDeltaX, velocityDeltaZ) || 1),
  );
  // Bounded impulses preserve knockback; no per-frame velocity overwrite.
  tank.body.applyImpulse(
    {
      x: velocityDeltaX * accelerationFraction * tank.body.mass(),
      y: 0,
      z: velocityDeltaZ * accelerationFraction * tank.body.mass(),
    },
    true,
  );
  tank.body.setRotation(
    { x: 0, y: Math.sin(tank.heading / 2), z: 0, w: Math.cos(tank.heading / 2) },
    true,
  );
}
