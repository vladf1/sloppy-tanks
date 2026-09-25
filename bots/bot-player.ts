/**
 * A traffic-test player that speaks the public room protocol like a browser tab but never
 * reads the map: it drives in random directions, sweeps its turret and fires at nothing.
 * Platform-free so the same class runs in the Durable Object and against MatchHost in tests.
 */

export const BOT_NAME_PREFIX = "bot-";
export const ROOM_SEATS = 8;
const ROOM_ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const ROOM_CODE_LENGTH = 8;
// Matches the browser's InputCadence: 20 Hz while driving or firing, 1 Hz refresh when idle.
const ACTIVE_INPUT_MS = 50;
const IDLE_INPUT_MS = 1000;
const PING_MS = 1000;
// The server rate-limits full-state baselines to two per second; resume requests one.
const RESUME_MS = 1000;
const HOST_START_DELAY_MS = 15_000;
const MIN_MANEUVER_MS = 400;
const MAX_MANEUVER_MS = 3000;
const PAUSE_CHANCE = 0.15;
const FIRE_CHANCE = 0.6;
const MINE_CHANCE = 0.08;
const AMMO_CHANCE = 0.08;
const MAX_TURRET_SPEED = 2.5;
const CREATED_ROUND_MINUTES = 5;
const MAPS = ["village", "harbor", "quarry"] as const;
const KINDS = ["scout", "balanced", "heavy"] as const;
const WEAPONS = ["standard", "spread", "rocket", "ricochet", "piercing"] as const;
const WIRE_SCALE = 1000;

export interface ServerInfo {
  version: number;
  contentVersion: string;
}
/** The subset of a directory listing the bots use to choose rooms. */
export interface RoomListingSummary {
  room: string;
  contentVersion: string;
  reserved: number;
}
export interface BotStats {
  bytesIn: number;
  bytesOut: number;
  messagesIn: number;
  messagesOut: number;
  inputs: number;
  rounds: number;
  joins: number;
  rttMs?: number;
}
type Random = () => number;
type Action = { type: "mine" } | { type: "ammo"; weapon: (typeof WEAPONS)[number] };

export function randomRoomCode(random: Random = Math.random): string {
  let code = "";
  for (let i = 0; i < ROOM_CODE_LENGTH; i++) {
    code += ROOM_ALPHABET[Math.floor(random() * ROOM_ALPHABET.length)];
  }
  return code;
}

/**
 * Free seats per compatible listed room, most-occupied first so bots join rooms with people
 * before empty ones. `pending` counts this swarm's bots already heading to a room but not
 * yet visible in its reserved-seat count.
 */
export function openSeats(
  rooms: readonly RoomListingSummary[],
  contentVersion: string,
  pending: ReadonlyMap<string, number>,
  blocked: ReadonlySet<string>,
): { room: string; free: number }[] {
  return rooms
    .filter((room) => room.contentVersion === contentVersion && !blocked.has(room.room))
    .map((room) => ({
      room: room.room,
      free: ROOM_SEATS - room.reserved - (pending.get(room.room) ?? 0),
    }))
    .filter((room) => room.free > 0)
    .sort((a, b) => a.free - b.free || a.room.localeCompare(b.room));
}

function wire(value: number): number {
  return Math.round(value * WIRE_SCALE) / WIRE_SCALE || 0;
}
function pick<T>(items: readonly T[], random: Random): T {
  return items[Math.floor(random() * items.length)];
}
function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Expected object");
  }
  return value as Record<string, unknown>;
}

export class BotPlayer {
  readonly stats: BotStats = {
    bytesIn: 0,
    bytesOut: 0,
    messagesIn: 0,
    messagesOut: 0,
    inputs: 0,
    rounds: 0,
    joins: 0,
  };
  playerId = "";
  hostId = "";
  phase: "joining" | "lobby" | "playing" | "results" = "joining";
  roundId = 0;
  /** Seat credential kept only to reclaim the same seat after a transport drop. */
  token?: string;
  roomEpoch?: string;
  lastError?: string;
  /** Whether the server asked the client not to rejoin this room. */
  fatal = false;
  private send?: (text: string) => void;
  private observedTick = 0;
  private seq = 0;
  private control?: { tankId: number; controlEpoch: number; driver: string };
  private phaseSinceMs = 0;
  private lastInputMs = -Infinity;
  private lastPingMs = -Infinity;
  private lastResumeMs = -Infinity;
  private maneuverUntilMs = 0;
  private moveX = 0;
  private moveZ = 0;
  private fire = false;
  private aim = 0;
  private turretSpeed = 0;
  private lastUpdateMs = 0;
  private actions: Action[] = [];

  constructor(
    readonly name: string,
    private readonly random: Random = Math.random,
  ) {}

  /** Starts a session on an open socket. `create` makes a fresh room and starts its first round. */
  join(send: (text: string) => void, server: ServerInfo, nowMs: number, create = false): void {
    this.send = send;
    this.phase = "joining";
    this.phaseSinceMs = nowMs;
    this.control = undefined;
    this.observedTick = 0;
    this.fatal = false;
    this.stats.joins++;
    this.transmit({
      type: "join",
      version: server.version,
      contentVersion: server.contentVersion,
      name: this.name,
      kind: pick(KINDS, this.random),
      token: this.token,
      roomEpoch: this.token ? this.roomEpoch : undefined,
      create: create
        ? {
            mapMode: pick(MAPS, this.random),
            difficulty: "normal",
            humansOnly: false,
            roundMinutes: CREATED_ROUND_MINUTES,
          }
        : undefined,
    });
  }

  /** Politely frees the seat instead of leaving a 30-second disconnected reservation. */
  leave(): void {
    this.message("leave");
    this.forgetSeat();
  }

