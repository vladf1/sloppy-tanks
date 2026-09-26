import { DurableObject } from "cloudflare:workers";
import {
  BOT_NAME_PREFIX,
  BotPlayer,
  ROOM_SEATS,
  openSeats,
  randomRoomCode,
  type RoomListingSummary,
  type ServerInfo,
} from "./bot-player";
import { controlPage } from "./control-page";

interface Env {
  SWARM: DurableObjectNamespace<BotSwarm>;
  SERVER_URL: string;
  BOT_ORIGIN: string;
}

/** Durable Object location hints; each region runs at most one swarm object. */
export const REGIONS = ["wnam", "enam", "sam", "weur", "eeur", "apac", "oc", "afr", "me"] as const;
type Region = (typeof REGIONS)[number];
const MAX_BOTS_PER_REGION = 32;
const DEFAULT_MINUTES = 30;
// Every running bot keeps its Durable Object awake and billed for wall time, so runs always end.
const MAX_MINUTES = 6 * 60;
const TICK_MS = 25;
// Directory polls and joins share the server's per-IP edge limits (120 listings, 60 joins a minute).
const MAINTAIN_MS = 10_000;
const MAX_JOINS_PER_PASS = 8;
// The alarm restarts bots after a runtime restart evicts the in-memory sockets.
const WATCHDOG_MS = 30_000;
const BLOCKED_ROOM_MS = 60_000;
const CONNECT_TIMEOUT_MS = 10_000;

interface SwarmConfig {
  region: Region;
  bots: number;
  stopAt: number;
  /** Target one room code instead of browsing the directory. */
  room?: string;
  /** When no listed room has space, create a bot room instead of waiting for people. */
  host: boolean;
  perRoom: number;
}
interface Seat {
  bot: BotPlayer;
  room?: string;
  socket?: WebSocket;
  connecting: boolean;
}

function json(value: unknown, status = 200): Response {
  return Response.json(value, { status, headers: { "Cache-Control": "no-store" } });
}
function swarm(env: Env, region: Region): DurableObjectStub<BotSwarm> {
  // The hint only takes effect when the object is first created, so every lookup passes it.
  return env.SWARM.get(env.SWARM.idFromName("swarm-" + region), { locationHint: region });
}
function region(value: unknown): Region {
  const found = REGIONS.find((item) => item === value);
  if (!found) {
    throw new Error("region must be one of " + REGIONS.join(", "));
  }
  return found;
}
function integer(
  value: unknown,
  fallback: number,
  min: number,
  max: number,
  label: string,
): number {
  const number = value === undefined || value === "" ? fallback : Number(value);
  if (!Number.isInteger(number) || number < min || number > max) {
    throw new Error(`${label} must be an integer from ${min} to ${max}`);
  }
  return number;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/") {
      return new Response(controlPage, {
        headers: { "Content-Type": "text/html; charset=utf-8", "X-Robots-Tag": "noindex" },
      });
    }
    if (!url.pathname.startsWith("/api/")) {
      return new Response("Not found", { status: 404 });
    }
    try {
      if (url.pathname === "/api/status" && request.method === "GET") {
        const regions = await Promise.all(REGIONS.map((name) => swarm(env, name).status()));
        return json({ server: env.SERVER_URL, regions: regions.filter((item) => item.running) });
      }
      if (request.method !== "POST") {
        return json({ error: "Not found" }, 404);
      }
      const body = (await request.json().catch(() => ({}))) as Record<string, unknown>;
      if (url.pathname === "/api/start") {
        const now = Date.now();
        const room = body.room ? String(body.room).toUpperCase() : undefined;
        if (room !== undefined && !/^[A-Z2-9]{8}$/.test(room)) {
          throw new Error("room must be an 8-character room code");
        }
        const config: SwarmConfig = {
          region: region(body.region),
          bots: integer(body.bots, 1, 1, MAX_BOTS_PER_REGION, "bots"),
          stopAt: now + integer(body.minutes, DEFAULT_MINUTES, 1, MAX_MINUTES, "minutes") * 60_000,
          room,
          host: body.host === true,
          perRoom: integer(body.perRoom, 1, 1, ROOM_SEATS, "perRoom"),
        };
        return json(await swarm(env, config.region).start(config));
      }
      if (url.pathname === "/api/stop") {
        const targets = body.region === undefined ? [...REGIONS] : [region(body.region)];
        await Promise.all(targets.map((name) => swarm(env, name).stop()));
        return json({ stopped: targets });
      }
      return json({ error: "Not found" }, 404);
    } catch (error) {
      return json({ error: error instanceof Error ? error.message : String(error) }, 400);
    }
  },
} satisfies ExportedHandler<Env>;

