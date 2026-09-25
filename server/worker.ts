import { DurableObject } from "cloudflare:workers";
import { Simulation } from "../src/game/simulation";
import { idleCommand } from "../src/game/types";
import { FixedStepClock, HOST_INTERVAL_MS } from "../src/net/fixed-step-clock";
import { experimentEvent, experimentState, ExperimentStream } from "../src/net/experiment-state";
import type { PlayerRoom } from "./player-room";
import type { RoomDirectory } from "./room-directory";
import { CONTENT_VERSION, PROTOCOL_VERSION, ROOM_CODE } from "../src/net/protocol";
export { PlayerRoom } from "./player-room";
export { RoomDirectory } from "./room-directory";

interface Env {
  ROOM: DurableObjectNamespace<Room>;
  EXPERIMENT_KEY?: string;
  EXPERIMENT_ENABLED: string;
  MULTIPLAYER_ENABLED: string;
  ALLOWED_ORIGINS: string;
  MATCH: DurableObjectNamespace<PlayerRoom>;
  DIRECTORY: DurableObjectNamespace<RoomDirectory>;
  DIRECTORY_RATE: RateLimit;
  CONNECTION_RATE: RateLimit;
  ENTRY_RATE: RateLimit;
}
const PROTOCOL = "multiplayer-hosting-experiment-v1";
const MAX_CLIENTS = 8;
const MAX_MESSAGE_BYTES = 2048;
const MAX_MESSAGES_PER_SECOND = 60;
const EMPTY_GRACE_MS = 30_000;
const CLIENT_TIMEOUT_MS = 5_000;
const MAX_ROOM_MS = 20 * 60_000;
const MAX_UNACKNOWLEDGED_TICKS = 180;
const MAPS = ["village", "harbor", "quarry"] as const;
type MapMode = (typeof MAPS)[number];
interface Client {
  lastSeenMs: number;
  observedTick: number;
  seq: number;
  windowMs: number;
  messages: number;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/health") {
      return Response.json({
        protocol: PROTOCOL,
        version: PROTOCOL_VERSION,
        contentVersion: CONTENT_VERSION,
        multiplayerEnabled: env.MULTIPLAYER_ENABLED === "true",
        experimental: true,
        enabled: env.EXPERIMENT_ENABLED === "true",
      });
    }
    const room = /^\/room\/([^/]+)$/.exec(url.pathname)?.[1];
    if (url.pathname === "/rooms") {
      const origin = request.headers.get("Origin") ?? "";
      if (!(env.ALLOWED_ORIGINS ?? "").split(",").includes(origin))
        return new Response("Origin not allowed", { status: 403 });
      const headers = {
        "Access-Control-Allow-Origin": origin,
        "Cache-Control": "no-store",
        Vary: "Origin",
      };
      if (request.method !== "GET") return new Response(null, { status: 405, headers });
      if (env.MULTIPLAYER_ENABLED !== "true")
        return new Response("Multiplayer unavailable", { status: 503, headers });
      if (
        !(
          await env.DIRECTORY_RATE.limit({
            key: request.headers.get("CF-Connecting-IP") ?? "local",
          })
        ).success
      )
        return new Response("Too many refreshes; try again shortly", { status: 429, headers });
      const response = await env.DIRECTORY.getByName("rooms").fetch("https://directory/rooms");
      return new Response(response.body, {
        status: response.status,
        headers: { ...headers, "Content-Type": "application/json" },
      });
    }
    // Stats probe: the Worker times its own request to the room, so the edge-to-room leg is
    // measured on the edge clock without relaying gameplay traffic.
    const probed = /^\/room\/([^/]+)\/ping$/.exec(url.pathname)?.[1];
    if (probed && ROOM_CODE.test(probed)) {
      const origin = request.headers.get("Origin") ?? "";
      if (
        env.MULTIPLAYER_ENABLED !== "true" ||
        !(env.ALLOWED_ORIGINS ?? "").split(",").includes(origin)
      )
        return new Response("Probe unavailable", { status: 403 });
      const headers = {
        "Access-Control-Allow-Origin": origin,
        "Cache-Control": "no-store",
        Vary: "Origin",
      };
      const ip = request.headers.get("CF-Connecting-IP") ?? "local";
      if (!(await env.DIRECTORY_RATE.limit({ key: "ping:" + ip })).success)
        return new Response(null, { status: 429, headers });
      const started = performance.now();
      await env.MATCH.get(env.MATCH.idFromName(probed)).fetch("https://room/ping");
      return Response.json({ roomMs: performance.now() - started }, { headers });
    }
    if (room && ROOM_CODE.test(room)) {
      if (env.MULTIPLAYER_ENABLED !== "true")
        return new Response("Multiplayer unavailable", { status: 503 });
      if (!(env.ALLOWED_ORIGINS ?? "").split(",").includes(request.headers.get("Origin") ?? ""))
        return new Response("Origin not allowed", { status: 403 });
      if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket")
        return new Response("WebSocket required", { status: 426 });
      const ip = request.headers.get("CF-Connecting-IP") ?? "local";
      if (
        !(await env.CONNECTION_RATE.limit({ key: ip })).success ||
        !(await env.ENTRY_RATE.limit({ key: "rooms" })).success
      )
        return new Response("Too many room connections; try again shortly", { status: 429 });
      return env.MATCH.get(env.MATCH.idFromName(room)).fetch(request);
    }
    if (env.EXPERIMENT_ENABLED !== "true") {
      return new Response("Experiment disabled", { status: 503 });
    }
    // Auth precedes DO lookup: arbitrary public requests cannot start simulations.
    if (
      !env.EXPERIMENT_KEY ||
      request.headers.get("Authorization") !== `Bearer ${env.EXPERIMENT_KEY}`
    ) {
      return new Response("Unauthorized", { status: 401 });
    }
    const code = /^\/room\/([a-zA-Z0-9-]{6,64})$/.exec(url.pathname)?.[1];
    if (!code || request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return new Response("Expected experimental room WebSocket", { status: 400 });
    }
    const map = url.searchParams.get("map") ?? "village";
    const seed = Number(url.searchParams.get("seed") ?? 4242);
    if (
      !MAPS.includes(map as MapMode) ||
      !Number.isSafeInteger(seed) ||
      seed < 0 ||
      seed > 0xffffffff
    ) {
      return new Response("Invalid map or seed", { status: 400 });
    }
    return env.ROOM.get(env.ROOM.idFromName(code)).fetch(request);
  },
} satisfies ExportedHandler<Env>;

