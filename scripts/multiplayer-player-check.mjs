import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { setTimeout as wait } from "node:timers/promises";
import WebSocket from "ws";
import { BotPlayer, randomRoomCode } from "../bots/bot-player.ts";
import { PROTOCOL_VERSION } from "../src/net/protocol.ts";
import { StateMirror } from "../src/net/replication.ts";
import { contentVersion } from "./content-version.mjs";

const endpoint = process.env.SLOPPY_SERVER_URL ?? "ws://127.0.0.1:8787";
const origin = process.env.SLOPPY_ORIGIN ?? "http://127.0.0.1:5173";
const seconds = Number(process.env.SLOPPY_PLAYER_SECONDS ?? 15);
const count = Number(process.env.SLOPPY_PLAYER_CLIENTS ?? 4);
const maps = (process.env.SLOPPY_PLAYER_MAPS ?? "village,harbor,quarry").split(",");
const recover = process.env.SLOPPY_PLAYER_RECOVER === "1";
const server = { version: PROTOCOL_VERSION, contentVersion: await contentVersion() };
const output = `artifacts/performance/multiplayer/players-${Date.now()}.json`;
const report = { endpoint, seconds, count, server, runs: [], errors: [], reconnects: [] };
/** The traffic bots' input loop wakes this often; BotPlayer rate-limits its own sends. */
const UPDATE_MS = 20;
async function until(check, label, timeout = 10000) {
  const deadline = Date.now() + timeout;
  while (!check()) {
    if (report.errors.length) throw new Error(JSON.stringify(report.errors));
    if (Date.now() > deadline) throw new Error(`Timed out: ${label}`);
    await wait(20);
  }
}
/**
 * One real socket driven by the traffic bots' BotPlayer, which sends the same joins,
 * input cadence, pings and resumes as a browser tab. This wrapper only measures bytes and
 * mirrors every snapshot to prove the stream stays contiguous.
 */
