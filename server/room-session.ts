import { MatchHost } from "../src/net/match-host";
import { HOST_INTERVAL_MS } from "../src/net/fixed-step-clock";
import { MAX_CLIENT_MESSAGE_BYTES } from "../src/net/protocol";
import type { RoomListing } from "../src/net/room-list";

export const MAX_PENDING_CONNECTIONS = 16;
export const JOIN_TIMEOUT_MS = 5000;
export const DIRECTORY_HEARTBEAT_MS = 20_000;
export const MAX_SOCKET_MESSAGES_PER_SECOND = 65;

/** The part of a WebSocket that a room needs. */
export interface RoomSocket {
  send(text: string): void;
  close(code: number, reason: string): void;
}
/** Seat and socket changes a runtime may log; they never affect room behaviour. */
export type RoomActivity =
  | { type: "created" }
  | { type: "joined"; players: number }
  | { type: "left"; players: number; code?: number }
  | { type: "closed"; players: number; code: number; reason: string };
export interface RoomSessionEvents {
  /** A listing for the public room directory; forced on lobby changes, otherwise a heartbeat. */
  listing(entry: RoomListing): void;
  activity?(event: RoomActivity): void;
  /** The match was disposed and every socket released; the runtime may forget this room. */
  ended?(reason: string, ageMs: number): void;
}
/** One room's state and load since the previous sample. Byte counts are UTF-16 lengths. */
export interface RoomSample {
  room: string;
  mapMode: RoomListing["mapMode"];
  phase: RoomListing["phase"];
  players: number;
  seats: number;
  sockets: number;
  timeLeft: number;
  scores: number[];
  ageSeconds: number;
  tick: number;
  debtMs: number;
  tickAvgMs: number;
  tickMaxMs: number;
  sentBytes: number;
  receivedBytes: number;
}
interface SocketInfo {
  id: string;
  openedMs: number;
  joined: boolean;
  windowMs: number;
  messages: number;
}

/**
 * Socket policy and the 50 ms timer around one MatchHost, independent of the WebSocket
 * library so tests can drive a room with fake sockets and mocked timers.
 */
