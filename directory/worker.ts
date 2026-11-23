/**
 * The server directory: game servers announce their room listings here, and the game page
 * reads which servers are up and how busy they are.
 *
 * - `PUT /servers?environment=production|dev` from a game server: its `/health` fields, start
 *   time, join count and load plus a `rooms` array, as one JSON object of at most MAX_BODY_BYTES. The caller's IPv4 address names
 *   the entry, `wss://a-b-c-d.nip.io`, so a server can announce only itself. A machine that
 *   runs several content versions announces each to `/v/<contentVersion>/servers`, listed
 *   at `wss://a-b-c-d.nip.io/v/<contentVersion>`, at most MAX_ENTRIES_PER_MACHINE of them.
 *   Replies 204.
 * - `GET /servers?environment=production|dev[&contentVersion=<hex>|&all=1]` from the page (or
 *   anyone): `{ "servers": [{ "url", ...announced fields, "seen" }] }`, fewest rooms first,
 *   only that content version's when the request names one; every version's, uncapped, with
 *   `all=1` (the overview).
 * - `GET /deployment?environment=production|dev|app-store`: `{ "deployment": { "versions",
 *   "writtenAt" } }`, the server versions the environment's machines run, from its deployment
 *   file in the server images' registry (`deployment.ts`), or `null` when it has none. Kept in
 *   the edge cache for DEPLOYMENT_CACHE_SECONDS; 502 when the registry fails.
 * - `GET /`: the overview page, static files in public/ that read these lists.
 * - `POST /traces?id=<id>` from a page opened with `?trace`: its trace as text, at most
 *   MAX_TRACE_BYTES, replacing that id's earlier upload; `GET /traces` lists the latest
 *   MAX_TRACES, newest first. A debugging aid for devices with no inspector at hand.
 *
 * Each environment is one Durable Object that holds its servers' latest announcements in
 * memory. Every request first drops the entries older than ENTRY_TTL_MS, so the list holds
 * the servers seen in the last 45 s with no timers or alarms: the object can hibernate
 * between requests, which keeps its duration billing near zero. Hibernating discards memory,
 * so a SQL table keeps each server too, written only when a server appears, when its rooms
 * open or close, at most every STORED_REFRESH_MS otherwise, and when it leaves: Cloudflare
 * bills every row written (100,000 a day on the free plan), and rewriting the row at each
 * announcement used them up.
 * There is no authentication.
 *
 * Deploy with `bun run directory:deploy`.
 */
import { DurableObject } from "cloudflare:workers";
import {
  ENTRY_TTL_MS,
  ENVIRONMENTS,
  roomsAt,
  MAX_BODY_BYTES,
  MAX_ENTRIES_PER_MACHINE,
  announcedVersion,
  listServers,
  machineOf,
  nipUrl,
  parseAnnouncement,
  parseEnvironment,
  readBody,
  serverUrl,
  type Announcement,
  type ListedServer,
} from "./listing";
import { DEPLOYMENT_CACHE_SECONDS, readDeployment } from "./deployment";

interface Env {
  DIRECTORY: DurableObjectNamespace<Directory>;
}

const PREFLIGHT_MAX_AGE_SECONDS = 86_400;
/** In UTF-8 bytes. The page uploads at most 60,000 characters of notes (index.html), in
 * keepalive requests the browser caps at 64 KB; the largest traces stored are 60 KB. */
const MAX_TRACE_BYTES = 256 * 1024;
const MAX_TRACES = 40;
/** How stale a listed server's stored row may grow. A server that keeps announcing has its
 * row rewritten this often, so after a wake from hibernation its stored `seen` is at most
 * this (and one announcement interval) behind the last announcement. */
const STORED_REFRESH_MS = 120_000;
// Any page may read the listing, and it changes with every announcement.
const HEADERS = { "Access-Control-Allow-Origin": "*", "Cache-Control": "no-store" };

function text(message: string, status: number): Response {
  return new Response(message, { status, headers: HEADERS });
}

