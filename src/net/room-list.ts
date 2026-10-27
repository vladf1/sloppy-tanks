import { gameServerAddress } from "./server-address";
import { readRooms, type RoomListing } from "./room-listing";
import type { RoomPhase } from "./room-protocol";

export { MAX_LISTED_ROOMS, type RoomListing } from "./room-listing";
export const MAX_LISTED_SERVERS = 16;

function fail(field: string): never {
  throw new Error(`Invalid server listing: ${field}`);
}

/** A game server as the directory lists it: its WebSocket address and the content version
 * it plays. */
export interface ServerListing {
  url: string;
  contentVersion: string;
}

/** An open room and the address of the server that hosts it. */
export interface HostedRoom extends RoomListing {
  server: string;
}

export interface ServerList {
  /** Fewest rooms first, the order a new room prefers. */
  servers: ServerListing[];
  /** Every server's rooms, the fullest first. */
  rooms: HostedRoom[];
}

/** The directory's `{ servers: [{ url, contentVersion, rooms, ... }] }`, at most
 * MAX_LISTED_SERVERS. Server addresses follow `gameServerAddress` for a page on
 * `hostname`. Throws on anything malformed. */
export function readServerList(value: unknown, hostname: string): ServerList {
  const entries = (value as { servers?: unknown } | null)?.servers;
  if (!Array.isArray(entries) || entries.length > MAX_LISTED_SERVERS) {
    fail("servers");
  }
  const servers: ServerListing[] = [];
  const rooms: HostedRoom[] = [];
  for (const value of entries) {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      fail("server");
    }
    const entry = value as Record<string, unknown>;
    const url = gameServerAddress(entry.url, hostname);
    const contentVersion = entry.contentVersion;
    if (typeof contentVersion !== "string") {
      fail("server contentVersion");
    }
    servers.push({ url, contentVersion });
    rooms.push(...readRooms(entry.rooms).map((room) => ({ ...room, server: url })));
  }
  rooms.sort((a, b) => b.players - a.players || a.room.localeCompare(b.room));
  return { servers, rooms };
}

/** A battle under way first, then a lobby, then a room between rounds. */
const PHASE_ORDER: Record<RoomPhase, number> = { playing: 0, lobby: 1, results: 2 };

/** The room a phone, which shows no room list, joins: of the rooms `open` says can take
 * a player, one whose battle is under way, then the one with the most players. */
export function busiestOpenRoom(
  rooms: readonly RoomListing[],
  open: (room: RoomListing) => boolean,
): RoomListing | undefined {
  return rooms
    .filter(open)
    .sort((a, b) => PHASE_ORDER[a.phase] - PHASE_ORDER[b.phase] || b.players - a.players)[0];
}
