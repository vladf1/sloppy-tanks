import { DurableObject } from "cloudflare:workers";
import { MatchHost } from "../src/net/match-host";
import { HOST_INTERVAL_MS } from "../src/net/fixed-step-clock";
import { MAX_CLIENT_MESSAGE_BYTES } from "../src/net/protocol";
import type { RoomDirectory } from "./room-directory";
import { executionColo, SampleWindow } from "./diagnostics";

const MAX_PENDING_CONNECTIONS = 16;
const JOIN_TIMEOUT_MS = 5000;
const DIRECTORY_HEARTBEAT_MS = 20_000;
/** Five seconds of 20 Hz timer deadlines. */
const TIMER_LATENESS_SAMPLES = 100;
/** Stats sockets are one per open Stats for nerds panel; they never hold a seat. */
const MAX_PROBE_SOCKETS = 16;
const MAX_PROBE_MESSAGE_BYTES = 16;
interface Env {
  DIRECTORY: DurableObjectNamespace<RoomDirectory>;
}
interface SocketInfo {
  id: string;
  openedMs: number;
  joined: boolean;
  windowMs: number;
  messages: number;
  /** Where the socket entered Cloudflare, and that colo's TCP round trip to the player. */
  edgeColo?: string;
  clientTcpRttMs?: number;
}
export class PlayerRoom extends DurableObject<Env> {
  private host?: MatchHost;
  private sockets = new Map<WebSocket, SocketInfo>();
  private byId = new Map<string, WebSocket>();
  private timer?: ReturnType<typeof setTimeout>;
  private nextTickMs = 0;
  private room = "";
  private lastListedMs = 0;
  private directoryDirty = false;
  private directoryPublishing = false;
  private timerLateness = new SampleWindow(TIMER_LATENESS_SAMPLES);
  private probes = 0;
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    for (const socket of ctx.getWebSockets()) {
      try {
        socket.send(
          JSON.stringify({
            type: "room-reset",
            roomEpoch: crypto.randomUUID(),
            reason: "runtime-restart",
          }),
        );
        socket.close(1012, "Room restarted");
      } catch {
        /* Disconnected while the object was evicted. */
      }
    }
  }
  override fetch(request: Request): Response | Promise<Response> {
    // Diagnostics and stats probes must not create a match or replace the listed room code.
    const path = new URL(request.url).pathname;
    if (path === "/ping") return new Response(null, { status: 204 });
    if (path === "/diag") return this.diagnostics();
    if (path === "/probe") return this.probe(request);
    this.room = new URL(request.url).pathname.split("/").at(-1)!;
    if (this.sockets.size >= MAX_PENDING_CONNECTIONS)
      return new Response("Room connection limit", { status: 429 });
    if (!this.host || this.host.disposed) {
      this.host = new MatchHost(
        {
          roomEpoch: crypto.randomUUID(),
          nowMs: Date.now(),
          token: () => crypto.randomUUID() + crypto.randomUUID(),
          seed: crypto.getRandomValues(new Uint32Array(1))[0],
        },
        {
          send: (connection, message) => {
            const socket = this.byId.get(connection);
            if (!socket) return;
            try {
              socket.send(message);
              if (message.startsWith('{"type":"welcome"')) this.sockets.get(socket)!.joined = true;
            } catch {
              this.drop(socket, 1011, "Send failed");
            }
          },
          close: (connection, code, reason) => {
            const socket = this.byId.get(connection);
            if (socket) this.drop(socket, code, reason);
          },
          changed: () => this.publishDirectory(true),
        },
      );
    }
    const pair = new WebSocketPair(),
      socket = pair[1],
      now = Date.now(),
      id = crypto.randomUUID();
    this.ctx.acceptWebSocket(socket);
    this.sockets.set(socket, {
      id,
      openedMs: now,
      joined: false,
      windowMs: now,
      messages: 0,
      edgeColo: request.cf?.colo as string | undefined,
      clientTcpRttMs: request.cf?.clientTcpRtt as number | undefined,
    });
    this.byId.set(id, socket);
    // The stats panel compares this with its stats socket's colo before trusting the edge legs.
    socket.send(JSON.stringify({ type: "edge", colo: request.cf?.colo ?? "unknown" }));
    if (this.timer === undefined) this.schedule();
    return new Response(null, { status: 101, webSocket: pair[0] });
  }
  /**
   * Echo socket for the Worker's stats relay. Replies go through the same event loop as game
   * pongs, so a busy room shows up in the edge-to-room time as it does in game RTT.
   */
  private probe(request: Request): Response {
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket")
      return new Response("WebSocket required", { status: 426 });
    if (this.probes >= MAX_PROBE_SOCKETS) return new Response("Too many probes", { status: 429 });
    const pair = new WebSocketPair(),
      socket = pair[1];
    socket.accept();
    this.probes++;
    let open = true;
    const close = () => {
      if (!open) return;
      open = false;
      this.probes--;
      try {
        socket.close(1000, "Probe closed");
      } catch {
        /* Already closed. */
      }
    };
    socket.addEventListener("message", (event) => {
      if (typeof event.data !== "string" || event.data.length > MAX_PROBE_MESSAGE_BYTES) close();
      else socket.send(event.data);
    });
    socket.addEventListener("close", close);
    socket.addEventListener("error", close);
    return new Response(null, { status: 101, webSocket: pair[0] });
  }
  /** Where this object runs, where its sockets entered Cloudflare, and how late its ticks fire. */
  private async diagnostics(): Promise<Response> {
    const now = Date.now();
    return Response.json({
      colo: await executionColo(),
      match: this.host !== undefined && !this.host.disposed,
      probes: this.probes,
      sockets: [...this.sockets.values()].map((info) => ({
        edgeColo: info.edgeColo ?? "unknown",
        clientTcpRttMs: info.clientTcpRttMs,
        joined: info.joined,
        ageSeconds: Math.round((now - info.openedMs) / 1000),
      })),
      timerLateMs: this.timerLateness.summary(),
    });
  }
  private publishDirectory(force = false): void {
    if (!this.room || !this.host || !this.env.DIRECTORY) return;
    const now = Date.now();
    if (!force && now - this.lastListedMs < DIRECTORY_HEARTBEAT_MS) return;
    this.lastListedMs = now;
    this.directoryDirty = true;
    if (this.directoryPublishing) return;
    this.directoryPublishing = true;
    this.ctx.waitUntil(
      (async () => {
        try {
          // Coalesce lobby churn instead of appending a queue of stale listings.
          while (this.directoryDirty && this.host) {
            this.directoryDirty = false;
            const entry = this.host.directoryEntry(this.room);
            const response = await this.env.DIRECTORY.getByName("rooms").fetch(
              "https://directory/rooms",
              {
                method: "PUT",
                body: JSON.stringify(entry),
              },
            );
            if (!response.ok) throw new Error("Room directory update failed");
          }
        } catch (error) {
          console.error("Room listing unavailable", error);
        } finally {
          this.directoryPublishing = false;
        }
      })(),
    );
  }
  override webSocketMessage(socket: WebSocket, message: string | ArrayBuffer): void {
    const info = this.sockets.get(socket);
    if (!info) return;
    const now = Date.now();
    if (now - info.windowMs >= 1000) {
      info.windowMs = now;
      info.messages = 0;
    }
    if (
      ++info.messages > 65 ||
      typeof message !== "string" ||
      new TextEncoder().encode(message).byteLength > MAX_CLIENT_MESSAGE_BYTES
    ) {
      this.drop(socket, 1008, "Invalid message or rate");
      return;
    }
    this.host?.receive(info.id, message, now);
  }
  override webSocketClose(socket: WebSocket, code: number, reason: string): void {
    this.forget(socket);
    // Hibernating sockets require the server half of the closing handshake.
    // Without it browsers can remain CLOSING until their seat reservation expires.
    try {
      socket.close(code === 1005 || code === 1006 ? 1000 : code, reason);
    } catch {
      /* The transport may already be gone. */
    }
  }
  override webSocketError(socket: WebSocket): void {
    this.drop(socket, 1011, "Socket failed");
  }
  private forget(socket: WebSocket): void {
    const info = this.sockets.get(socket);
    if (!info) return;
    this.sockets.delete(socket);
    this.byId.delete(info.id);
    this.host?.disconnect(info.id, Date.now());
  }
  private drop(socket: WebSocket, code: number, reason: string): void {
    this.forget(socket);
    try {
      socket.close(code, reason);
    } catch {
      /* Already closed. */
    }
  }
  private schedule(): void {
    // Fixed deadlines keep snapshot batches at 20 Hz; chaining a full interval after each
    // callback would add simulation time and timer slop to every gap clients must buffer.
    const now = Date.now();
    // A missed deadline re-anchors one full interval out: an immediate callback would only
    // resend the snapshot just broadcast. FixedStepClock still owes the late ticks.
    this.nextTickMs += HOST_INTERVAL_MS;
    if (this.nextTickMs <= now || this.nextTickMs > now + HOST_INTERVAL_MS) {
      this.nextTickMs = now + HOST_INTERVAL_MS;
    }
    const deadline = this.nextTickMs;
    this.timer = setTimeout(() => {
      this.timer = undefined;
      const now = Date.now();
      // A late timer means the event loop was busy, which also delays pongs and probes.
      this.timerLateness.add(now - deadline);
      for (const [socket, info] of this.sockets)
        if (!info.joined && now - info.openedMs >= JOIN_TIMEOUT_MS)
          this.drop(socket, 1008, "Join timed out");
      try {
        this.host?.advance(now);
        this.publishDirectory();
      } catch (error) {
        console.error("Room simulation failed", error);
        this.host?.dispose("simulation-error");
      }
      if (this.host && !this.host.disposed) this.schedule();
      else {
        this.sockets.clear();
        this.byId.clear();
      }
    }, this.nextTickMs - now);
  }
}
