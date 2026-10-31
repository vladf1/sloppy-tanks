/**
 * A region's pilots: players run by the bot brain (`crates/pilot`) instead of the traffic
 * bots' random driving. The squad picks one room for all of them (a given code, a new room
 * it creates, or the fullest open one), seats them there and keeps them playing: a pilot
 * whose connection gave up joins again a little later, and a trap in the engine restarts
 * every pilot in a new one, each taking back its seat. Platform-free apart from `connect`.
 */

import { MAPS, ROOM_SEATS, randomRoomCode } from "./bot-player";
import { PilotEngineError, type PilotEngine } from "./pilot-engine";
import { PilotSeat, type Connect, type PilotHost } from "./pilot-seat";

export const PILOT_NAME_PREFIX = "pilot-";
export const MISSIONS = /^(skirmish|hunt(:.{1,24})?|(escort|ally):.{1,24})$/i;
export const SKILLS = ["easy", "normal", "hard"] as const;
export const KINDS = ["scout", "balanced", "heavy"] as const;
export const STYLES = [
  "scout",
  "guard",
  "sniper",
  "heavy",
  "minelayer",
  "support",
  "artillery",
] as const;

/** What a region's pilots are told (`crates/pilot/src/orders.rs`). */
export interface PilotBrain {
  mission: string;
  skill: (typeof SKILLS)[number];
  kind: (typeof KINDS)[number];
  style?: (typeof STYLES)[number];
  /** The map of a room the pilots create. */
  map: string;
}

export interface SquadConfig {
  region: string;
  pilots: number;
  /** Join this room. */
  room?: string;
  /** With no room given, create one rather than join the fullest open room. */
  host: boolean;
  brain: PilotBrain;
}

/** A game server as the directory lists it, with the room fields the squad reads. */
export interface DirectoryServer {
  url: string;
  contentVersion: string;
  rooms: { room: string; contentVersion: string; reserved: number; mapMode: string }[];
}

/** A pilot whose connection gave up joins again this long after. */
const REJOIN_MS = 15_000;
/** Connection ends after which the pilot's room is gone for every pilot: pick a room again.
 * A rejection is one too unless another pilot plays there ({@link PilotSquad.roomGone}). */
const ROOM_GONE = new Set(["room-ended", "outdated"]);

function choose<T extends string>(value: unknown, options: readonly T[], label: string): T {
  const found = options.find((option) => option === value);
  if (!found) {
    throw new Error(`${label} must be one of ${options.join(", ")}`);
  }
  return found;
}

/** The brain options of a start request; throws a message for the operator. */
export function readBrain(body: Record<string, unknown>): PilotBrain {
  const mission = String(body.mission ?? "skirmish").trim();
  if (!MISSIONS.test(mission)) {
    throw new Error("mission must be skirmish, hunt, hunt:NAME or escort:NAME");
  }
  return {
    mission,
    skill: choose(body.skill ?? "normal", SKILLS, "skill"),
    kind: choose(body.kind ?? "balanced", KINDS, "kind"),
    style: body.style ? choose(body.style, STYLES, "style") : undefined,
    map: choose(body.map ?? "village", MAPS, "map"),
  };
}

export class PilotSquad {
  readonly seats: PilotSeat[] = [];
  lastError?: string;
  /** The room the pilots play, once chosen, and the server hosting it. */
  room?: string;
  server?: string;
  /** Traps the engine recovered from. */
  restarts = 0;
  private engine?: PilotEngine;
  /** The first pilot makes the room; the others wait until it is seated. */
  private creating = false;
  private readonly endedAt = new Map<PilotSeat, number>();
  private readonly host: PilotHost;

  constructor(
    private readonly config: SquadConfig,
    /** A fresh engine, at the start and after a trap. */
    private readonly createEngine: () => PilotEngine,
    connect: Connect,
    private readonly now: () => number = Date.now,
  ) {
    this.host = {
      engine: () => this.currentEngine(),
      now: this.now,
      connect,
      failed: (error) => this.recover(error),
    };
  }

  /** The engine's content version: servers that report another refuse the pilots. */
  get contentVersion(): string {
    return this.currentEngine().contentVersion;
  }

  /** One frame of every pilot. */
  tick(): void {
    for (const seat of this.seats) {
      if (!seat.handle) {
        continue;
      }
      try {
        seat.tick();
      } catch (error) {
        this.recover(error);
        return;
      }
      if (seat.ended && !this.endedAt.has(seat)) {
        this.endedAt.set(seat, this.now());
      }
    }
  }

  /** Every few seconds: picks the room and starts the pilots that need it. */
  maintain(servers: readonly DirectoryServer[]): void {
    const now = this.now();
    if (!this.room) {
      this.place(servers);
    }
    const room = this.room;
    const server = this.server;
    if (!room || !server) {
      return;
    }
    while (this.seats.length < this.config.pilots) {
      this.seats.push(new PilotSeat(this.setup(this.seats.length, room, server), this.host));
    }
    const maker = this.seats[0];
    if (this.creating && maker.report && maker.report.phase !== "connecting") {
      // The room exists now: nobody creates it again, the maker included.
      this.creating = false;
      for (const seat of this.seats) {
        seat.setup = { ...seat.setup, create: undefined };
      }
    }
    for (const [index, seat] of this.seats.entries()) {
      if (seat.ended) {
        const endedAt = this.endedAt.get(seat) ?? now;
        if (now - endedAt < REJOIN_MS) {
          continue;
        }
        if (this.roomGone(seat)) {
          // The room is over for every pilot: choose again next time.
          this.lastError = `${seat.name}: ${seat.ended.text}`;
          this.reset();
          return;
        }
        // A seat the server may still hold is taken back; the engine forgets one it lost.
        // A pilot the room turned away tries again, as a seat may have come free.
        seat.stop();
        this.endedAt.delete(seat);
      }
      if (seat.handle || (index > 0 && this.creating)) {
        continue;
      }
      try {
        seat.start();
      } catch (error) {
        if (error instanceof PilotEngineError) {
          this.recover(error);
          return;
        }
        seat.lastError = error instanceof Error ? error.message : String(error);
      }
    }
  }

