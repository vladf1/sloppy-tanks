import { roomAddress } from "../game/join-screen";
import type { JoinChoice } from "./room-protocol";
import { gameServerAddress } from "./server-address";

const PENDING_JOIN_KEY = "sloppy-pending-join";

/** A room picked on Battle Setup, the server that hosts it, and the player's choices. */
export interface RoomSelection {
  room: string;
  /** The game server's WebSocket address, from the server directory. */
  server: string;
  choice: JoinChoice;
}

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

/** The stored selection, read once so a later reload of the room page opens Battle Setup.
 * The engine validates its choices (`NetGame.pending_join`) before joining. */
export function takePendingJoin(): string | undefined {
  try {
    const saved = sessionStorage.getItem(PENDING_JOIN_KEY) ?? undefined;
    sessionStorage.removeItem(PENDING_JOIN_KEY);
    return saved;
  } catch {
    return undefined;
  }
}

/** The server stored with the pending join for `room`, checked like a directory listing's
 * address. */
export function pendingServer(saved: string | undefined, room: string): string | undefined {
  try {
    const selection = JSON.parse(saved ?? "null") as Partial<RoomSelection> | null;
    return selection?.room === room
      ? gameServerAddress(selection.server, location.hostname)
      : undefined;
  } catch {
    return undefined;
  }
}
