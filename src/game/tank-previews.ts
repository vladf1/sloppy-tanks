import type { Team, VehicleKind } from "./types";

/** Checked-in images rendered from the actual models by generate:previews. */
export function tankPreview(kind: VehicleKind, team: Team): string {
  return `${import.meta.env.BASE_URL}previews/${team}-${kind}.webp`;
}
