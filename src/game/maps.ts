import { MAP_OPTIONS } from "./map-options";
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

const layouts = { village: arenaLayout, harbor: harborLayout, quarry: quarryLayout };

export const MAPS = MAP_OPTIONS.map((map) => ({
  ...map,
  layout: layouts[map.id],
})) satisfies ArenaMap[];
