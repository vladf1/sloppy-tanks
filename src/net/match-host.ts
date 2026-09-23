import { STEP } from "../game/data";
import type { Simulation } from "../game/simulation";
import type { Tank, VehicleCommand } from "../game/types";
import { FixedStepClock } from "./fixed-step-clock";
import {
  createMultiplayerSimulation,
  claimPlayerTank,
  releasePlayerTank,
  MAX_PLAYERS,
  TEAM_SLOTS,
} from "./multiplayer-simulation";
import { PlayerControls } from "./player-controls";
import { captureScene, eventReader, rounded, shotReader, playerKind, team } from "./scene-codec";
import { StateStream, type TimedEvent, type ShotTrace, type Snapshot } from "./replication";
import {
  CONTENT_VERSION,
  PROTOCOL_VERSION,
  MAX_CLIENT_MESSAGE_BYTES,
  EMPTY_GRACE_MS,
  MAX_ROOM_MS,
  ROOM_IDLE_MS,
  DEFAULT_ROUND_MINUTES,
  joinReader,
  settingsReader,
  type Player,
  type Lobby,
  type RoomSettings,
  type Control,
} from "./protocol";
import { id, number, record } from "./schema";
import type { RoomListing } from "./room-list";

const MAX_MESSAGES_PER_SECOND = 60;
const CLIENT_TIMEOUT_MS = 65_000;
const MAX_UNACKNOWLEDGED_TICKS = 180;
const MAX_RESYNCS_PER_SECOND = 2;
const MAX_PARTICIPANTS = 128;
interface Seat {
  player: Player;
  token: string;
  connection?: string;
  disconnectedMs?: number;
  controls?: PlayerControls;
  controlKey?: string;
  suspended: boolean;
}
interface Client {
  seat: Seat;
  lastSeenMs: number;
  observedTick: number;
  windowMs: number;
  count: number;
  fullWindowMs: number;
  fullCount: number;
}
export interface HostTransport {
  send(connection: string, message: string): void;
  close(connection: string, code: number, reason: string): void;
  changed?(): void;
}
export interface HostOptions {
  roomEpoch: string;
  nowMs: number;
  token: () => string;
  seed?: number;
  contentVersion?: string;
}

