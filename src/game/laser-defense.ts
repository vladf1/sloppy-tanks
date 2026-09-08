import { LASER_DEFENSE } from "./data";
import type { Simulation } from "./simulation";
import type { Shot, Tank } from "./types";

/** First entry into defense range, in the same time coordinates as other contacts.
 * Relative motion also catches a tank driving into the path of an incoming shell. */
export function laserContactTime(s: Simulation, shot: Shot, tank: Tank,
  limit: number, elapsed: number, frameDelta: number): number | null {
  if (!tank.alive || tank.laser <= 0 || tank.team === shot.team || shot.laserCheckedBy?.includes(tank.id)) return null;
  const end = tank.body.translation();
  const tx = frameDelta > 0 ? (end.x - tank.previous.x) / frameDelta : 0;
  const tz = frameDelta > 0 ? (end.z - tank.previous.z) / frameDelta : 0;
  const x = end.x - tx * (frameDelta - elapsed), z = end.z - tz * (frameDelta - elapsed);
  const dx = shot.x - x, dz = shot.z - z, vx = shot.vx - tx, vz = shot.vz - tz;
  const speed2 = vx * vx + vz * vz, approach = dx * vx + dz * vz;
  if (speed2 < 1e-8 || approach >= 0) return null;
  const distance2 = dx * dx + dz * dz;
  // Ignore shots traveling away or passing safely to the side.
  if (distance2 - approach * approach / speed2 > LASER_DEFENSE.threatRadius ** 2) return null;
  const c = distance2 - LASER_DEFENSE.range ** 2;
  const discriminant = approach * approach - speed2 * c;
  if (discriminant < 0) return null;
  const time = c <= 0 ? 0 : (-approach - Math.sqrt(discriminant)) / speed2;
  if (time < 0 || time > limit) return null;
  return s.visible({ x: x + tx * time, z: z + tz * time },
    { x: shot.x + shot.vx * time, z: shot.z + shot.vz * time }) ? time : null;
}
