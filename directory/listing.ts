// The server directory's rules (worker.ts), free of Cloudflare imports so node tests run them.
import { readRooms, type RoomListing } from "./room-listing";

/** Each environment lists its own game servers: production's, the dev site's, and the App
 * Store apps', which run the released apps' content version rather than main's. */
export const ENVIRONMENTS = ["production", "dev", "app-store"] as const;
export type Environment = (typeof ENVIRONMENTS)[number];

/** Game servers announce at least every 15 s, so an entry outlives two missed announcements. */
export const ENTRY_TTL_MS = 45_000;
/** Far above a busy server's listing (about 67 KB at MAX_LISTED_ROOMS full rooms, 3 KB at
 * a server's default ten); bounds what one caller can make the directory store. */
export const MAX_BODY_BYTES = 256 * 1024;
/** Bounds on the announced load figures, far above any game server's. */
const MAX_CPUS = 1024;
const MAX_MEMORY_MB = 16 * 1024 * 1024;

/** What a game server announces: its `/health` fields and its rooms (`room-listing.ts`). */
export interface Announcement {
  version?: string;
  protocol: number;
  contentVersion: string;
  serverBuild: string;
  commit?: string;
  dirty?: boolean;
  builtAt?: string;
  /** When the server process started, ISO 8601. */
  startedAt?: string;
  /** The process's CPU time over its latest 10 s sample, in percent of one core. */
  cpuPercent?: number;
  cpus?: number;
  rssMB?: number;
  hostMemoryMB?: number;
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

/**
 * A request's body, or undefined when it is longer than `limit` bytes. A declared
 * Content-Length over the limit is refused unread, and the body is counted as it streams
 * in, so a sender that declares nothing (or too little) costs at most one chunk past the
 * limit before the stream is cancelled.
 */
export async function readBody(
  request: { headers: Headers; body: ReadableStream<Uint8Array> | null },
  limit: number,
): Promise<Uint8Array | undefined> {
  if (Number(request.headers.get("Content-Length")) > limit) return undefined;
  const chunks: Uint8Array[] = [];
  let size = 0;
  if (request.body) {
    const reader = request.body.getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) {
        await reader.cancel();
        return undefined;
      }
      chunks.push(value);
    }
  }
  const body = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return body;
}

/** A short text field, or undefined when `value` is not one. */
function text(value: unknown): string | undefined {
  return typeof value === "string" && value.length >= 1 && value.length <= 128 ? value : undefined;
}

/** A finite number from 0 to `max`, or undefined when `value` is not one. */
function amount(value: unknown, max: number): number | undefined {
  return typeof value === "number" && value >= 0 && value <= max ? value : undefined;
}

/** The announcement in `body`, keeping only its declared fields, or undefined when any is
 * missing or malformed. */
export function parseAnnouncement(body: string): Announcement | undefined {
  try {
    const value = JSON.parse(body) as Record<string, unknown>;
    const optional = (name: string) =>
      value[name] === undefined ? undefined : (text(value[name]) ?? fail(name));
    const optionalAmount = (name: string, max: number) =>
      value[name] === undefined ? undefined : (amount(value[name], max) ?? fail(name));
    if (!Number.isSafeInteger(value.protocol)) fail("protocol");
    if (value.dirty !== undefined && typeof value.dirty !== "boolean") fail("dirty");
    const startedAt = optional("startedAt");
    if (startedAt !== undefined && Number.isNaN(Date.parse(startedAt))) fail("startedAt");
    const cpus = optionalAmount("cpus", MAX_CPUS);
    if (cpus !== undefined && !Number.isInteger(cpus)) fail("cpus");
    return {
      version: optional("version"),
      protocol: value.protocol as number,
      contentVersion: text(value.contentVersion) ?? fail("contentVersion"),
      serverBuild: text(value.serverBuild) ?? fail("serverBuild"),
      commit: optional("commit"),
      dirty: value.dirty as boolean | undefined,
      builtAt: optional("builtAt"),
      startedAt,
      cpuPercent: optionalAmount("cpuPercent", MAX_CPUS * 100),
      cpus,
      rssMB: optionalAmount("rssMB", MAX_MEMORY_MB),
      hostMemoryMB: optionalAmount("hostMemoryMB", MAX_MEMORY_MB),
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

/**
 * The listing the page reads, fewest rooms first: the page creates rooms on the first compatible
 * server, so this order is the whole of the matchmaking. Ties go by url to keep the order stable.
 */
export function listServers(servers: ListedServer[]): ListedServer[] {
  return [...servers].sort(
    (a, b) => a.rooms.length - b.rooms.length || (a.url < b.url ? -1 : a.url > b.url ? 1 : 0),
  );
}
