import { TEAM_COLORS } from "./data";
import type { Team } from "./types";

/** V-Tanks' fill thresholds, shared by the overhead bar and player HUD. */
export function healthBarState(hp: number, maximum: number, team: Team) {
  const ratio = Math.max(0, Math.min(1, hp / Math.max(1, maximum)));
  return { ratio, color: ratio <= 0.3 ? 0xff7c73 : ratio <= 0.6 ? 0xffe27a : TEAM_COLORS[team] };
}