export class RoomSession<Socket extends RoomSocket = RoomSocket> {
  private host?: MatchHost;
  private hostCreatedMs = 0;
  private sockets = new Map<Socket, SocketInfo>();
  private byId = new Map<string, Socket>();
  private timer?: ReturnType<typeof setTimeout>;
  private nextTickMs = 0;
  private lastListedMs = 0;
  // Load counters since the last sample().
  private sentBytes = 0;
  private receivedBytes = 0;
  private ticks = 0;
  private tickTotalMs = 0;
  private tickMaxMs = 0;
  constructor(
    readonly room: string,
    private readonly events: RoomSessionEvents,
  ) {}
  get connections(): number {
    return this.sockets.size;
  }
  get full(): boolean {
    return this.sockets.size >= MAX_PENDING_CONNECTIONS;
  }
  /** Registers an open socket; false means the caller should refuse it at the room limit. */
  accept(socket: Socket): boolean {
    if (this.full) return false;
    if (!this.host || this.host.disposed) {
      this.host = this.createHost();
      this.hostCreatedMs = Date.now();
      this.events.activity?.({ type: "created" });
    }
    const now = Date.now(),
      id = crypto.randomUUID();
    this.sockets.set(socket, { id, openedMs: now, joined: false, windowMs: now, messages: 0 });
    this.byId.set(id, socket);
    if (this.timer === undefined) this.schedule();
    return true;
  }
  message(socket: Socket, message: string | ArrayBuffer): void {
    const info = this.sockets.get(socket);
    if (!info) return;
    const now = Date.now();
    if (now - info.windowMs >= 1000) {
      info.windowMs = now;
      info.messages = 0;
    }
    const bytes =
      typeof message === "string" ? new TextEncoder().encode(message).byteLength : Infinity;
    if (++info.messages > MAX_SOCKET_MESSAGES_PER_SECOND || bytes > MAX_CLIENT_MESSAGE_BYTES) {
      this.drop(socket, 1008, "Invalid message or rate");
      return;
    }
    this.receivedBytes += bytes;
    this.host?.receive(info.id, message as string, now);
  }
  /** The transport closed; the seat stays reserved for the host's reconnect grace. */
  closed(socket: Socket, code?: number): void {
    const info = this.forget(socket);
    if (info?.joined) this.events.activity?.({ type: "left", players: this.players, code });
  }
  failed(socket: Socket): void {
    this.drop(socket, 1011, "Socket failed");
  }
  /** Ends the match now, telling joined players why, and releases every socket. */
  reset(reason: string): void {
    this.host?.dispose(reason);
    this.stop();
  }
  /** Current state plus load since the previous call; undefined when no match is live. */
  sample(): RoomSample | undefined {
    if (!this.host || this.host.disposed) return undefined;
    const entry = this.host.directoryEntry(this.room);
    const sample: RoomSample = {
      room: this.room,
      mapMode: entry.mapMode,
      phase: entry.phase,
      players: entry.players,
      seats: entry.reserved,
      sockets: this.sockets.size,
      timeLeft: entry.time,
      scores: entry.scores,
      ageSeconds: Math.round((Date.now() - this.hostCreatedMs) / 1000),
      tick: this.host.tick,
      debtMs: this.host.debtMs,
      tickAvgMs: this.ticks ? this.tickTotalMs / this.ticks : 0,
      tickMaxMs: this.tickMaxMs,
      sentBytes: this.sentBytes,
      receivedBytes: this.receivedBytes,
    };
    this.sentBytes = this.receivedBytes = this.ticks = this.tickTotalMs = this.tickMaxMs = 0;
    return sample;
  }
  private get players(): number {
    return this.host?.connections ?? 0;
  }
  private createHost(): MatchHost {
    return new MatchHost(
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
            this.sentBytes += message.length;
            if (message.startsWith('{"type":"welcome"')) {
              this.sockets.get(socket)!.joined = true;
              this.events.activity?.({ type: "joined", players: this.players });
            }
          } catch {
            this.drop(socket, 1011, "Send failed");
          }
        },
        close: (connection, code, reason) => {
          const socket = this.byId.get(connection);
          if (socket) this.drop(socket, code, reason);
        },
        changed: () => this.publishListing(true),
      },
    );
  }
  private publishListing(force = false): void {
    if (!this.host) return;
    const now = Date.now();
    if (!force && now - this.lastListedMs < DIRECTORY_HEARTBEAT_MS) return;
    this.lastListedMs = now;
    this.events.listing(this.host.directoryEntry(this.room));
  }
  private forget(socket: Socket): SocketInfo | undefined {
    const info = this.sockets.get(socket);
    if (!info) return undefined;
    this.sockets.delete(socket);
    this.byId.delete(info.id);
    this.host?.disconnect(info.id, Date.now());
    return info;
  }
  private drop(socket: Socket, code: number, reason: string): void {
    if (this.forget(socket))
      this.events.activity?.({ type: "closed", players: this.players, code, reason });
    try {
      socket.close(code, reason);
    } catch {
      /* Already closed. */
    }
  }
  private stop(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
    // Sockets that never joined have no seat to reset; close them rather than leak them.
    for (const socket of [...this.sockets.keys()]) this.drop(socket, 1012, "Room closed");
    this.events.ended?.(this.host?.disposeReason ?? "closed", Date.now() - this.hostCreatedMs);
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
    this.timer = setTimeout(() => {
      this.timer = undefined;
      const now = Date.now();
      for (const [socket, info] of this.sockets)
        if (!info.joined && now - info.openedMs >= JOIN_TIMEOUT_MS)
          this.drop(socket, 1008, "Join timed out");
      const started = performance.now();
      try {
        this.host?.advance(now);
        this.publishListing();
      } catch (error) {
        console.error("Room simulation failed", error);
        this.host?.dispose("simulation-error");
      }
      const elapsed = performance.now() - started;
      this.ticks++;
      this.tickTotalMs += elapsed;
      this.tickMaxMs = Math.max(this.tickMaxMs, elapsed);
      if (this.host && !this.host.disposed) this.schedule();
      else this.stop();
    }, this.nextTickMs - now);
  }
}
