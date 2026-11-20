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
 * - `GET /`: the overview page, static files in public/ that read these lists.
 * - `POST /traces?id=<id>` from a page opened with `?trace`: its trace as text, at most
 *   MAX_TRACE_BYTES, replacing that id's earlier upload; `GET /traces` lists the latest
 *   MAX_TRACES, newest first. A debugging aid for devices with no inspector at hand.
 *
 * Each environment is one Durable Object with a SQL table of servers and their latest
 * announcements. Every request first deletes the rows older than ENTRY_TTL_MS, so the list holds the servers
 * seen in the last 45 s with no timers, alarms or in-memory state: the object hibernates between
 * requests, which keeps its duration billing near zero. There is no authentication.
 *
 * Deploy with `bun run directory:deploy`.
 */
import { DurableObject } from "cloudflare:workers";
import {
  ENTRY_TTL_MS,
  ENVIRONMENTS,
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

interface Env {
  DIRECTORY: DurableObjectNamespace<Directory>;
}

const PREFLIGHT_MAX_AGE_SECONDS = 86_400;
/** In UTF-8 bytes. The page uploads at most 60,000 characters of notes (index.html), in
 * keepalive requests the browser caps at 64 KB; the largest traces stored are 60 KB. */
const MAX_TRACE_BYTES = 256 * 1024;
const MAX_TRACES = 40;
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

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/traces") {
      return traces(request, url, env);
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

/** One environment's servers: a row each in its SQLite storage, holding the latest
 * announcement and when it arrived. */
export class Directory extends DurableObject<Env> {
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

  /** Stores the announcement, unless it would give its machine more than
   * MAX_ENTRIES_PER_MACHINE entries. */
  announce(url: string, announcement: Announcement): boolean {
    this.dropExpired();
    const machine = machineOf(url);
    const others = this.ctx.storage.sql
      .exec<{ url: string }>("SELECT url FROM servers")
      .toArray()
      .filter((row) => row.url !== url && machineOf(row.url) === machine).length;
    if (others >= MAX_ENTRIES_PER_MACHINE) return false;
    this.ctx.storage.sql.exec(
      "INSERT OR REPLACE INTO servers (url, seen, announcement) VALUES (?, ?, ?)",
      url,
      Date.now(),
      JSON.stringify(announcement),
    );
    return true;
  }

  servers(which: { contentVersion?: string; all?: boolean }): ListedServer[] {
    this.dropExpired();
    const rows = this.ctx.storage.sql
      .exec<{ url: string; seen: number; announcement: string }>(
        "SELECT url, seen, announcement FROM servers",
      )
      .toArray();
    return listServers(
      rows.map(({ url, seen, announcement }) => ({
        ...(JSON.parse(announcement) as Announcement),
        seen,
        url,
      })),
      which,
    );
  }

  /** A server leaves the list ENTRY_TTL_MS after its last announcement. */
  private dropExpired(): void {
    this.ctx.storage.sql.exec("DELETE FROM servers WHERE seen < ?", Date.now() - ENTRY_TTL_MS);
  }
}
