import {
  CONTENT_VERSION,
  PROTOCOL_VERSION,
  MAX_SERVER_MESSAGE_BYTES,
  type RoomSettings,
} from "./protocol";
import { id, record, string } from "./schema";
import type { Scenario } from "./scene-codec";
import type { PlayerVehicleKind, Team } from "../game/types";

const RECONNECT_WINDOW_MS = 30_000;
const MAX_BUFFERED_BYTES = 16_384;
/** Dev-only URL parameters that add transport delay (see transport-delay.ts). */
export const TRANSPORT_DELAY_PARAMS = ["latency", "jitter", "stall"];
export interface JoinChoice {
  name: string;
  kind: PlayerVehicleKind;
  team?: Team;
  create?: RoomSettings;
  existingRoom?: boolean;
  scenario?: Scenario;
}
/** Why a connection stopped retrying. Each cause offers its own way back into a game. */
export type EndCause =
  /** The server stayed unreachable for the whole reconnect window. */
  | "lost"
  /** The room itself is gone: a server restart, a time limit or a server fault. */
  | "room-ended"
  /** The server let the seat go; joining again takes a new one. */
  | "seat-expired"
  /** The same seat connected from another tab, which now drives the tank. */
  | "other-tab"
  /** This page and the server run different game versions. */
  | "outdated"
  | "rejected";
