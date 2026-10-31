/**
 * The pilot engine: `crates/pilot` as Wasm (built by `scripts/build-pilot.mjs` into
 * `bots/generated/`), every pilot of a swarm in one instance behind the plain exports of
 * `crates/pilot/src/abi.rs`. The module imports nothing; text and bytes pass through one
 * staging buffer each way. Platform-free, so the Worker and `tests/pilots.test.ts` share it.
 */

interface Exports {
  memory: WebAssembly.Memory;
  pilot_start(): number;
  pilot_panic(): number;
  pilot_input(length: number): number;
  pilot_output(): number;
  pilot_output_length(): number;
  pilot_create(length: number, nowMs: number): number;
  pilot_opened(handle: number, socket: number, nowMs: number): void;
  pilot_text(handle: number, socket: number, length: number, nowMs: number): void;
  pilot_binary(handle: number, socket: number, length: number, nowMs: number): void;
  pilot_closed(handle: number, socket: number, code: number, nowMs: number): void;
  pilot_tick(handle: number, nowMs: number): number;
  pilot_leave(handle: number, nowMs: number): number;
  pilot_report(handle: number, nowMs: number): number;
  pilot_remove(handle: number): void;
}

/** `sloppy_core::net::client::ClientAction` as JSON, with the pilot's socket ids. */
export type PilotAction =
  | { type: "open"; socket: number; url: string }
  | { type: "send"; socket: number; text: string }
  | { type: "close"; socket: number }
  | { type: "saveSeat"; token: string; roomEpoch: string }
  | { type: "forgetSeat" };

export interface PilotStep {
  actions: PilotAction[];
  /** Why the pilot's connection gave up (it no longer retries), once it has. */
  ended: { cause: string; text: string } | null;
}

export interface SavedSeat {
  token: string;
  roomEpoch: string;
}

/** What a pilot is told to do (`crates/pilot/src/orders.rs`). */
export interface PilotOrders {
  /** `skirmish`, `hunt`, `hunt:NAME` or `escort:NAME`. */
  mission: string;
  skill: "easy" | "normal" | "hard";
  kind: "scout" | "balanced" | "heavy";
  /** A bot role (`scout`, `guard`, `sniper`, …); by default the chassis' own. */
  personality?: string;
}

/** A pilot's setup (`crates/pilot/src/fleet.rs`). */
export interface PilotSetup {
  serverUrl: string;
  room: string;
  name: string;
  orders: PilotOrders;
  create?: { mapMode: string; difficulty: string; humansOnly: boolean; roundMinutes: number };
  savedSeat?: SavedSeat;
  seed: number;
}

/** A mean, 95th percentile and count of one reading. */
export interface Reading {
  mean: number;
  p95: number;
  count: number;
}

/** A pilot's report: `PilotReport` in `crates/pilot/src/report.rs`, field for field. */
export interface PilotReport {
  name: string;
  room: string;
  mission: string;
  /** `connecting`, `lobby`, `playing`, `results`, `ended` or `left`. */
  phase: string;
  team: number | null;
  host: boolean;
  status: string;
  notice: string | null;
  ended: string | null;
  endedText: string | null;
  scoreKills: number;
  scoreDeaths: number;
  tally: {
    rounds: number;
    wins: number;
    losses: number;
    hits: number;
    kills: number;
    hitsTaken: number;
    deaths: number;
    playerHits: number;
    playerHitsTaken: number;
    triggerPulls: number;
    teamChanges: number;
  };
  feel: {
    rttMs: number;
    fireToFlashMs: Reading;
    fireToReportMs: Reading;
    viewDelayMs: Reading;
    hitOffsetM: Reading;
    incomingOffsetM: Reading;
    unseenIncoming: number;
    predictedShots: number;
    confirmedShots: number;
    unconfirmedShots: number;
    unpredictedShots: number;
    phantomStrikes: number;
    phantomIncoming: number;
    corrections: number;
    correctionP95M: number;
    correctionMPerS: number;
    predictionLeadMs: number;
    lateBatches: number;
    longestBatchGapMs: number;
  };
}

