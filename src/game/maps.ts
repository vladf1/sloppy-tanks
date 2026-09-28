import { MAP_OPTIONS, type StandardMapId } from "./map-options";
import { arenaLayout } from "./arena";
import type { CoverDef } from "./arena";
import type { GroundKind } from "./ground-surfaces";
import { harborLayout } from "./harbor-layout";
import { quarryLayout } from "./quarry-layout";

export interface ArenaMap {
  id: string;
  name: string;
  description: string;
  theme?: "village" | "harbor" | "quarry";
  floor?: GroundKind;
  outerFloor?: GroundKind;
  outerFloorExtent?: number;
  /** A compact yard's size relative to the standard arena; see Simulation.mapScale. */
  scale?: number;
  layout: () => CoverDef[];
}

const layouts: Record<StandardMapId, () => CoverDef[]> = {
  village: arenaLayout,
  harbor: harborLayout,
  quarry: quarryLayout,
};

/** The standard maps. Extra levels bring their own map as `customMap`. */
export const MAPS = MAP_OPTIONS.flatMap((map) =>
  "extra" in map ? [] : [{ ...map, layout: layouts[map.id] }],
) satisfies ArenaMap[];