async function announce(
  request: Request,
  version: string,
  directory: DurableObjectStub<Directory>,
): Promise<Response> {
  const base = nipUrl(request.headers.get("CF-Connecting-IP"));
  if (!base) {
    return text("announce over IPv4", 400);
  }
  const body = await readBody(request, MAX_BODY_BYTES);
  if (!body) {
    return text(`announcements are limited to ${MAX_BODY_BYTES} bytes`, 413);
  }
  const announcement = parseAnnouncement(new TextDecoder().decode(body));
  if (!announcement) {
    return text("the body must be a JSON object with a rooms array", 400);
  }
  const url = serverUrl(base, version, announcement);
  if (!url) {
    return text("a server announces under its own content version", 400);
  }
  if (!(await directory.announce(url, announcement))) {
    return text(`a machine lists at most ${MAX_ENTRIES_PER_MACHINE} servers`, 429);
  }
  return new Response(null, { status: 204, headers: HEADERS });
}

async function traces(request: Request, url: URL, env: Env): Promise<Response> {
  const store = env.DIRECTORY.get(env.DIRECTORY.idFromName("traces"));
  if (request.method === "GET") {
    return Response.json({ traces: await store.traces() }, { headers: HEADERS });
  }
  const id = url.searchParams.get("id") ?? "";
  if (request.method !== "POST" || !/^[\w-]{1,64}$/.test(id)) {
    return text("POST a trace with an id", 400);
  }
  const body = await readBody(request, MAX_TRACE_BYTES);
  if (!body) {
    return text(`traces are limited to ${MAX_TRACE_BYTES} bytes`, 413);
  }
  const trace = new TextDecoder().decode(body);
  await store.addTrace(id, request.headers.get("User-Agent") ?? "", trace);
  return new Response(null, { status: 204, headers: HEADERS });
}

/** The environment's deployment file, from the edge cache when a recent read is there. */
async function deployment(
  request: Request,
  url: URL,
  context: ExecutionContext,
): Promise<Response> {
  const environment = parseEnvironment(url.searchParams.get("environment"));
  if (request.method !== "GET" || !environment) {
    return text("GET a deployment with environment=" + ENVIRONMENTS.join("|"), 400);
  }
  const key = new Request(`${url.origin}/deployment?environment=${environment}`);
  const cached = await caches.default.match(key);
  if (cached) return cached;
  let file;
  try {
    file = await readDeployment(environment);
  } catch (error) {
    return text(`the registry: ${(error as Error).message}`, 502);
  }
  const response = Response.json(
    { deployment: file ?? null },
    {
      headers: {
        ...HEADERS,
        "Cache-Control": `public, max-age=${DEPLOYMENT_CACHE_SECONDS}`,
      },
    },
  );
  context.waitUntil(caches.default.put(key, response.clone()));
  return response;
}

export default {
  async fetch(request: Request, env: Env, context: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/traces") {
      return traces(request, url, env);
    }
    if (url.pathname === "/deployment") {
      return deployment(request, url, context);
    }
    const version = announcedVersion(url.pathname);
    if (version === undefined || (version !== "" && request.method !== "PUT")) {
      return text("Not found", 404);
    }
    const environment = parseEnvironment(url.searchParams.get("environment"));
    if (!environment) {
      return text("environment must be one of " + ENVIRONMENTS.join(", "), 400);
    }
    const directory = env.DIRECTORY.get(env.DIRECTORY.idFromName(environment));
    if (request.method === "GET") {
      const contentVersion = url.searchParams.get("contentVersion") ?? undefined;
      const all = url.searchParams.get("all") === "1";
      return Response.json(
        { servers: await directory.servers({ contentVersion, all }) },
        { headers: HEADERS },
      );
    }
    if (request.method === "PUT") {
      return announce(request, version, directory);
    }
    if (request.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: {
          ...HEADERS,
          "Access-Control-Allow-Methods": "GET",
          "Access-Control-Max-Age": String(PREFLIGHT_MAX_AGE_SECONDS),
        },
      });
    }
    return new Response("Method not allowed", {
      status: 405,
      headers: { ...HEADERS, Allow: "GET, PUT, OPTIONS" },
    });
  },
} satisfies ExportedHandler<Env>;

/** A listed server: when it was last seen (or, restored from storage, may have been), its
 * latest announcement known here and when that arrived, and when its row was stored with
 * which rooms. */
interface Entry {
  seen: number;
  announcement: Announcement;
  arrived: number;
  stored: number;
  storedRooms: string;
}

/** The rooms an announcement lists, by code: a change stores the server's row, so a wake from
 * hibernation lists no room that has ended nor misses one that has opened. */
function roomCodes(announcement: Announcement): string {
  return announcement.rooms.map((room) => room.room).join(" ");
}