export interface ConnectionEnd {
  cause: EndCause;
  text: string;
}
export interface ConnectionEvents {
  message(value: Record<string, unknown>): void;
  /** Progress while (re)connecting; `connected` once the seat is back. */
  status(text: string, connected: boolean): void;
  /** A server answer to the player's last request, such as a full team. */
  notice(text: string): void;
  /** The connection gave up and will not retry on its own. */
  ended(end: ConnectionEnd): void;
  clearInput(): void;
}
/** Server `room-reset` reasons, in words a player can act on. */
const ROOM_END_REASONS = new Map([
  ["server-restart", "The game server restarted, which closed every room."],
  ["expired", "Rooms close after 30 minutes, or after 5 idle minutes between battles."],
  ["overload", "The game server fell behind and had to close this room."],
  ["simulation-error", "The battle hit a server error and the room closed."],
]);
/** Fatal server error codes that are not a plain rejection. */
const FATAL_ERROR_CAUSES = new Map<unknown, EndCause>([
  ["incompatible", "outdated"],
  ["seat-expired", "seat-expired"],
  ["expired", "room-ended"],
  ["room-gone", "room-ended"],
]);
export class Connection {
  roomEpoch = "";
  roundId = 0;
  playerId = "";
  observedTick = 0;
  rtt = 0;
  connected = false;
  /** Set once the connection gives up or leaves; until then it retries on its own. */
  stopped = false;
  private socket?: WebSocket;
  private token?: string;
  private retry?: ReturnType<typeof setTimeout>;
  private heartbeat?: ReturnType<typeof setInterval>;
  private openedAt = 0;
  private retryStarted = 0;
  private attempt = 0;
  private lastMessageAt = 0;
  private choice?: JoinChoice;
  private readonly storageKey: string;
  private delay?: {
    send: (text: string, emit: (text: string) => void) => void;
    receive: (text: string, emit: (text: string) => void) => void;
    clear: () => void;
  };
  constructor(
    readonly url: string,
    readonly room: string,
    private readonly events: ConnectionEvents,
  ) {
    this.storageKey = "sloppy-seat:" + url + ":" + room;
    try {
      const parsed: unknown = JSON.parse(sessionStorage.getItem(this.storageKey) ?? "null");
      if (parsed) {
        const saved = record(parsed);
        this.token = string(128, 16).read(saved.token);
        this.roomEpoch = string(128, 1).read(saved.roomEpoch);
      }
    } catch {
      /* Storage is optional. */
    }
  }
  async connect(choice: JoinChoice): Promise<void> {
    this.stop();
    this.choice = choice;
    this.stopped = false;
    this.retryStarted = 0;
    this.attempt = 0;
    const params = new URLSearchParams(location.search);
    if (import.meta.env.DEV && TRANSPORT_DELAY_PARAMS.some((key) => params.has(key))) {
      const { transportDelay } = await import("./transport-delay");
      this.delay = transportDelay(params);
    }
    this.open();
  }
  private open(): void {
    if (this.stopped || !this.choice) {
      return;
    }
    this.events.status(
      this.attempt
        ? "Still trying to reach the game server. Your seat is held for 30 seconds."
        : "Connecting to the room…",
      false,
    );
    const socket = new WebSocket(this.url.replace(/\/$/, "") + "/room/" + this.room);
    this.socket = socket;
    this.openedAt = this.lastMessageAt = performance.now();
    const active = () => this.socket === socket && !this.stopped;
    socket.onopen = () => {
      if (active()) {
        this.raw({
          type: "join",
          version: PROTOCOL_VERSION,
          contentVersion: CONTENT_VERSION,
          ...this.choice,
          token: this.token,
          roomEpoch: this.roomEpoch || undefined,
        });
      }
    };
    socket.onmessage = (event) => {
      if (!active()) {
        return;
      }
      if (typeof event.data !== "string" || event.data.length > MAX_SERVER_MESSAGE_BYTES) {
        this.fail("rejected", "The server sent a message this page can't read.");
        return;
      }
      const receive = (text: string) => {
        if (active()) {
          this.receive(text);
        }
      };
      if (this.delay) {
        this.delay.receive(event.data, receive);
      } else {
        receive(event.data);
      }
    };
    socket.onclose = (event) => {
      if (!active()) {
        return;
      }
      this.connected = false;
      this.delay?.clear();
      this.events.clearInput();
      clearInterval(this.heartbeat);
      if (event.code === 4001) {
        this.fail("other-tab", "Your seat is now playing in another tab or window.");
        return;
      }
      if (event.code === 1008) {
        this.fail("rejected", "The server closed the connection.");
        return;
      }
      const now = performance.now();
      this.retryStarted ||= now;
      if (now - this.retryStarted >= RECONNECT_WINDOW_MS) {
        this.fail(
          "lost",
          "The game server hasn't answered for 30 seconds, so your seat may be gone.",
        );
        return;
      }
      this.events.status("Reconnecting…", false);
      this.retry = setTimeout(
        () => {
          this.attempt++;
          this.open();
        },
        Math.min(5000, 500 * 2 ** this.attempt),
      );
    };
    socket.onerror = () => {
      /* onclose owns retry; browsers hide failed-upgrade details. */
    };
    clearInterval(this.heartbeat);
    this.heartbeat = setInterval(() => {
      if (!active()) {
        return;
      }
      const now = performance.now();
      if ((!this.connected && now - this.openedAt > 10_000) || now - this.lastMessageAt > 10_000) {
        socket.close();
        return;
      }
      if (this.connected) {
        this.send("ping", { t: Math.round(now), observedTick: this.observedTick });
      }
    }, 1000);
  }
  private receive(text: string): void {
    try {
      const message = record(JSON.parse(text));
      this.lastMessageAt = performance.now();
      if (message.type === "welcome") {
        if (message.version !== PROTOCOL_VERSION || message.contentVersion !== CONTENT_VERSION) {
          this.fail("outdated", "Sloppy Tanks was updated. Reload to get the new version.");
          return;
        }
        this.roomEpoch = string(128, 1).read(message.roomEpoch);
        this.playerId = string(128, 1).read(message.playerId);
        this.token = string(128, 16).read(message.token);
        this.connected = true;
        this.retryStarted = 0;
        this.attempt = 0;
        this.observedTick = 0;
        this.events.clearInput();
        try {
          sessionStorage.setItem(
            this.storageKey,
            JSON.stringify({ token: this.token, roomEpoch: this.roomEpoch }),
          );
        } catch {
          /* Session can continue without storage. */
        }
        this.events.status("Connected", true);
        if (message.reset) {
          this.events.notice("The server restarted the room. This is a fresh lobby.");
        }
      } else if (message.type === "pong") {
        const sent = typeof message.t === "number" ? message.t : NaN;
        if (!Number.isFinite(sent)) {
          throw new Error("Invalid pong");
        }
        this.rtt = Math.max(0, performance.now() - sent);
        id.read(message.tick);
      } else if (message.type === "error") {
        const text = string(200).read(message.message);
        if (message.fatal) {
          const cause = FATAL_ERROR_CAUSES.get(message.code) ?? "rejected";
          if (cause === "seat-expired") {
            this.forgetSeat();
          }
          this.fail(cause, text);
          return;
        }
        this.events.notice(text);
      } else if (message.type === "room-reset") {
        this.forgetSeat();
        const reason = string(80).read(message.reason);
        this.fail("room-ended", ROOM_END_REASONS.get(reason) ?? "The room was closed.");
        return;
      }
      this.events.message(message);
    } catch (error) {
      console.error("Multiplayer protocol error", error);
      this.fail(
        "outdated",
        "This page couldn't read the game state. Reload to get the latest version.",
      );
    }
  }
  send(type: string, fields: object = {}): boolean {
    return this.raw({ type, roundId: this.roundId, ...fields });
  }
  settings(value: RoomSettings): void {
    this.send("settings", value);
  }
  private raw(value: object): boolean {
    const socket = this.socket;
    if (socket?.readyState !== WebSocket.OPEN || this.stopped) {
      return false;
    }
    if (socket.bufferedAmount > MAX_BUFFERED_BYTES) {
      socket.close();
      return false;
    }
    const text = JSON.stringify(value);
    const emit = (text: string) => {
      if (this.socket === socket && socket.readyState === WebSocket.OPEN && !this.stopped) {
        socket.send(text);
      }
    };
    if (this.delay) {
      this.delay.send(text, emit);
    } else {
      emit(text);
    }
    return true;
  }
  private forgetSeat(): void {
    this.token = undefined;
    this.roomEpoch = "";
    try {
      sessionStorage.removeItem(this.storageKey);
    } catch {
      /* Optional storage. */
    }
  }
  private fail(cause: EndCause, text: string): void {
    this.stop();
    this.events.ended({ cause, text });
  }
  stop(): void {
    this.stopped = true;
    this.connected = false;
    clearTimeout(this.retry);
    clearInterval(this.heartbeat);
    this.delay?.clear();
    this.events.clearInput();
    this.socket?.close();
    this.socket = undefined;
  }
  leave(): void {
    this.send("leave");
    this.forgetSeat();
    this.stop();
  }
}
