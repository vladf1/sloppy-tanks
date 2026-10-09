import type { EngineGlue, WireView } from "../engine";

/** Most messages the log keeps. */
const KEPT_MESSAGES = 300;

/** One logged message, readable in the console. */
export interface WireEntry {
  /** `performance.now()` when it arrived or left. */
  at: number;
  direction: "received" | "sent";
  message: unknown;
}

/**
 * A `?debug` room page's log of its socket traffic as JSON, for the console
 * (`sloppy.wire`). Room state arrives as binary frames of differences, which only the
 * engine's decoder can read in order, so each socket gets its own `WireView`; text
 * messages are parsed as they are. Nothing here affects the game: the engine reads the
 * same messages separately.
 */
export class WireLog {
  private readonly entries: WireEntry[] = [];
  private readonly views = new Map<number, WireView>();
  private following = false;

  constructor(private readonly engine: EngineGlue) {}

  received(socket: number, data: string | ArrayBuffer, at: number): void {
    let message: unknown;
    try {
      if (typeof data === "string") {
        message = JSON.parse(data);
      } else {
        let view = this.views.get(socket);
        if (!view) {
          view = new this.engine.WireView();
          this.views.set(socket, view);
        }
        message = JSON.parse(view.json(new Uint8Array(data)));
      }
    } catch (error) {
      message = { unreadable: String(error) };
    }
    this.add({ at, direction: "received", message });
  }

  sent(text: string, at: number): void {
    this.add({ at, direction: "sent", message: JSON.parse(text) as unknown });
  }

  /** The socket closed; its next one starts from a new baseline. */
  closed(socket: number): void {
    this.views.get(socket)?.free();
    this.views.delete(socket);
  }

  /** The console surface, `window.sloppy.wire`. */
  api() {
    return {
      /** The last `count` messages, oldest first. */
      last: (count = 1): WireEntry[] => this.entries.slice(-count),
      /** Log each message as it arrives or leaves; `follow(false)` stops. */
      follow: (on = true): void => {
        this.following = on;
      },
      /** Every kept message (the last 300). */
      all: (): WireEntry[] => [...this.entries],
      clear: (): void => {
        this.entries.length = 0;
      },
    };
  }

  private add(entry: WireEntry): void {
    this.entries.push(entry);
    if (this.entries.length > KEPT_MESSAGES) {
      this.entries.shift();
    }
    if (this.following) {
      console.debug(`wire ${entry.direction}`, entry.message);
    }
  }
}