/** A trap inside the engine: its state is lost and a new engine must take over. */
export class PilotEngineError extends Error {}

export class PilotEngine {
  readonly protocol: number;
  readonly contentVersion: string;
  private readonly exports: Exports;
  private readonly encoder = new TextEncoder();
  private readonly decoder = new TextDecoder();

  constructor(module: WebAssembly.Module) {
    const instance = new WebAssembly.Instance(module, {});
    this.exports = instance.exports as unknown as Exports;
    const versions = JSON.parse(this.answer(() => this.exports.pilot_start())) as {
      protocol: number;
      contentVersion: string;
    };
    this.protocol = versions.protocol;
    this.contentVersion = versions.contentVersion;
  }

  /** A new pilot; its first socket opens on its first tick. */
  create(setup: PilotSetup, nowMs: number): number {
    const length = this.write(this.encoder.encode(JSON.stringify(setup)));
    const handle = this.call(() => this.exports.pilot_create(length, nowMs));
    if (!handle) {
      throw new Error(this.read(this.exports.pilot_output_length()));
    }
    return handle;
  }

  opened(handle: number, socket: number, nowMs: number): void {
    this.call(() => this.exports.pilot_opened(handle, socket, nowMs));
  }

  /** A text or binary frame from the pilot's socket. */
  message(handle: number, socket: number, data: string | ArrayBuffer | Uint8Array, nowMs: number) {
    if (typeof data === "string") {
      const length = this.write(this.encoder.encode(data));
      this.call(() => this.exports.pilot_text(handle, socket, length, nowMs));
      return;
    }
    const length = this.write(data instanceof Uint8Array ? data : new Uint8Array(data));
    this.call(() => this.exports.pilot_binary(handle, socket, length, nowMs));
  }

  closed(handle: number, socket: number, code: number, nowMs: number): void {
    this.call(() => this.exports.pilot_closed(handle, socket, code, nowMs));
  }

  /** One frame of a pilot, and what to do now. */
  tick(handle: number, nowMs: number): PilotStep {
    return JSON.parse(this.answer(() => this.exports.pilot_tick(handle, nowMs))) as PilotStep;
  }

  /** Gives the pilot's seat up; the actions send `leave` and close its socket. */
  leave(handle: number, nowMs: number): PilotStep {
    return JSON.parse(this.answer(() => this.exports.pilot_leave(handle, nowMs))) as PilotStep;
  }

  report(handle: number, nowMs: number): PilotReport | null {
    return JSON.parse(
      this.answer(() => this.exports.pilot_report(handle, nowMs)),
    ) as PilotReport | null;
  }

  remove(handle: number): void {
    this.call(() => this.exports.pilot_remove(handle));
  }

  /** Copies `bytes` into the engine's input buffer; their length. */
  private write(bytes: Uint8Array): number {
    const pointer = this.call(() => this.exports.pilot_input(bytes.length));
    new Uint8Array(this.exports.memory.buffer, pointer, bytes.length).set(bytes);
    return bytes.length;
  }

  /** `length` bytes of the engine's last answer, as text. Memory may have grown, so the
   * view is taken afresh. */
  private read(length: number): string {
    const pointer = this.exports.pilot_output();
    return this.decoder.decode(new Uint8Array(this.exports.memory.buffer, pointer, length));
  }

  private answer(call: () => number): string {
    return this.read(this.call(call));
  }

  /** Runs an export; a trap (a Rust panic) becomes a {@link PilotEngineError} naming it. */
  private call<T>(call: () => T): T {
    try {
      return call();
    } catch (error) {
      if (error instanceof WebAssembly.RuntimeError) {
        let panic = "";
        try {
          panic = this.read(this.exports.pilot_panic());
        } catch {
          /* The engine is past reading. */
        }
        throw new PilotEngineError(`Pilot engine trapped: ${panic || error.message}`);
      }
      throw error;
    }
  }
}
