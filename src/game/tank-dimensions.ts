import { Box3, Vector3 } from "three";
import { VEHICLES } from "./data";
import { tankModel } from "./models";
import type { VehicleKind } from "./types";

// Measure each chassis once using the same geometry and transforms as rendering.
// The hull includes tracks; the independently rotating gun is not a hull target.
// Kept apart from hitboxes.ts so rendering reads muzzles without the physics engine.
const dimensions = Object.fromEntries(
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
    return [kind, { center, size, muzzle, visualMuzzle }];
  }),
) as Record<
  VehicleKind,
  { center: Vector3; size: Vector3; muzzle: Vector3; visualMuzzle: Vector3 }
>;

/** Hull bounds in the tank's local frame. */
export function tankHull(kind: VehicleKind) {
  return dimensions[kind];
}

export function tankMuzzle(kind: VehicleKind) {
  return dimensions[kind].muzzle;
}

/** Render-only launch point; unlike tankMuzzle, this is not used for collision queries. */
export function tankVisualMuzzle(kind: VehicleKind) {
  return dimensions[kind].visualMuzzle;
}
