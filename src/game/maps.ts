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
  layout: () => CoverDef[];
}

/** The menu and Surprise me draw from the same authored maps. */
export const MAPS = [
  {
    id: "village",
    name: "Pine Village",
    description: "A quiet little village. Bring the noise.",
    layout: arenaLayout,
  },
  {
    id: "harbor",
    name: "Harbor Havoc",
    description: "Salt air. Hot steel. Dockside mayhem.",
    layout: harborLayout,
  },
  {
    id: "quarry",
    name: "Dusty Dig",
    description: "Open ground. Weathered stone. Dig your own shortcut.",
    layout: quarryLayout,
  },
] as const satisfies readonly ArenaMap[];