class Player {
  mirror = new StateMirror();
  bytes = 0;
  messages = 0;
  snapshotBytes = [];
  fullBytes = [];
  rtts = [];
  traces = 0;
  events = 0;
  deaths = 0;
  respawns = 0;
  rounds = 0;
  expectedClose = false;
  recovering = false;
  constructor(room, index) {
    this.room = room;
    this.index = index;
    this.bot = new BotPlayer(`Player ${index + 1}`);
  }
  async connect() {
    this.expectedClose = false;
    this.welcome = undefined;
    const socket = new WebSocket(`${endpoint}/room/${this.room}`, { origin });
    this.socket = socket;
    socket.on("error", (error) => report.errors.push(error.message));
    socket.on("close", (status, reason) => {
      if (this.socket !== socket) return;
      this.bot.disconnected();
      if (!this.expectedClose) {
        const incident = {
          close: status,
          reason: String(reason),
          room: this.room,
          player: this.index,
          tick: this.mirror.tick,
          round: this.lobby?.roundId,
          at: new Date().toISOString(),
        };
        if (!recover || this.recovering) report.errors.push(incident);
        else void this.reconnect(incident).catch((error) => report.errors.push(error.stack));
      }
    });
    socket.on("message", (raw) => {
      try {
        const text = String(raw);
        this.bytes += raw.length;
        this.messages++;
        this.bot.receive(text, Date.now());
        const m = JSON.parse(text);
        if (m.type === "welcome") this.welcome = m;
        if (m.type === "lobby") {
          if (this.lobby?.roundId !== m.roundId) this.mirror = new StateMirror();
          this.lobby = m;
          this.rounds = Math.max(this.rounds, m.roundId);
        }
        if (m.type === "full") {
          this.mirror.applyFull(m, this.lobby);
          this.fullBytes.push(raw.length);
        }
        if (m.type === "snapshot") {
          this.snapshotBytes.push(raw.length);
          for (const snap of m.snapshots) {
            assert.ok(this.mirror.applySnapshot(snap), "Contiguous valid snapshots");
            const events = snap.events ?? [];
            this.traces += snap.traces?.length ?? 0;
            this.events += events.length;
            this.deaths += events.filter((e) => e.event.type === "death").length;
            this.respawns += events.filter((e) => e.event.type === "respawn").length;
          }
        }
        if (m.type === "pong") this.rtts.push(Date.now() - m.t);
        if (m.type === "error" || m.type === "room-reset") report.errors.push(m);
      } catch (error) {
        report.errors.push(error.stack);
      }
    });
    await new Promise((resolve, reject) => {
      socket.once("open", resolve);
      socket.once("error", reject);
    });
    this.bot.join((text) => socket.send(text), server, Date.now());
    await until(() => this.lobby && this.welcome, "welcome");
    this.timer = setInterval(() => this.bot.update(Date.now()), UPDATE_MS);
  }
  /** Host commands the bots never send: explicit settings, start and end. */
  send(type, fields = {}) {
    if (this.socket?.readyState === WebSocket.OPEN)
      this.socket.send(JSON.stringify({ type, roundId: this.bot.roundId, ...fields }));
  }
  async reconnect(incident) {
    this.recovering = true;
    const before = this.welcome;
    this.close();
    this.lobby = undefined;
    this.mirror = new StateMirror();
    await wait(500);
    await this.connect();
    await until(() => this.mirror.state, "reconnect state");
    assert.equal(
      this.welcome.roomEpoch,
      before.roomEpoch,
      "Transport reconnect must not lose the room",
    );
    assert.equal(
      this.welcome.playerId,
      before.playerId,
      "Transport reconnect must retain the seat",
    );
    report.reconnects.push({
      ...incident,
      recoveredTick: this.mirror.tick,
      recoveredAt: new Date().toISOString(),
    });
    this.recovering = false;
  }
  close() {
    this.expectedClose = true;
    clearInterval(this.timer);
    this.socket?.close();
  }
  summary() {
    const sorted = this.snapshotBytes.toSorted((a, b) => a - b);
    return {
      bytes: this.bytes,
      messages: this.messages,
      bytesPerSecond: this.bytes / seconds,
      snapshotP95: sorted[Math.floor(sorted.length * 0.95)],
      snapshotMax: Math.max(0, ...sorted),
      fullMax: Math.max(0, ...this.fullBytes),
      rttP95: this.rtts.toSorted((a, b) => a - b)[Math.floor(this.rtts.length * 0.95)],
      inputs: this.bot.stats.inputs,
      traces: this.traces,
      events: this.events,
      deaths: this.deaths,
      respawns: this.respawns,
      rounds: this.rounds,
      tick: this.mirror.tick,
    };
  }
}
const open = [];
try {
  for (const map of maps) {
    const room = randomRoomCode(),
      players = Array.from({ length: count }, (_, i) => new Player(room, i));
    open.push(...players);
    for (const player of players) await player.connect();
    const host = players[0];
    host.send("settings", { mapMode: map, difficulty: "normal", humansOnly: false });
    host.send("start");
    await until(() => players.every((p) => p.mirror.state), "initial states");
    const start = Date.now();
    let lastRound = 1;
    while (Date.now() - start < seconds * 1000) {
      if (report.errors.length) throw new Error(JSON.stringify(report.errors));
      const currentHost = players.find(
        (p) => p.lobby?.hostId === p.welcome?.playerId && !p.recovering,
      );
      if (currentHost?.lobby.phase === "results" && currentHost.lobby.roundId === lastRound) {
        currentHost.send("start");
        lastRound++;
      }
      await wait(100);
    }
    const guest = players[1];
    if (guest) {
      const before = guest.welcome.playerId;
      guest.close();
      await wait(100);
      guest.lobby = undefined;
      await guest.connect();
      assert.equal(guest.welcome.playerId, before, "Reconnection retains player identity");
      await until(() => guest.mirror.state, "reconnect baseline");
    }
    await until(() => players.every((p) => !p.recovering && p.lobby), "all recovered");
    players.find((p) => p.lobby.hostId === p.welcome.playerId).send("end");
    await until(() => players.every((p) => p.lobby.phase === "results"), "shared results");
    const row = { map, room, wallMs: Date.now() - start, players: players.map((p) => p.summary()) };
    report.runs.push(row);
    for (const player of row.players) {
      assert.ok(player.inputs > 0, "Players drive with real input");
      assert.ok(player.fullMax < 160_000, "Full-state budget");
      assert.ok(player.snapshotMax < 128_000, "Snapshot batch budget");
      assert.ok(player.bytesPerSecond < 512_000, "Sustained JSON byte budget");
    }
    console.log(JSON.stringify(row));
    for (const player of players) {
      player.bot.leave();
      player.close();
    }
  }
  assert.deepEqual(report.errors, []);
  console.log("Real player protocol passed:", output);
} finally {
  report.finalClients = open.map((player) => ({
    room: player.room,
    player: player.index,
    ...player.summary(),
  }));
  for (const player of open) player.close();
  await mkdir("artifacts/performance/multiplayer", { recursive: true });
  await writeFile(output, JSON.stringify(report, null, 2));
}
