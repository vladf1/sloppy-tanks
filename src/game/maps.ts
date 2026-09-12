import { arenaLayout } from "./arena";
import { harborLayout } from "./harbor-layout";

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
] as const;
