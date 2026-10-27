/**
 * The server directory: game servers announce their room listings here, and the game page
 * reads which servers are up and how busy they are.
 *
 * - `PUT /servers?environment=production|dev` from a game server: its `/health` fields plus a
 *   `rooms` array, as one JSON object of at most MAX_BODY_BYTES. The caller's IPv4 address names
 *   the entry, `wss://a-b-c-d.nip.io`, so a server can announce only itself. Replies 204.
 * - `GET /servers?environment=production|dev` from the page (or anyone):
 *   `{ "servers": [{ "url", ...announced fields, "seen" }] }`, fewest rooms first.
 * - `GET /`: the overview page, static files in public/ that read these lists.
 *
 * Each environment is one Durable Object with a SQL table of servers and their latest
 * announcements. Every request first deletes the rows older than ENTRY_TTL_MS, so the list holds the servers
 * seen in the last 45 s with no timers, alarms or in-memory state: the object hibernates between
 * requests, which keeps its duration billing near zero. There is no authentication.
 *
 * Deploy with `pnpm run directory:deploy`.
 */
import { DurableObject } from "cloudflare:workers";
import {
  ENTRY_TTL_MS,
  ENVIRONMENTS,
  MAX_BODY_BYTES,
  listServers,
  nipUrl,
  parseAnnouncement,
  parseEnvironment,
  type Announcement,
  type ListedServer,
} from "./listing";

interface Env {
  DIRECTORY: DurableObjectNamespace<Directory>;
}

const PREFLIGHT_MAX_AGE_SECONDS = 86_400;
// Any page may read the listing, and it changes with every announcement.
const HEADERS = { "Access-Control-Allow-Origin": "*", "Cache-Control": "no-store" };

function text(message: string, status: number): Response {
  return new Response(message, { status, headers: HEADERS });
}

async function announce(
  request: Request,
  directory: DurableObjectStub<Directory>,
): Promise<Response> {
  const url = nipUrl(request.headers.get("CF-Connecting-IP"));
  if (!url) {
    return text("announce over IPv4", 400);
  }
  const body = await request.arrayBuffer();
  if (body.byteLength > MAX_BODY_BYTES) {
    return text(`announcements are limited to ${MAX_BODY_BYTES} bytes`, 413);
  }
  const announcement = parseAnnouncement(new TextDecoder().decode(body));
  if (!announcement) {
    return text("the body must be a JSON object with a rooms array", 400);
  }
  await directory.announce(url, announcement);
  return new Response(null, { status: 204, headers: HEADERS });
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname !== "/servers") {
      return text("Not found", 404);
    }
    const environment = parseEnvironment(url.searchParams.get("environment"));
    if (!environment) {
      return text("environment must be one of " + ENVIRONMENTS.join(", "), 400);
    }
    const directory = env.DIRECTORY.get(env.DIRECTORY.idFromName(environment));
    if (request.method === "GET") {
      return Response.json({ servers: await directory.servers() }, { headers: HEADERS });
    }
    if (request.method === "PUT") {
      return announce(request, directory);
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
  }

  announce(url: string, announcement: Announcement): void {
    this.dropExpired();
    this.ctx.storage.sql.exec(
      "INSERT OR REPLACE INTO servers (url, seen, announcement) VALUES (?, ?, ?)",
      url,
      Date.now(),
      JSON.stringify(announcement),
    );
  }

  servers(): ListedServer[] {
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
    );
  }

  /** A server leaves the list ENTRY_TTL_MS after its last announcement. */
  private dropExpired(): void {
    this.ctx.storage.sql.exec("DELETE FROM servers WHERE seen < ?", Date.now() - ENTRY_TTL_MS);
  }
}