  /** Every pilot leaves its seat (the server frees it at once) and stops. */
  stop(): void {
    for (const seat of this.seats) {
      try {
        seat.leave();
      } catch {
        /* A trapped engine cannot say goodbye; the seat expires on its own. */
      }
      seat.stop();
    }
    this.seats.length = 0;
    this.endedAt.clear();
  }

  summary() {
    return {
      room: this.room ?? null,
      server: this.server ?? null,
      contentVersion: this.engine?.contentVersion ?? null,
      restarts: this.restarts,
      lastError: this.lastError ?? null,
      pilots: this.seats.map((seat) => ({
        name: seat.name,
        room: seat.room,
        connected: !!seat.handle && !seat.ended,
        lastError: seat.lastError ?? null,
        report: seat.report ?? null,
      })),
    };
  }

  private currentEngine(): PilotEngine {
    this.engine ??= this.createEngine();
    return this.engine;
  }

  /** Whether a pilot's connection ended in a way that sends every pilot elsewhere. A room
   * that refuses one pilot while another plays there is full for that one alone. */
  private roomGone(seat: PilotSeat): boolean {
    const cause = seat.ended?.cause;
    if (cause === "rejected") {
      return !this.seats.some(
        (other) =>
          other !== seat &&
          other.handle &&
          !other.ended &&
          other.report !== undefined &&
          other.report.phase !== "connecting",
      );
    }
    return !!cause && ROOM_GONE.has(cause);
  }

  /** The pilots leave the room they were in, to choose one again. */
  private reset(): void {
    this.stop();
    this.room = undefined;
    this.server = undefined;
    this.creating = false;
  }

  /** A trap lost the engine's state: a new engine, and every pilot back in its seat. */
  private recover(error: unknown): void {
    if (!(error instanceof PilotEngineError)) {
      this.lastError = error instanceof Error ? error.message : String(error);
      return;
    }
    this.lastError = error.message;
    this.restarts++;
    for (const seat of this.seats) {
      seat.stop();
    }
    this.engine = undefined;
    for (const seat of this.seats) {
      if (!seat.ended) {
        try {
          seat.start();
        } catch (failure) {
          seat.lastError = failure instanceof Error ? failure.message : String(failure);
        }
      }
    }
  }

  private place(servers: readonly DirectoryServer[]): void {
    const version = this.contentVersion;
    const mismatch = (other: string | undefined) =>
      `The pilot engine plays content ${version} but the server runs ${other ?? "nothing"}: ` +
      "deploy the pilots (bun run pilots:deploy) from the commit the server runs";
    const { room, host } = this.config;
    if (room) {
      const server = servers.find((listed) => listed.rooms.some((entry) => entry.room === room));
      if (!server) {
        this.lastError = `The server directory lists no room ${room}`;
        return;
      }
      if (server.contentVersion !== version) {
        this.lastError = mismatch(server.contentVersion);
        return;
      }
      this.choose(room, server.url, false);
      return;
    }
    const compatible = servers.filter((server) => server.contentVersion === version);
    if (!compatible.length) {
      this.lastError = mismatch(servers[0]?.contentVersion);
      return;
    }
    if (host) {
      // The directory lists the least busy server first.
      this.choose(randomRoomCode(), compatible[0].url, true);
      return;
    }
    // Only a room with a seat for every pilot: the squad plays together.
    const open = compatible
      .flatMap((server) => server.rooms.map((entry) => ({ ...entry, url: server.url })))
      .filter(
        (entry) =>
          entry.contentVersion === version &&
          MAPS.includes(entry.mapMode) &&
          entry.reserved + this.config.pilots <= ROOM_SEATS,
      )
      .sort((a, b) => b.reserved - a.reserved || a.room.localeCompare(b.room));
    if (!open.length) {
      this.lastError =
        `No open room has ${this.config.pilots} free seats: ` +
        "give a room code, or let the pilots create one";
      return;
    }
    this.choose(open[0].room, open[0].url, false);
  }

  private choose(room: string, server: string, creating: boolean): void {
    this.room = room;
    this.server = server;
    this.creating = creating;
    this.lastError = undefined;
  }

  private setup(index: number, room: string, server: string): PilotSeat["setup"] {
    const { brain, region } = this.config;
    return {
      serverUrl: server,
      room,
      name: `${PILOT_NAME_PREFIX}${region}-${index + 1}`,
      orders: {
        mission: brain.mission,
        skill: brain.skill,
        kind: brain.kind,
        personality: brain.style,
      },
      create:
        index === 0 && this.creating
          ? { mapMode: brain.map, difficulty: "normal", humansOnly: false, roundMinutes: 10 }
          : undefined,
      seed: Math.floor(Math.random() * 2 ** 32),
    };
  }
}