/** Runs one region's bots. Its outbound sockets keep it awake only while a run is configured. */
export class BotSwarm extends DurableObject<Env> {
  private config?: SwarmConfig;
  private seats: Seat[] = [];
  private timer?: ReturnType<typeof setInterval>;
  private server?: ServerInfo;
  private colo?: string;
  private coloRequested = false;
  private lastMaintainMs = 0;
  private maintaining = false;
  private blocked = new Map<string, number>();
  private lastError?: string;
  /** Bot counters live in memory, so rates restart with the object. */
  private countingSinceMs = Date.now();

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    void ctx.blockConcurrencyWhile(async () => {
      this.config = await ctx.storage.get<SwarmConfig>("config");
    });
  }

  async start(config: SwarmConfig): Promise<ReturnType<BotSwarm["summary"]>> {
    // Starting an identical run again only moves its deadline.
    if (
      JSON.stringify({ ...this.config, stopAt: 0 }) !== JSON.stringify({ ...config, stopAt: 0 })
    ) {
      this.release();
    }
    this.config = config;
    await this.ctx.storage.put("config", config);
    await this.ctx.storage.setAlarm(Date.now() + WATCHDOG_MS);
    this.run();
    await this.maintain(Date.now());
    return this.summary();
  }

  async stop(): Promise<void> {
    this.config = undefined;
    this.release();
    await this.ctx.storage.deleteAll();
    await this.ctx.storage.deleteAlarm();
  }

  status(): ReturnType<BotSwarm["summary"]> {
    if (this.config && Date.now() < this.config.stopAt) {
      this.run();
    }
    return this.summary();
  }

  override async alarm(): Promise<void> {
    const config = this.config;
    if (!config || Date.now() >= config.stopAt) {
      await this.stop();
      return;
    }
    this.run();
    await this.ctx.storage.setAlarm(Math.min(config.stopAt, Date.now() + WATCHDOG_MS));
  }

  private summary() {
    const config = this.config;
    const bots = this.seats.map(({ bot, room, socket }) => ({
      name: bot.name,
      room: room ?? null,
      connected: !!socket,
      phase: socket ? bot.phase : "seeking",
      rttMs: bot.stats.rttMs ?? null,
      lastError: bot.lastError ?? null,
      ...bot.stats,
    }));
    const elapsed = Math.max(1, (Date.now() - this.countingSinceMs) / 1000);
    const bytesIn = bots.reduce((sum, bot) => sum + bot.bytesIn, 0);
    const bytesOut = bots.reduce((sum, bot) => sum + bot.bytesOut, 0);
    return {
      region: config?.region ?? null,
      colo: this.colo ?? null,
      running: !!config,
      config: config ?? null,
      contentVersion: this.server?.contentVersion ?? null,
      lastError: this.lastError ?? null,
      rooms: [...new Set(bots.flatMap((bot) => (bot.connected && bot.room ? [bot.room] : [])))],
      bytesIn,
      bytesOut,
      bytesInPerSecond: Math.round(bytesIn / elapsed),
      bytesOutPerSecond: Math.round(bytesOut / elapsed),
      bots,
    };
  }

  private run(): void {
    const config = this.config;
    if (!config) {
      return;
    }
    if (!this.seats.length) {
      this.countingSinceMs = Date.now();
    }
    while (this.seats.length < config.bots) {
      const name = `${BOT_NAME_PREFIX}${config.region}-${this.seats.length + 1}`;
      this.seats.push({ bot: new BotPlayer(name), connecting: false });
    }
    for (const seat of this.seats.splice(config.bots)) {
      this.disconnect(seat, true);
    }
    this.timer ??= setInterval(() => {
      const now = Date.now();
      if (!this.config || now >= this.config.stopAt) {
        void this.stop();
        return;
      }
      for (const { bot, socket } of this.seats) {
        if (socket) {
          bot.update(now);
        }
      }
      if (now - this.lastMaintainMs >= MAINTAIN_MS) {
        void this.maintain(now);
      }
    }, TICK_MS);
    if (!this.coloRequested) {
      // Location hints are best-effort; report where the swarm actually runs.
      this.coloRequested = true;
      void fetch("https://cloudflare.com/cdn-cgi/trace")
        .then((response) => response.text())
        .then((text) => (this.colo = /^colo=(\w+)$/m.exec(text)?.[1]))
        .catch(() => (this.coloRequested = false));
    }
  }

  /** Assigns idle bots to rooms: their previous seat, the configured room, open rooms, or a new room. */
  private async maintain(now: number): Promise<void> {
    if (this.maintaining || !this.config) {
      return;
    }
    this.maintaining = true;
    this.lastMaintainMs = now;
    try {
      const config = this.config;
      const idle = this.seats
        .filter((seat) => !seat.socket && !seat.connecting)
        .slice(0, MAX_JOINS_PER_PASS);
      if (!idle.length) {
        return;
      }
      this.server ??= await this.health();
      for (const [room, until] of this.blocked) {
        if (now >= until) {
          this.blocked.delete(room);
        }
      }
      const rejoining = idle.filter((seat) => seat.bot.token && seat.room);
      for (const seat of rejoining) {
        this.joinRoom(seat, seat.room!, false);
      }
      const seekers = idle.filter((seat) => !rejoining.includes(seat));
      if (!seekers.length) {
        return;
      }
      if (config.room) {
        for (const seat of seekers) {
          if (!this.blocked.has(config.room)) {
            this.joinRoom(seat, config.room, false);
          }
        }
        return;
      }
      const ours = new Map<string, number>();
      for (const seat of this.seats) {
        if (seat.room && (seat.socket || seat.connecting)) {
          ours.set(seat.room, (ours.get(seat.room) ?? 0) + 1);
        }
      }
      // Bots in a listed room are already in its reserved count; only in-flight joins are pending.
      const pending = new Map<string, number>();
      for (const seat of this.seats) {
        if (seat.room && seat.connecting) {
          pending.set(seat.room, (pending.get(seat.room) ?? 0) + 1);
        }
      }
      const listed = await this.listRooms();
      // Directory writes lag joins; count rooms our bots already sit in even before they appear.
      for (const seat of this.seats) {
        const room = seat.room;
        if (room && seat.socket && seat.bot.phase !== "joining") {
          if (!listed.some((entry) => entry.room === room)) {
            listed.push({ room, contentVersion: this.server.contentVersion, reserved: 0 });
          }
        }
      }
      for (const entry of listed) {
        const joined = this.seats.filter(
          (seat) => seat.room === entry.room && seat.socket && seat.bot.phase !== "joining",
        ).length;
        entry.reserved = Math.max(entry.reserved, joined);
      }
      const rooms = openSeats(
        listed,
        this.server.contentVersion,
        pending,
        new Set(this.blocked.keys()),
      );
      for (const seat of seekers) {
        const open = rooms.find(
          (room) => room.free > 0 && (ours.get(room.room) ?? 0) < config.perRoom,
        );
        if (open) {
          open.free--;
          ours.set(open.room, (ours.get(open.room) ?? 0) + 1);
          this.joinRoom(seat, open.room, false);
        } else if (config.host) {
          // Others wait for the next pass so the creator, not a joiner, becomes the host.
          this.joinRoom(seat, randomRoomCode(), true);
          break;
        }
      }
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
      this.server = undefined;
    } finally {
      this.maintaining = false;
    }
  }

  private async health(): Promise<ServerInfo> {
    const response = await fetch(this.env.SERVER_URL + "/health");
    const health = (await response.json()) as Record<string, unknown>;
    return { version: Number(health.version), contentVersion: String(health.contentVersion) };
  }

  private async listRooms(): Promise<RoomListingSummary[]> {
    const response = await fetch(this.env.SERVER_URL + "/rooms", {
      headers: { Origin: this.env.BOT_ORIGIN },
    });
    if (!response.ok) {
      throw new Error(`Room directory ${response.status}: ${await response.text()}`);
    }
    return ((await response.json()) as { rooms: RoomListingSummary[] }).rooms;
  }

  private joinRoom(seat: Seat, room: string, create: boolean): void {
    seat.room = room;
    seat.connecting = true;
    void (async () => {
      try {
        // The signal stays bound to the upgraded socket, so the timeout must end with the handshake.
        const abort = new AbortController();
        const timeout = setTimeout(() => abort.abort(), CONNECT_TIMEOUT_MS);
        const response = await fetch(this.env.SERVER_URL + "/room/" + room, {
          headers: { Upgrade: "websocket", Origin: this.env.BOT_ORIGIN },
          signal: abort.signal,
        }).finally(() => clearTimeout(timeout));
        const socket = response.webSocket;
        if (!socket) {
          seat.bot.lastError = `Room ${room} refused ${response.status}: ${await response.text()}`;
          seat.bot.token = undefined;
          // Edge rate limits are not about the room; retry it on the next pass.
          if (response.status !== 429) {
            this.blocked.set(room, Date.now() + BLOCKED_ROOM_MS);
          }
          return;
        }
        socket.accept();
        if (!this.seats.includes(seat) || !this.config) {
          socket.close(1000, "Bot stopped");
          return;
        }
        seat.socket = socket;
        socket.addEventListener("message", (event) => {
          if (typeof event.data !== "string") {
            return;
          }
          try {
            seat.bot.receive(event.data, Date.now());
          } catch (error) {
            seat.bot.lastError = error instanceof Error ? error.message : String(error);
          }
        });
        socket.addEventListener("close", () => this.closed(seat, socket));
        socket.addEventListener("error", () => this.closed(seat, socket));
        seat.bot.join((text) => socket.send(text), this.server!, Date.now(), create);
      } catch (error) {
        seat.bot.lastError = error instanceof Error ? error.message : String(error);
        seat.bot.token = undefined;
        this.blocked.set(room, Date.now() + BLOCKED_ROOM_MS);
      } finally {
        seat.connecting = false;
      }
    })();
  }

  private closed(seat: Seat, socket: WebSocket): void {
    if (seat.socket !== socket) {
      return;
    }
    seat.socket = undefined;
    seat.bot.disconnected();
    if (seat.bot.fatal && seat.room) {
      // Full, ended or incompatible rooms stay off the list briefly instead of being retried at once.
      this.blocked.set(seat.room, Date.now() + BLOCKED_ROOM_MS);
      if (seat.bot.lastError === "incompatible") {
        this.server = undefined;
      }
    }
  }

  private disconnect(seat: Seat, leave: boolean): void {
    const socket = seat.socket;
    seat.socket = undefined;
    if (!socket) {
      return;
    }
    if (leave) {
      seat.bot.leave();
    }
    seat.bot.disconnected();
    try {
      socket.close(1000, "Bot stopped");
    } catch {
      /* Already closed. */
    }
  }

  private release(): void {
    if (this.timer !== undefined) {
      clearInterval(this.timer);
      this.timer = undefined;
    }
    for (const seat of this.seats) {
      this.disconnect(seat, true);
    }
    this.seats = [];
    this.blocked.clear();
    this.lastError = undefined;
  }
}