/** One environment's servers: the latest announcement of each in memory, and a row each in
 * SQLite storage to restore them after hibernation. */
export class Directory extends DurableObject<Env> {
  private readonly entries = new Map<string, Entry>();

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    ctx.storage.sql.exec(`CREATE TABLE IF NOT EXISTS servers (
      url TEXT PRIMARY KEY,
      seen INTEGER NOT NULL,
      announcement TEXT NOT NULL
    )`);
    ctx.storage.sql.exec(`CREATE TABLE IF NOT EXISTS traces (
      id TEXT PRIMARY KEY,
      at INTEGER NOT NULL,
      agent TEXT NOT NULL,
      trace TEXT NOT NULL
    )`);
    // A stored row's `seen` lags the server's last announcement by less than
    // STORED_REFRESH_MS and one interval, so a restored server counts as seen up to
    // STORED_REFRESH_MS later: a live one stays listed until it announces again, and one that
    // went quiet leaves at most that much later than it would have without the wake.
    const now = Date.now();
    const rows = ctx.storage.sql
      .exec<{ url: string; seen: number; announcement: string }>(
        "SELECT url, seen, announcement FROM servers",
      )
      .toArray();
    for (const row of rows) {
      const announcement = JSON.parse(row.announcement) as Announcement;
      this.entries.set(row.url, {
        seen: Math.min(now, row.seen + STORED_REFRESH_MS),
        announcement,
        arrived: row.seen,
        stored: row.seen,
        storedRooms: roomCodes(announcement),
      });
    }
  }

  addTrace(id: string, agent: string, trace: string): void {
    this.ctx.storage.sql.exec(
      "INSERT OR REPLACE INTO traces (id, at, agent, trace) VALUES (?, ?, ?, ?)",
      id,
      Date.now(),
      agent,
      trace,
    );
    this.ctx.storage.sql.exec(
      "DELETE FROM traces WHERE id NOT IN (SELECT id FROM traces ORDER BY at DESC LIMIT ?)",
      MAX_TRACES,
    );
  }

  traces(): { id: string; at: number; agent: string; trace: string }[] {
    return this.ctx.storage.sql
      .exec<{ id: string; at: number; agent: string; trace: string }>(
        "SELECT id, at, agent, trace FROM traces ORDER BY at DESC",
      )
      .toArray();
  }

  /** Keeps the announcement, unless it would give its machine more than
   * MAX_ENTRIES_PER_MACHINE entries, and stores its row when the server is new, its rooms
   * opened or closed, or its row is STORED_REFRESH_MS old. */
  announce(url: string, announcement: Announcement): boolean {
    const now = Date.now();
    this.dropExpired(now);
    const machine = machineOf(url);
    let others = 0;
    for (const listed of this.entries.keys()) {
      if (listed !== url && machineOf(listed) === machine) others++;
    }
    if (others >= MAX_ENTRIES_PER_MACHINE) return false;
    const entry = this.entries.get(url);
    const rooms = roomCodes(announcement);
    let { stored, storedRooms } = entry ?? { stored: now, storedRooms: rooms };
    if (!entry || rooms !== entry.storedRooms || now - entry.stored >= STORED_REFRESH_MS) {
      this.ctx.storage.sql.exec(
        "INSERT OR REPLACE INTO servers (url, seen, announcement) VALUES (?, ?, ?)",
        url,
        now,
        JSON.stringify(announcement),
      );
      stored = now;
      storedRooms = rooms;
    }
    this.entries.set(url, { seen: now, announcement, arrived: now, stored, storedRooms });
    return true;
  }

  servers(which: { contentVersion?: string; all?: boolean }): ListedServer[] {
    const now = Date.now();
    this.dropExpired(now);
    return listServers(
      [...this.entries].map(([url, { seen, announcement, arrived }]) => ({
        ...announcement,
        rooms: roomsAt(announcement.rooms, arrived, now),
        seen,
        url,
      })),
      which,
    );
  }

  /** A server leaves the list ENTRY_TTL_MS after its last announcement. */
  private dropExpired(now: number): void {
    for (const [url, { seen }] of this.entries) {
      if (now - seen > ENTRY_TTL_MS) {
        this.entries.delete(url);
        this.ctx.storage.sql.exec("DELETE FROM servers WHERE url = ?", url);
      }
    }
  }
}
