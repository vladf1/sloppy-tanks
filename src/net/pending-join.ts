import { roomAddress } from "../game/join-screen";
import type { JoinChoice } from "./room-protocol";

const PENDING_JOIN_KEY = "sloppy-pending-join";

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

/** The stored choices, read once so a later reload of the room page opens Battle Setup.
 * The engine validates them (`NetGame.pending_join`) before joining. */
export function takePendingJoin(): string | undefined {
  try {
    const saved = sessionStorage.getItem(PENDING_JOIN_KEY) ?? undefined;
    sessionStorage.removeItem(PENDING_JOIN_KEY);
    return saved;
  } catch {
    return undefined;
  }
}
