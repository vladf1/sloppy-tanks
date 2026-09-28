import { roomAddress } from "../game/join-screen";
import type { JoinChoice } from "./connection";
import { settingsReader } from "./protocol";
import { boolean, object, optional, record, string } from "./schema";
import { playerKind, team } from "./scene-codec";

const PENDING_JOIN_KEY = "sloppy-pending-join";
const pendingChoiceReader = object<JoinChoice>({
  name: string(24, 1),
  kind: playerKind,
  team: optional(team),
  create: optional(settingsReader),
  existingRoom: optional(boolean),
});

/** A room picked on Battle Setup and the player's choices for it. */
export interface RoomSelection {
  room: string;
  choice: JoinChoice;
}

export { roomAddress };

/** A page that already built a single-player arena reloads into the room instead of
 * running two renderers; the room page joins with the same choices. */
export function joinAfterReload(selection: RoomSelection): void {
  try {
    sessionStorage.setItem(PENDING_JOIN_KEY, JSON.stringify(selection));
  } catch {
    /* Without storage the room page opens Battle Setup with the room selected. */
  }
  location.assign(roomAddress(selection.room));
}

/** Read the choices once, so a later reload of the room page opens Battle Setup. */
export function takePendingJoin(room: string): JoinChoice | undefined {
  try {
    const saved = sessionStorage.getItem(PENDING_JOIN_KEY);
    sessionStorage.removeItem(PENDING_JOIN_KEY);
    const pending = record(JSON.parse(saved ?? "null"));
    return pending.room === room ? pendingChoiceReader.read(pending.choice) : undefined;
  } catch {
    return undefined;
  }
}
