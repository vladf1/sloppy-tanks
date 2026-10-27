// The server directory's rules (worker.ts), free of Cloudflare imports so node tests run them.

/** Each environment lists its own game servers; deploy/servers.json names the same two. */
export const ENVIRONMENTS = ["production", "dev"] as const;
export type Environment = (typeof ENVIRONMENTS)[number];

/** Game servers announce at least every 15 s, so an entry outlives two missed announcements. */
export const ENTRY_TTL_MS = 45_000;
/** Far above a busy server's listing; bounds what one caller can make the directory store. */
export const MAX_BODY_BYTES = 256 * 1024;

/** A game server's `/health` fields and room listings, passed through as it sent them. */
export interface Announcement {
  rooms: unknown[];
  [field: string]: unknown;
}
/** An announcement as stored, with when it arrived (`Date.now()`). */
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
 * address itself (a hostname every game server serves, deploy/servers.json). The directory
 * takes no address from the body, so a caller can list only the machine it announces from.
 */
export function nipUrl(ip: string | null): string | undefined {
  const octets = ip?.split(".") ?? [];
  const ipv4 =
    octets.length === 4 &&
    octets.every((octet) => /^(0|[1-9]\d{0,2})$/.test(octet) && Number(octet) <= 255);
  return ipv4 ? `wss://${octets.join("-")}.nip.io` : undefined;
}

/** The body of an announcement, or undefined unless it is a JSON object with a `rooms` array. */
export function parseAnnouncement(text: string): Announcement | undefined {
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    return undefined;
  }
  if (typeof body !== "object" || body === null || Array.isArray(body)) {
    return undefined;
  }
  return Array.isArray((body as Record<string, unknown>).rooms)
    ? (body as Announcement)
    : undefined;
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
