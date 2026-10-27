// The server directory's rules (worker.ts), free of Cloudflare imports so node tests run them.
import { readRooms, type RoomListing } from "../src/net/room-listing";

/** Each environment lists its own game servers: production's and the dev site's. */
export const ENVIRONMENTS = ["production", "dev"] as const;
export type Environment = (typeof ENVIRONMENTS)[number];

/** Game servers announce at least every 15 s, so an entry outlives two missed announcements. */
export const ENTRY_TTL_MS = 45_000;
/** Far above a busy server's listing; bounds what one caller can make the directory store. */
export const MAX_BODY_BYTES = 256 * 1024;

/** What a game server announces: its `/health` fields and its rooms, checked as the page
 * checks them (src/net/room-listing.ts) so one bad announcement cannot spoil the list. */
export interface Announcement {
  version?: string;
  protocol: number;
  contentVersion: string;
  serverBuild: string;
  commit?: string;
  dirty?: boolean;
  builtAt?: string;
  rooms: RoomListing[];
}
export interface ServerEntry extends Announcement {
  seen: number;
}
export interface ListedServer extends ServerEntry {
  url: string;
}

export function parseEnvironment(value: string | null): Environment | undefined {
  return ENVIRONMENTS.find((environment) => environment === value);
}

/**
 * Where players reach a server announcing from `ip`: nip.io resolves the dashed name to the
 * address itself (the hostname every game server serves, deploy/server/). The directory
 * takes no address from the body, so a caller can list only the machine it announces from.
 */
export function nipUrl(ip: string | null): string | undefined {
  const octets = ip?.split(".") ?? [];
  const ipv4 =
    octets.length === 4 &&
    octets.every((octet) => /^(0|[1-9]\d{0,2})$/.test(octet) && Number(octet) <= 255);
  return ipv4 ? `wss://${octets.join("-")}.nip.io` : undefined;
}

/** A short text field, or undefined when `value` is not one. */
function text(value: unknown): string | undefined {
  return typeof value === "string" && value.length >= 1 && value.length <= 128 ? value : undefined;
}

/** The announcement in `body`, keeping only its declared fields, or undefined when any is
 * missing or malformed. */
export function parseAnnouncement(body: string): Announcement | undefined {
  try {
    const value = JSON.parse(body) as Record<string, unknown>;
    const optional = (name: string) =>
      value[name] === undefined ? undefined : (text(value[name]) ?? fail(name));
    if (!Number.isSafeInteger(value.protocol)) fail("protocol");
    if (value.dirty !== undefined && typeof value.dirty !== "boolean") fail("dirty");
    return {
      version: optional("version"),
      protocol: value.protocol as number,
      contentVersion: text(value.contentVersion) ?? fail("contentVersion"),
      serverBuild: text(value.serverBuild) ?? fail("serverBuild"),
      commit: optional("commit"),
      dirty: value.dirty as boolean | undefined,
      builtAt: optional("builtAt"),
      rooms: readRooms(value.rooms),
    };
  } catch {
    // JSON.parse, a null body and every check throw alike.
    return undefined;
  }
}

function fail(field: string): never {
  throw new Error(`Invalid announcement: ${field}`);
}

/** Keys of the entries no server has announced within ENTRY_TTL_MS. */
export function expiredKeys(entries: Map<string, ServerEntry>, now: number): string[] {
  return [...entries].filter(([, entry]) => now - entry.seen > ENTRY_TTL_MS).map(([key]) => key);
}

/**
 * The listing the page reads, fewest rooms first: the page creates rooms on the first compatible
 * server, so this order is the whole of the matchmaking. Ties go by url to keep the order stable,
 * and the stored key's url follows the spread so an announced `url` field cannot replace it.
 */
export function listServers(entries: Map<string, ServerEntry>): ListedServer[] {
  return [...entries]
    .map(([url, entry]) => ({ ...entry, url }))
    .sort(
      (a, b) => a.rooms.length - b.rooms.length || (a.url < b.url ? -1 : a.url > b.url ? 1 : 0),
    );
}