/** Owns simulation and room policy. Call advance from a runtime's 50 ms timer; no platform APIs here. */
export class MatchHost {
  simulation?: Simulation;
  settings: RoomSettings = {
    mapMode: "village",
    difficulty: "normal",
    humansOnly: false,
    roundMinutes: DEFAULT_ROUND_MINUTES,
  };
  phase: Lobby["phase"] = "lobby";
  roundId = 0;
  hostId = "";
  disposed = false;
  private seats: Seat[] = [];
  private clients = new Map<string, Client>();
  private participants = new Map<string, Player>();
  private owners = new Map<string, string>();
  private clock?: FixedStepClock;
  private stream?: StateStream;
  private events: TimedEvent[] = [];
  private traces: ShotTrace[] = [];
  private cursor = 0;
  private lifecycle = "";
  private frames: Snapshot[] = [];
  private emptySinceMs?: number;
  private activeMs: number;
  private readonly contentVersion: string;
  constructor(
    readonly options: HostOptions,
    private readonly transport: HostTransport,
  ) {
    this.activeMs = options.nowMs;
    this.emptySinceMs = options.nowMs;
    this.contentVersion = options.contentVersion ?? CONTENT_VERSION;
  }
  get tick(): number {
    return this.clock?.tick ?? 0;
  }
  get debtMs(): number {
    return this.clock?.debtMs ?? 0;
  }
  get connections(): number {
    return this.clients.size;
  }
  private identity() {
    return { roomEpoch: this.options.roomEpoch, roundId: this.roundId };
  }
  private send(connection: string, value: unknown): void {
    this.transport.send(connection, JSON.stringify(value));
  }
  private error(connection: string, code: string, message: string, fatal = false): void {
    this.send(connection, { type: "error", code, message, fatal });
    if (fatal) {
      this.transport.close(connection, 1008, code);
    }
  }
  receive(connection: string, text: string, nowMs: number): void {
    if (this.disposed) {
      this.error(connection, "expired", "This room has expired. Create a new room.", true);
      return;
    }
    try {
      if (new TextEncoder().encode(text).byteLength > MAX_CLIENT_MESSAGE_BYTES) {
        throw new Error("Message too large");
      }
      const message = record(JSON.parse(text));
      if (message.type === "join") {
        if (this.clients.has(connection)) {
          throw new Error("Already joined");
        }
        this.join(connection, message, nowMs);
        return;
      }
      const client = this.clients.get(connection);
      if (!client) {
        throw new Error("Join required");
      }
      if (nowMs - client.windowMs >= 1000) {
        client.windowMs = nowMs;
        client.count = 0;
      }
      if (++client.count > MAX_MESSAGES_PER_SECOND) {
        throw new Error("Message rate exceeded");
      }
      if (message.type === "ping") {
        if (message.roomEpoch !== this.options.roomEpoch || message.roundId !== this.roundId) {
          return;
        }
        const tick = id.read(message.observedTick);
        const t = number(0, Number.MAX_SAFE_INTEGER).read(message.t);
        if (tick > this.tick) {
          throw new Error("Invalid observed tick");
        }
        client.observedTick = Math.max(client.observedTick, tick);
        client.lastSeenMs = nowMs;
        this.send(connection, { type: "pong", t, tick: this.tick });
        return;
      }
      if (message.roomEpoch !== this.options.roomEpoch || message.roundId !== this.roundId) {
        return;
      }
      client.lastSeenMs = nowMs;
      const seat = client.seat;
      switch (message.type) {
        case "input":
          if (this.phase === "playing" && seat.controls?.accept(message, this.tick, nowMs)) {
            client.observedTick = Math.max(client.observedTick, id.read(message.observedTick));
          }
          break;
        case "choose": {
          if (this.phase === "playing") {
            throw new Error("Choices are locked during a round");
          }
          const choice = playerKind.read(message.kind);
          const side = message.team === undefined ? this.autoTeam(seat) : team.read(message.team);
          const slot = this.freeSlot(side, seat);
          if (slot < 0) {
            this.error(connection, "team-full", "That team has six reserved seats.");
            return;
          }
          Object.assign(seat.player, { team: side, slot, kind: choice });
          this.activeMs = nowMs;
          this.broadcastLobby();
          break;
        }
        case "settings":
          if (this.hostId !== seat.player.playerId || this.phase === "playing") {
            throw new Error("Only the lobby host can change settings");
          }
          this.settings = settingsReader.read(message);
          this.activeMs = nowMs;
          this.broadcastLobby();
          break;
        case "start":
          if (this.hostId !== seat.player.playerId || this.phase === "playing") {
            throw new Error("Only the lobby host can start");
          }
          this.start(nowMs);
          break;
        case "end":
          if (this.hostId !== seat.player.playerId || this.phase !== "playing") {
            throw new Error("Only the host can end the round");
          }
          this.simulation!.match.phase = "results";
          this.simulation!.match.endedEarly = true;
          this.finish(nowMs);
          break;
        case "suspend":
          seat.suspended = true;
          seat.controls?.suspend();
          this.sendControl(seat);
          break;
        case "resume":
          seat.suspended = false;
          seat.controls?.resume(nowMs);
          client.observedTick = this.tick;
          this.sendControl(seat);
          this.sendFullLimited(connection, client, nowMs);
          break;
        case "resync":
          this.sendFullLimited(connection, client, nowMs);
          break;
        case "leave":
          this.disconnect(connection, nowMs);
          this.release(seat);
          this.broadcastLobby();
          this.transport.close(connection, 1000, "Left room");
          if (!this.seats.length) {
            this.dispose("empty");
          }
          break;
        default:
          throw new Error("Unknown message");
      }
    } catch (error) {
      this.disconnect(connection, nowMs);
      this.error(
        connection,
        "invalid-message",
        error instanceof Error ? error.message : "Invalid message",
        true,
      );
    }
  }
  private freeSlot(side: 0 | 1, except?: Seat): number {
    for (let slot = 0; slot < TEAM_SLOTS; slot++) {
      if (
        !this.seats.some(
          (seat) => seat !== except && seat.player.team === side && seat.player.slot === slot,
        )
      ) {
        return slot;
      }
    }
    return -1;
  }
  private autoTeam(except?: Seat): 0 | 1 {
    const count = (side: 0 | 1) =>
      this.seats.filter((seat) => seat !== except && seat.player.team === side).length;
    return count(0) <= count(1) ? 0 : 1;
  }
  private join(connection: string, message: Record<string, unknown>, nowMs: number): void {
    const request = joinReader.read(message);
    if (request.version !== PROTOCOL_VERSION || request.contentVersion !== this.contentVersion) {
      this.error(
        connection,
        "incompatible",
        "Game updated. Reload this page before joining.",
        true,
      );
      return;
    }
    if (!request.name.trim()) {
      throw new Error("Enter a player name");
    }
    let seat = request.token
      ? this.seats.find((candidate) => candidate.token === request.token)
      : undefined;
    const create = request.create && !seat && !request.roomEpoch;
    if (create && this.seats.length) {
      this.error(
        connection,
        "room-exists",
        "That room code is already in use. Create another room.",
        true,
      );
      return;
    }
    if (request.existingRoom && !seat && !this.seats.length) {
      this.error(connection, "room-gone", "This room has ended. Go back to the room list.", true);
      return;
    }
    if (request.token && request.roomEpoch === this.options.roomEpoch && !seat) {
      this.error(
        connection,
        "seat-expired",
        "Your reserved seat expired. Join again to take a new seat.",
        true,
      );
      return;
    }
    const reset = !!request.roomEpoch && request.roomEpoch !== this.options.roomEpoch;
    if (!seat) {
      if (this.seats.length >= MAX_PLAYERS) {
        this.error(connection, "room-full", "This room has eight reserved player seats.", true);
        return;
      }
      if (this.participants.size >= MAX_PARTICIPANTS) {
        this.error(
          connection,
          "round-full",
          "This round has reached its participant limit. Try the next round.",
          true,
        );
        return;
      }
      const preferred = request.team ?? this.autoTeam();
      const side =
        request.team === undefined && this.freeSlot(preferred) < 0
          ? ((1 - preferred) as 0 | 1)
          : preferred;
      const slot = this.freeSlot(side);
      if (slot < 0) {
        this.error(connection, "team-full", "That team has six reserved seats.", true);
        return;
      }
      const player: Player = {
        playerId: this.options.token(),
        name: request.name.trim(),
        kind: request.kind,
        team: side,
        slot,
        connected: true,
        kills: 0,
        deaths: 0,
      };
      seat = { player, token: this.options.token(), connection, suspended: false };
      this.seats.push(seat);
      if (this.simulation) {
        const tank = claimPlayerTank(this.simulation, player);
        player.tankId = tank.id;
        seat.controls = new PlayerControls(tank, nowMs, !this.simulation.humansOnly);
        this.participants.set(player.playerId, { ...player });
        this.rememberOwner(tank);
        this.drainEvents(this.tick);
      }
    } else {
      if (seat.connection) {
        const old = seat.connection;
        this.clients.delete(old);
        seat.connection = undefined;
        this.transport.close(old, 4001, "Seat reconnected elsewhere");
      }
      seat.connection = connection;
      seat.player.connected = true;
      seat.disconnectedMs = undefined;
      seat.suspended = false;
      seat.controlKey = undefined;
      seat.controls?.resume(nowMs);
    }
    this.clients.set(connection, {
      seat,
      lastSeenMs: nowMs,
      observedTick: this.tick,
      windowMs: nowMs,
      count: 0,
      fullWindowMs: nowMs,
      fullCount: 0,
    });
    this.emptySinceMs = undefined;
    this.activeMs = nowMs;
    if (!this.hostId) {
      this.hostId = seat.player.playerId;
    }
    this.send(connection, {
      type: "welcome",
      version: PROTOCOL_VERSION,
      contentVersion: this.contentVersion,
      roomEpoch: this.options.roomEpoch,
      playerId: seat.player.playerId,
      token: seat.token,
      hostId: this.hostId,
      reset,
    });
    this.broadcastLobby();
    this.sendControl(seat);
    this.sendFull(connection);
    if (create) {
      this.settings = request.create!;
      this.start(nowMs);
    }
  }
  directoryEntry(room: string): RoomListing {
    return {
      room,
      contentVersion: this.contentVersion,
      ...this.settings,
      players: this.clients.size,
      reserved: this.seats.length,
      phase: this.phase,
      roundId: this.roundId,
      time: Math.max(0, Math.ceil(this.simulation?.match.time ?? this.settings.roundMinutes * 60)),
      scores: this.simulation ? [...this.simulation.match.scores] : [0, 0],
    };
  }
  private rememberOwner(tank: Tank): void {
    if (tank.playerId) {
      this.owners.set(tank.id + ":" + tank.life, tank.playerId);
    }
  }
  private start(nowMs: number): void {
    this.simulation?.dispose();
    this.roundId++;
    this.cursor = 0;
    this.events = [];
    this.traces = [];
    this.owners.clear();
    this.participants.clear();
    this.simulation = createMultiplayerSimulation(
      ((this.options.seed ?? 4242) + this.roundId - 1) >>> 0,
      this.seats.map((seat) => seat.player),
      { ...this.settings, round: this.roundId },
    );
    this.simulation.match.time = this.settings.roundMinutes * 60;
    this.clock = new FixedStepClock(nowMs);
    this.stream = new StateStream(this.identity());
    for (const client of this.clients.values()) {
      client.observedTick = 0;
    }
    this.simulation.onProjectileMove = (shot, seconds, offset) => {
      if (seconds <= 0) {
        return;
      }
      const endTick = this.tick - 1 + (offset + seconds) / STEP;
      this.traces.push(
        rounded({
          tick: this.tick - 1 + offset / STEP,
          endTick,
          shot: shotReader.read({
            ...shot,
            x: shot.x - shot.vx * seconds,
            z: shot.z - shot.vz * seconds,
          }),
          end: { x: shot.x, z: shot.z },
        }),
      );
    };
    for (const seat of this.seats) {
      const tank = this.simulation.tanks.find((tank) => tank.playerId === seat.player.playerId)!;
      Object.assign(seat.player, { tankId: tank.id, kills: 0, deaths: 0 });
      this.participants.set(seat.player.playerId, { ...seat.player });
      this.rememberOwner(tank);
      seat.controls = new PlayerControls(tank, nowMs, !this.simulation.humansOnly);
      seat.controlKey = undefined;
      if (!seat.connection || seat.suspended) {
        seat.controls.suspend();
      }
    }
    this.simulation.start();
    this.phase = "playing";
    this.activeMs = nowMs;
    this.lifecycle = this.lifecycleKey();
    this.frames = [];
    this.broadcastLobby();
    for (const seat of this.seats) {
      this.sendControl(seat);
      if (seat.connection) {
        this.sendFull(seat.connection);
      }
    }
  }
  private drainEvents(tick: number): void {
    for (const raw of this.simulation!.events.splice(0)) {
      if (raw.type === "death") {
        const victim = this.seats.find((seat) => seat.player.tankId === raw.id)?.player;
        if (victim) {
          victim.deaths++;
        }
        const ownerId =
          raw.ownerLife === undefined
            ? undefined
            : this.owners.get(raw.owner + ":" + raw.ownerLife);
        const owner = ownerId
          ? (this.seats.find((seat) => seat.player.playerId === ownerId)?.player ??
            this.participants.get(ownerId))
          : undefined;
        if (owner && raw.owner !== raw.id && owner.team !== raw.team) {
          owner.kills++;
        }
      }
      this.events.push({ eventId: ++this.cursor, tick, event: rounded(eventReader.read(raw)) });
    }
    for (const seat of this.seats) {
      this.participants.set(seat.player.playerId, { ...seat.player });
      const tank = seat.controls?.tank;
      if (tank) {
        tank.kills = seat.player.kills;
        tank.deaths = seat.player.deaths;
        this.rememberOwner(tank);
      }
    }
  }
  advance(nowMs: number): void {
    if (this.disposed) {
      return;
    }
    if (
      (this.emptySinceMs !== undefined && nowMs - this.emptySinceMs >= EMPTY_GRACE_MS) ||
      nowMs - this.options.nowMs >= MAX_ROOM_MS ||
      (this.phase !== "playing" && nowMs - this.activeMs >= ROOM_IDLE_MS)
    ) {
      this.dispose("expired");
      return;
    }
    let changed = false;
    for (const seat of [...this.seats]) {
      if (seat.disconnectedMs !== undefined && nowMs - seat.disconnectedMs >= EMPTY_GRACE_MS) {
        this.release(seat);
        changed = true;
      }
    }
    for (const [connection, client] of this.clients) {
      if (
        nowMs - client.lastSeenMs > CLIENT_TIMEOUT_MS ||
        (!client.seat.suspended && this.tick - client.observedTick > MAX_UNACKNOWLEDGED_TICKS)
      ) {
        this.disconnect(connection, nowMs);
        this.transport.close(connection, 4002, "Connection is not consuming state");
      }
    }
    if (this.phase === "playing") {
      const simulation = this.simulation!;
      if (
        !this.clock!.advance(nowMs, (tick) => {
          if (simulation.match.phase !== "playing") {
            return;
          }
          const commands = new Map<number, VehicleCommand>();
          for (const seat of this.seats) {
            const command = seat.controls!.command(tick, nowMs);
            if (command) {
              commands.set(seat.controls!.tank.id, command);
            }
          }
          simulation.stepWith(commands);
          this.drainEvents(tick);
          const lifecycle = this.lifecycleKey();
          if (lifecycle !== this.lifecycle) {
            this.lifecycle = lifecycle;
            this.captureFrame();
          }
        })
      ) {
        this.dispose("overload");
        return;
      }
      for (const seat of this.seats) {
        // Synchronize post-step deaths/respawns before publishing the next control epoch.
        seat.controls?.refreshLife();
        this.sendControl(seat);
      }
      this.broadcastSnapshot();
      if (simulation.match.phase === "results") {
        this.finish(nowMs);
      }
    }
    if (changed) {
      this.broadcastLobby();
    }
  }
  private broadcastSnapshot(): void {
    if (!this.simulation || !this.stream) {
      return;
    }
    if (this.frames.at(-1)?.tick !== this.tick || this.events.length) {
      this.captureFrame();
    }
    const body = JSON.stringify(this.frames);
    this.frames = [];
    for (const [connection, client] of this.clients) {
      // Hidden/menu clients receive a fresh baseline on resume, not an accumulating stream.
      if (client.seat.suspended) {
        continue;
      }
      const controls = client.seat.controls;
      const ack = { controlEpoch: controls?.controlEpoch ?? 0, ...controls?.ack };
      this.transport.send(
        connection,
        '{"type":"snapshot","ack":' + JSON.stringify(ack) + ',"snapshots":' + body + "}",
      );
    }
  }
  private captureFrame(): void {
    this.frames.push(
      this.stream!.snapshot(captureScene(this.simulation!), this.tick, this.events, this.traces),
    );
    this.events = [];
    this.traces = [];
  }
  private lifecycleKey(): string {
    const sim = this.simulation!;
    // Intermediate deaths/respawns and membership changes survive the 20 Hz batching.
    return [
      sim.tanks.map((t) => t.id + ":" + t.life + ":" + t.alive + ":" + t.playerId),
      sim.covers.map((c) => c.id + ":" + c.alive),
      sim.fragments.map((f) => f.id),
      sim.mines.map((m) => m.id),
      sim.pickups.map((p) => p.id + ":" + p.available),
    ]
      .map((list) => list.join(","))
      .join("/");
  }
  private sendControl(seat: Seat): void {
    const controls = seat.controls;
    if (!seat.connection || !controls) {
      return;
    }
    const tank = controls.tank;
    const value: Control = {
      ...this.identity(),
      type: "control",
      tankId: tank.id,
      life: tank.life,
      controlEpoch: controls.controlEpoch,
      driver: tank.driver,
    };
    const key = JSON.stringify(value);
    if (key !== seat.controlKey) {
      seat.controlKey = key;
      this.transport.send(seat.connection, key);
    }
  }
  private sendFullLimited(connection: string, client: Client, nowMs: number): void {
    if (nowMs - client.fullWindowMs >= 1000) {
      client.fullWindowMs = nowMs;
      client.fullCount = 0;
    }
    if (++client.fullCount > MAX_RESYNCS_PER_SECOND) {
      throw new Error("Too many full-state requests");
    }
    this.sendFull(connection);
  }
  private sendFull(connection: string): void {
    if (this.simulation && this.stream) {
      this.send(
        connection,
        this.stream.full(captureScene(this.simulation), this.tick, this.cursor),
      );
    }
  }
  private finish(nowMs: number): void {
    this.phase = "results";
    this.activeMs = nowMs;
    // END BATTLE can arrive outside a timer callback, so publish its final state too.
    this.broadcastSnapshot();
    this.broadcastLobby();
  }
  private broadcastLobby(): void {
    const value: Lobby = {
      ...this.identity(),
      type: "lobby",
      phase: this.phase,
      hostId: this.hostId,
      players: this.seats.map((seat) => ({ ...seat.player })),
      scoreboard: [...this.participants.values()].map((player) => ({ ...player })),
      settings: this.settings,
    };
    const body = JSON.stringify(value);
    for (const connection of this.clients.keys()) {
      this.transport.send(connection, body);
    }
    this.transport.changed?.();
  }
  disconnect(connection: string, nowMs: number): void {
    const client = this.clients.get(connection);
    if (!client) {
      return;
    }
    this.clients.delete(connection);
    const seat = client.seat;
    seat.connection = undefined;
    seat.disconnectedMs = nowMs;
    seat.player.connected = false;
    seat.controls?.suspend();
    if (this.hostId === seat.player.playerId) {
      this.hostId = this.seats.find((candidate) => candidate.connection)?.player.playerId ?? "";
    }
    if (!this.clients.size) {
      this.emptySinceMs = nowMs;
    }
    this.broadcastLobby();
  }
  private release(seat: Seat): void {
    if (seat.controls && this.simulation) {
      this.participants.set(seat.player.playerId, { ...seat.player, connected: false });
      releasePlayerTank(this.simulation, seat.controls.tank);
    }
    this.seats = this.seats.filter((candidate) => candidate !== seat);
  }
  dispose(reason = "closed"): void {
    if (this.disposed) {
      return;
    }
    this.disposed = true;
    for (const connection of this.clients.keys()) {
      this.send(connection, { type: "room-reset", roomEpoch: this.options.roomEpoch, reason });
      this.transport.close(connection, 1012, reason);
    }
    this.clients.clear();
    this.seats = [];
    this.events = [];
    this.traces = [];
    this.frames = [];
    this.participants.clear();
    this.owners.clear();
    this.simulation?.dispose();
    this.simulation = undefined;
    this.clock = undefined;
    this.stream = undefined;
    this.transport.changed?.();
  }
}
