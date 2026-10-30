/**
 * One pilot's seat in a swarm: its handle in the {@link PilotEngine} and the sockets the
 * engine asks for. The engine says what to do (`open`, `send`, `close`); the seat does it
 * through `connect` and reports the sockets' events back. Platform-free: the Worker opens
 * sockets with a fetch upgrade, `tests/pilots.test.ts` with `ws`.
 */

import type { PilotAction, PilotEngine, PilotReport, PilotSetup, SavedSeat } from "./pilot-engine";

/** A connected WebSocket, as far as a seat uses one. */
export interface PilotSocket {
  send(text: string): void;
  close(code?: number, reason?: string): void;
}

/** What a socket reports once connected. */
export interface PilotSocketEvents {
  message(data: string | ArrayBuffer | Uint8Array): void;
  close(code: number): void;
}

/** Opens `url`; resolves once the socket is open, or rejects. */
export type Connect = (url: string, events: PilotSocketEvents) => Promise<PilotSocket>;

/** What every seat of a swarm shares. */
export interface PilotHost {
  /** The current engine; the swarm replaces it after a trap. */
  engine(): PilotEngine;
  now(): number;
  connect: Connect;
  /** An engine call failed inside a socket event (a trap: the swarm starts a new engine). */
  failed(error: unknown): void;
}

/** A browser's code for a socket that closed without a close frame. */
const ABNORMAL_CLOSE = 1006;

export class PilotSeat {
  /** The pilot's handle in the current engine; 0 while it has none. */
  handle = 0;
  /** The seat to take back when the pilot starts again (a new engine, a restart). */
  savedSeat?: SavedSeat;
  /** Why the pilot's connection gave up, once it has. */
  ended?: { cause: string; text: string };
  lastError?: string;
  report?: PilotReport;
  private readonly sockets = new Map<number, PilotSocket>();
  /** Sockets asked for and not yet open. */
  private readonly opening = new Set<number>();

  constructor(
    /** What the pilot starts with; a room's maker stops creating once the room exists. */
    public setup: Omit<PilotSetup, "savedSeat">,
    private readonly host: PilotHost,
  ) {}

  get name(): string {
    return this.setup.name;
  }

  get room(): string {
    return this.setup.room;
  }

  /** Starts the pilot in the current engine, taking back its saved seat if any. */
  start(): void {
    this.stop();
    this.ended = undefined;
    this.handle = this.host
      .engine()
      .create({ ...this.setup, savedSeat: this.savedSeat }, this.host.now());
  }

  /** One frame. */
  tick(): void {
    if (this.handle) {
      this.perform(this.host.engine().tick(this.handle, this.host.now()));
      this.report = this.host.engine().report(this.handle, this.host.now()) ?? this.report;
    }
  }

  /** Gives the seat up: `leave` goes out, then the socket closes. */
  leave(): void {
    if (this.handle) {
      this.perform(this.host.engine().leave(this.handle, this.host.now()));
      this.savedSeat = undefined;
    }
  }

  /** Forgets the pilot and its sockets, as after a trap or at the end of a run. */
  stop(): void {
    this.close();
    if (this.handle) {
      try {
        this.host.engine().remove(this.handle);
      } catch {
        /* A trapped engine is being replaced anyway. */
      }
    }
    this.handle = 0;
  }

  private close(): void {
    for (const socket of this.sockets.values()) {
      try {
        socket.close(1000, "Pilot stopped");
      } catch {
        /* Already closed. */
      }
    }
    this.sockets.clear();
    this.opening.clear();
  }

  private perform(step: { actions: PilotAction[]; ended: PilotSeat["ended"] | null }): void {
    for (const action of step.actions) {
      switch (action.type) {
        case "open":
          this.open(action.socket, action.url);
          break;
        case "send":
          this.sockets.get(action.socket)?.send(action.text);
          break;
        case "close": {
          const socket = this.sockets.get(action.socket);
          this.sockets.delete(action.socket);
          this.opening.delete(action.socket);
          try {
            socket?.close(1000, "Pilot closed");
          } catch {
            /* Already closed. */
          }
          // The client reconnects once it hears the close; a Worker socket closed from
          // this side may never report it.
          this.host.engine().closed(this.handle, action.socket, 1000, this.host.now());
          break;
        }
        case "saveSeat":
          this.savedSeat = { token: action.token, roomEpoch: action.roomEpoch };
          break;
        case "forgetSeat":
          this.savedSeat = undefined;
          break;
      }
    }
    if (step.ended) {
      this.ended = step.ended;
    }
  }

  private open(socket: number, url: string): void {
    const handle = this.handle;
    // Events of a socket this seat no longer holds (closed, or a restarted pilot's) are
    // dropped; the engine also ignores ids that are not its current socket.
    const current = () =>
      this.handle === handle && (this.sockets.has(socket) || this.opening.has(socket));
    this.opening.add(socket);
    const report = (call: () => void) => {
      try {
        call();
      } catch (error) {
        this.host.failed(error);
      }
    };
    this.host
      .connect(url, {
        message: (data) => {
          if (current()) {
            report(() => this.host.engine().message(handle, socket, data, this.host.now()));
          }
        },
        close: (code) => {
          if (current()) {
            this.sockets.delete(socket);
            this.opening.delete(socket);
            report(() => this.host.engine().closed(handle, socket, code, this.host.now()));
          }
        },
      })
      .then(
        (opened) => {
          if (this.handle !== handle || !this.opening.delete(socket)) {
            opened.close(1000, "Pilot stopped");
            return;
          }
          this.sockets.set(socket, opened);
          report(() => this.host.engine().opened(handle, socket, this.host.now()));
        },
        (error: unknown) => {
          this.lastError = error instanceof Error ? error.message : String(error);
          if (this.handle === handle && this.opening.delete(socket)) {
            report(() =>
              this.host.engine().closed(handle, socket, ABNORMAL_CLOSE, this.host.now()),
            );
          }
        },
      );
  }
}
