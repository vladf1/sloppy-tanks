import type { Team, VehicleKind } from "./types";

const columns: Record<VehicleKind, number> = { scout: 0, balanced: 1, heavy: 2 };

/** A clipped tile from the checked-in sheet produced by generate:previews. */
export function tankPreview(kind: VehicleKind, team: Team, label: string): string {
  return `<svg class="tank-preview" viewBox="0 0 640 400" role="img" aria-label="${label}" focusable="false">
    <svg width="640" height="400" overflow="hidden">
      <image href="${import.meta.env.BASE_URL}previews/tanks.webp" x="${-columns[kind] * 640}" y="${-team * 400}" width="1920" height="800" />
    </svg>
  </svg>`;
}
