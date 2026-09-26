import {
  CONTENT_VERSION,
  PROTOCOL_VERSION,
  MAX_SERVER_MESSAGE_BYTES,
  type RoomSettings,
} from "./protocol";
import { id, record, string } from "./schema";
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
}
export interface ConnectionEvents {
  message(value: Record<string, unknown>): void;
  status(text: string, connected: boolean): void;
  clearInput(): void;
}
export class Connection {
  roomEpoch = "";
  roundId = 0;
  playerId = "";
  observedTick = 0;
  rtt = 0;
  connected = false;
  private socket?: WebSocket;
  private token?: string;
  private stopped = false;
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
      this.attempt ? "Reconnecting… Your seat is reserved." : "Connecting to room…",
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
        this.fail("Server sent an invalid message.");
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
        this.fail("This seat was opened in another tab.");
        return;
      }
      if (event.code === 1008) {
        this.fail("The server rejected the connection. Reload to join again.");
        return;
      }
      const now = performance.now();
      this.retryStarted ||= now;
      if (now - this.retryStarted >= RECONNECT_WINDOW_MS) {
        this.fail("Connection lost. Your seat may have expired; join again.");
        return;
      }
      this.events.status("Reconnecting… Your tank is bot-driven.", false);
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
          this.fail("Game updated. Reload this page.");
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
        this.events.status(
          message.reset ? "Room restarted. A fresh lobby is ready." : "Connected",
          true,
        );
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
          if (message.code === "seat-expired") {
            this.forgetSeat();
          }
          this.fail(text);
          return;
        }
        this.events.status(text, this.connected);
      } else if (message.type === "room-reset") {
        this.forgetSeat();
        this.fail(
          "Room ended (" + string(80).read(message.reason) + "). Join again for a fresh lobby.",
        );
        return;
      }
      this.events.message(message);
    } catch (error) {
      console.error("Multiplayer protocol error", error);
      this.fail("Game state was incompatible. Reload before joining again.");
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
  private fail(message: string): void {
    this.stop();
    this.events.status(message, false);
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
