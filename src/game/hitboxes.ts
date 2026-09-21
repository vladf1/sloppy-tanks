import RAPIER from "@dimforge/rapier3d-compat";
import { Box3, Vector3 } from "three";
import { GROUP, VEHICLES } from "./data";
import { tankModel } from "./models";
import type { Shot, Tank, VehicleKind } from "./types";

export const SHELL_HIT_RADIUS = 0.18;
// Measure each chassis once using the same geometry and transforms as rendering.
// The hull includes tracks; the independently rotating gun is not a hull target.
const shapes = Object.fromEntries(
  (Object.keys(VEHICLES) as VehicleKind[]).map((kind) => {
    const model = tankModel(kind, 0);
    model.updateMatrixWorld(true);
    const bounds = new Box3().setFromObject(model.userData.hull);
    const size = bounds.getSize(new Vector3());
    const center = bounds.getCenter(new Vector3());
    const visualMuzzle = model.userData.muzzle.getWorldPosition(new Vector3());
    const muzzle = visualMuzzle.clone();
    if (kind === "humvee") {
      // Keep the visual roof launcher high, but put its combat lane inside the
      // planar tank hit volume used by the rest of the simulation.
      muzzle.y = 1.15;
    }
    return [
      kind,
      {
        shape: new RAPIER.Cuboid(
          size.x / 2 + SHELL_HIT_RADIUS,
          0.9, // Combat is planar; visual launcher height does not enlarge the target.
          size.z / 2 + SHELL_HIT_RADIUS,
        ),
        center,
        size,
        muzzle,
        visualMuzzle,
      },
    ];
  }),
) as Record<
  VehicleKind,
  { shape: RAPIER.Cuboid; center: Vector3; size: Vector3; muzzle: Vector3; visualMuzzle: Vector3 }
>;

export function tankMuzzle(kind: VehicleKind) {
  return shapes[kind].muzzle;
}

/** Render-only launch point; unlike tankMuzzle, this is not used for collision queries. */
export function tankVisualMuzzle(kind: VehicleKind) {
  return shapes[kind].visualMuzzle;
}

/** Full visible footprint for tank contact, without the shell-radius allowance. */
export function tankContactCollider(kind: VehicleKind) {
  const { size, center } = shapes[kind];
  return RAPIER.ColliderDesc.cuboid(size.x / 2, 0.6, size.z / 2)
    .setTranslation(center.x, 0, center.z)
    .setCollisionGroups(GROUP.tankContact)
    .setMass(0)
    .setFriction(0.05)
    .setRestitution(0);
}

/** Sweep a shell against the hull, accounting for this tick's tank translation. */
export function tankHitTime(
  shot: Pick<Shot, "x" | "y" | "z" | "vx" | "vz" | "owner">,
  tank: Tank,
  limit: number,
  elapsed = 0,
  frameDelta = 0,
): number | null {
  if (!tank.alive || tank.id === shot.owner) {
    return null;
  }
  const end = tank.body.translation();
  const vx = frameDelta > 0 ? (end.x - tank.previous.x) / frameDelta : 0;
  const vz = frameDelta > 0 ? (end.z - tank.previous.z) / frameDelta : 0;
  const { shape, center } = shapes[tank.kind];
  const rotation = tank.body.rotation();
  // Live tanks rotate only around the vertical axis.
  const cos = 1 - 2 * rotation.y * rotation.y;
  const sin = 2 * rotation.w * rotation.y;
  const position = {
    x: end.x - vx * (frameDelta - elapsed) + center.x * cos + center.z * sin,
    y: end.y,
    z: end.z - vz * (frameDelta - elapsed) - center.x * sin + center.z * cos,
  };
  const time = shape.castRay(
    new RAPIER.Ray(
      { x: shot.x, y: shot.y ?? 1, z: shot.z },
      { x: shot.vx - vx, y: 0, z: shot.vz - vz },
    ),
    position,
    rotation,
    limit,
    true,
  );
  return time >= 0 && time <= limit ? time : null;
}