  disconnected(): void {
    this.send = undefined;
    this.control = undefined;
    if (this.fatal) {
      this.forgetSeat();
    }
  }

  receive(text: string, nowMs: number): void {
    this.stats.bytesIn += text.length;
    this.stats.messagesIn++;
    const message = record(JSON.parse(text));
    switch (message.type) {
      case "welcome":
        this.playerId = String(message.playerId);
        this.hostId = String(message.hostId);
        this.token = String(message.token);
        this.roomEpoch = String(message.roomEpoch);
        break;
      case "lobby": {
        const roundId = Number(message.roundId);
        if (roundId !== this.roundId) {
          // Ticks restart with every round; an old observed tick would be rejected as future.
          this.roundId = roundId;
          this.observedTick = 0;
          this.control = undefined;
        }
        const phase = message.phase as BotPlayer["phase"];
        if (phase !== this.phase) {
          this.phase = phase;
          this.phaseSinceMs = nowMs;
          if (phase === "playing") {
            this.stats.rounds++;
          }
        }
        this.hostId = String(message.hostId);
        break;
      }
      case "control":
        if (message.roundId === this.roundId) {
          this.control = {
            tankId: Number(message.tankId),
            controlEpoch: Number(message.controlEpoch),
            driver: String(message.driver),
          };
          if (this.control.driver !== "human" && nowMs - this.lastResumeMs >= RESUME_MS) {
            // A dead tank stops accepting input and the server hands it to its AI; take it back.
            this.lastResumeMs = nowMs;
            this.message("resume");
          }
        }
        break;
      case "full":
        if (message.roundId === this.roundId) {
          this.observe(Number(message.tick));
        }
        break;
      case "snapshot": {
        const snapshots = message.snapshots;
        if (message.roundId === this.roundId && Array.isArray(snapshots) && snapshots.length) {
          this.observe(Number(record(snapshots.at(-1)).tick));
        }
        break;
      }
      case "pong":
        this.stats.rttMs = Math.max(0, nowMs - Number(message.t));
        break;
      case "error":
        this.lastError = String(message.code);
        this.fatal ||= message.fatal === true;
        break;
      case "room-reset":
        this.lastError = "room-reset: " + String(message.reason);
        this.fatal = true;
        break;
    }
  }

  /** Call often (every few tens of ms); it rate-limits its own traffic. */
  update(nowMs: number): void {
    if (!this.send || this.phase === "joining") {
      return;
    }
    if (nowMs - this.lastPingMs >= PING_MS) {
      this.lastPingMs = nowMs;
      this.message("ping", { t: nowMs, observedTick: this.observedTick });
    }
    if (
      this.hostId === this.playerId &&
      this.phase !== "playing" &&
      nowMs - this.phaseSinceMs >= HOST_START_DELAY_MS
    ) {
      // A bot that inherits the host seat would otherwise strand people in the lobby.
      this.phaseSinceMs = nowMs;
      this.message("start");
    }
    const elapsed = Math.min(0.25, Math.max(0, (nowMs - this.lastUpdateMs) / 1000));
    this.lastUpdateMs = nowMs;
    if (this.phase !== "playing" || this.control?.driver !== "human") {
      return;
    }
    if (nowMs >= this.maneuverUntilMs) {
      this.maneuver(nowMs);
    }
    this.aim = Math.atan2(
      Math.sin(this.aim + this.turretSpeed * elapsed),
      Math.cos(this.aim + this.turretSpeed * elapsed),
    );
    const active = this.moveX !== 0 || this.moveZ !== 0 || this.fire || this.actions.length;
    if (nowMs - this.lastInputMs < (active ? ACTIVE_INPUT_MS : IDLE_INPUT_MS)) {
      return;
    }
    this.lastInputMs = nowMs;
    this.stats.inputs++;
    this.message("input", {
      controlEpoch: this.control.controlEpoch,
      seq: ++this.seq,
      observedTick: this.observedTick,
      moveX: wire(this.moveX),
      moveZ: wire(this.moveZ),
      aim: { angle: wire(this.aim) },
      fire: this.fire || undefined,
      actions: this.actions.length ? this.actions.splice(0) : undefined,
    });
  }

  private maneuver(nowMs: number): void {
    const random = this.random;
    this.maneuverUntilMs = nowMs + MIN_MANEUVER_MS + random() * (MAX_MANEUVER_MS - MIN_MANEUVER_MS);
    const heading = random() * Math.PI * 2;
    const moving = random() >= PAUSE_CHANCE;
    this.moveX = moving ? Math.sin(heading) : 0;
    this.moveZ = moving ? Math.cos(heading) : 0;
    this.fire = random() < FIRE_CHANCE;
    this.turretSpeed = (random() * 2 - 1) * MAX_TURRET_SPEED;
    if (random() < MINE_CHANCE) {
      this.actions.push({ type: "mine" });
    }
    if (random() < AMMO_CHANCE) {
      this.actions.push({ type: "ammo", weapon: pick(WEAPONS, random) });
    }
  }

  private observe(tick: number): void {
    if (Number.isSafeInteger(tick) && tick > this.observedTick) {
      this.observedTick = tick;
    }
  }

  private message(type: string, fields: Record<string, unknown> = {}): void {
    this.transmit({ type, roundId: this.roundId, ...fields });
  }

  private transmit(value: Record<string, unknown>): void {
    if (!this.send) {
      return;
    }
    const text = JSON.stringify(value);
    this.stats.bytesOut += text.length;
    this.stats.messagesOut++;
    this.send(text);
  }

  private forgetSeat(): void {
    this.token = undefined;
    this.roomEpoch = undefined;
  }
}