/** Bot-only M1 host. Public player sessions are a later protocol, not this lab API. */
export class Room extends DurableObject<Env> {
  private simulation?: Simulation;
  private clock?: FixedStepClock;
  private stream = new ExperimentStream();
  private epoch = crypto.randomUUID();
  private clients = new Map<WebSocket, Client>();
  private timer?: ReturnType<typeof setTimeout>;
  private createdMs = 0;
  private emptySinceMs?: number;
  private eventCursor = 0;
  private pendingEvents: ReturnType<typeof experimentEvent>[] = [];
  private map?: MapMode;
  private seed?: number;
  private maxDebtMs = 0;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    // A runtime restart loses the in-memory match. Never silently revive old sockets.
    for (const socket of ctx.getWebSockets()) {
      socket.send(
        JSON.stringify({ type: "room-reset", roomEpoch: this.epoch, reason: "runtime-restart" }),
      );
      socket.close(1012, "Room restarted");
    }
  }

  override fetch(request: Request): Response {
    if (this.clients.size >= MAX_CLIENTS) {
      return new Response("Room full", { status: 409 });
    }
    const url = new URL(request.url);
    const map = (url.searchParams.get("map") ?? "village") as MapMode;
    const seed = Number(url.searchParams.get("seed") ?? 4242);
    if (this.simulation && (this.map !== map || this.seed !== seed)) {
      return new Response("Existing room settings differ", { status: 409 });
    }
    const now = Date.now();
    if (!this.simulation) {
      this.map = map;
      this.seed = seed;
      this.createdMs = now;
      this.epoch = crypto.randomUUID();
      this.stream = new ExperimentStream();
      this.eventCursor = 0;
      this.maxDebtMs = 0;
      this.simulation = new Simulation(seed, {
        mapMode: map,
        gameMode: "team",
        endlessMatch: true,
      });
      this.simulation.start();
      this.clock = new FixedStepClock(now);
      console.log(JSON.stringify({ type: "room-created", map, seed, roomEpoch: this.epoch }));
    }
    const pair = new WebSocketPair();
    const socket = pair[1];
    this.ctx.acceptWebSocket(socket);
    this.clients.set(socket, {
      lastSeenMs: now,
      observedTick: this.clock!.tick,
      seq: 0,
      windowMs: now,
      messages: 0,
    });
    this.emptySinceMs = undefined;
    socket.send(
      JSON.stringify({
        type: "welcome",
        protocol: PROTOCOL,
        roomEpoch: this.epoch,
        map,
        seed,
        botOnly: true,
        tick: this.clock!.tick,
      }),
    );
    this.sendFull(socket);
    if (this.timer === undefined) {
      this.schedule();
    }
    return new Response(null, { status: 101, webSocket: pair[0] });
  }

  override webSocketMessage(socket: WebSocket, message: string | ArrayBuffer): void {
    const client = this.clients.get(socket);
    if (!client || !this.clock || !this.simulation) {
      socket.close(1008, "Room expired");
      return;
    }
    if (
      typeof message !== "string" ||
      new TextEncoder().encode(message).byteLength > MAX_MESSAGE_BYTES
    ) {
      this.drop(socket, 1009, "Invalid message size");
      return;
    }
    const now = Date.now();
    if (now - client.windowMs >= 1000) {
      client.windowMs = now;
      client.messages = 0;
    }
    if (++client.messages > MAX_MESSAGES_PER_SECOND) {
      this.drop(socket, 1008, "Message rate exceeded");
      return;
    }
    let data: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(message);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error("Invalid message");
      }
      data = parsed as Record<string, unknown>;
    } catch {
      this.drop(socket, 1008, "Invalid JSON");
      return;
    }
    if (data.type === "input" || data.type === "ping") {
      if (
        typeof data.observedTick !== "number" ||
        !Number.isInteger(data.observedTick) ||
        data.observedTick < client.observedTick ||
        data.observedTick > this.clock.tick
      ) {
        this.drop(socket, 1008, "Invalid observed tick");
        return;
      }
      if (data.type === "input") {
        if (
          typeof data.seq !== "number" ||
          !Number.isSafeInteger(data.seq) ||
          data.seq <= client.seq ||
          typeof data.moveX !== "number" ||
          !Number.isFinite(data.moveX) ||
          Math.abs(data.moveX) > 1 ||
          typeof data.moveZ !== "number" ||
          !Number.isFinite(data.moveZ) ||
          Math.abs(data.moveZ) > 1 ||
          typeof data.aim !== "number" ||
          !Number.isFinite(data.aim) ||
          typeof data.fire !== "boolean" ||
          !Array.isArray(data.actions) ||
          data.actions.length > 8
        ) {
          this.drop(socket, 1008, "Invalid input");
          return;
        }
        client.seq = data.seq;
      } else {
        socket.send(JSON.stringify({ type: "pong", tick: this.clock.tick, t: data.t }));
      }
      client.observedTick = data.observedTick;
      client.lastSeenMs = now;
    } else if (data.type === "full") {
      this.sendFull(socket);
    } else if (data.type === "collapse") {
      // Reproducible burst on each map, independent of the evolving bot decisions.
      for (const cover of this.simulation.covers
        .filter((cover) => cover.alive && cover.destructible)
        .slice(0, 8)) {
        this.simulation.damageCover(cover, 10000, -1, 0);
      }
      this.drainEvents(this.clock.tick);
    } else if (data.type === "stall") {
      clearTimeout(this.timer);
      this.timer = setTimeout(() => this.advance(), 350);
    } else if (data.type === "restart") {
      this.dispose("deliberate-restart");
    } else {
      this.drop(socket, 1008, "Unknown message");
    }
  }

  override webSocketClose(socket: WebSocket): void {
    this.forget(socket);
  }
  override webSocketError(socket: WebSocket): void {
    this.drop(socket, 1011, "Socket failure");
  }

  private sendFull(socket: WebSocket): void {
    socket.send(
      JSON.stringify({
        ...this.stream.full(experimentState(this.simulation!), this.clock!.tick, this.eventCursor),
        roomEpoch: this.epoch,
      }),
    );
  }
  private drainEvents(tick: number): void {
    for (const event of this.simulation!.events.splice(0)) {
      this.pendingEvents.push(experimentEvent(event, tick, ++this.eventCursor));
    }
  }
  private schedule(): void {
    this.timer = setTimeout(() => this.advance(), HOST_INTERVAL_MS);
  }
  private advance(): void {
    this.timer = undefined;
    const now = Date.now();
    if (
      (this.emptySinceMs !== undefined && now - this.emptySinceMs >= EMPTY_GRACE_MS) ||
      now - this.createdMs >= MAX_ROOM_MS
    ) {
      this.dispose("expired");
      return;
    }
    try {
      const clock = this.clock!;
      const simulation = this.simulation!;
      if (
        !clock.advance(now, (tick) => {
          simulation.step(idleCommand(), true);
          this.drainEvents(tick);
        })
      ) {
        this.dispose("overload");
        return;
      }
      this.maxDebtMs = Math.max(this.maxDebtMs, clock.debtMs);
      const events = this.pendingEvents;
      this.pendingEvents = [];
      const body = JSON.stringify({
        ...this.stream.snapshot(experimentState(simulation), clock.tick, events),
        roomEpoch: this.epoch,
        debtMs: clock.debtMs,
      });
      for (const [socket, client] of this.clients) {
        if (
          now - client.lastSeenMs > CLIENT_TIMEOUT_MS ||
          clock.tick - client.observedTick > MAX_UNACKNOWLEDGED_TICKS
        ) {
          this.drop(socket, 1008, "Client is not consuming snapshots");
          continue;
        }
        try {
          // The large shared body is serialized once. This ack is traffic-only: M1 bots own all controls.
          socket.send(`{"ack":${client.seq},"snapshot":${body}}`);
        } catch {
          this.drop(socket, 1011, "Send failed");
        }
      }
      this.schedule();
    } catch (error) {
      console.error("Simulation failed", error);
      this.dispose("simulation-error");
    }
  }
  private forget(socket: WebSocket): void {
    this.clients.delete(socket);
    if (!this.clients.size && this.emptySinceMs === undefined) {
      this.emptySinceMs = Date.now();
    }
  }
  private drop(socket: WebSocket, code: number, reason: string): void {
    this.forget(socket);
    try {
      socket.close(code, reason);
    } catch {
      /* Already closed. */
    }
  }
  private dispose(reason: string): void {
    clearTimeout(this.timer);
    this.timer = undefined;
    for (const socket of this.clients.keys()) {
      try {
        socket.send(JSON.stringify({ type: "room-reset", roomEpoch: this.epoch, reason }));
        socket.close(1012, reason);
      } catch {
        /* Broken sockets cannot receive the reset. */
      }
    }
    console.log(
      JSON.stringify({
        type: "room-disposed",
        reason,
        roomEpoch: this.epoch,
        tick: this.clock?.tick,
        maxDebtMs: this.maxDebtMs,
        lifetimeMs: Date.now() - this.createdMs,
      }),
    );
    this.clients.clear();
    this.simulation?.dispose();
    this.simulation = undefined;
    this.clock = undefined;
    this.pendingEvents = [];
    this.emptySinceMs = undefined;
  }
}
