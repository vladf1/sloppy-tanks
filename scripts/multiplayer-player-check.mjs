import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { setTimeout as wait } from "node:timers/promises";
import WebSocket from "ws";
import { StateMirror } from "../src/net/replication.ts";
import { contentVersion } from "./content-version.mjs";

const endpoint = process.env.SLOPPY_SERVER_URL ?? "ws://127.0.0.1:8787";
const origin = process.env.SLOPPY_ORIGIN ?? "http://127.0.0.1:5175";
const seconds = Number(process.env.SLOPPY_PLAYER_SECONDS ?? 15);
const count = Number(process.env.SLOPPY_PLAYER_CLIENTS ?? 4);
const maps = (process.env.SLOPPY_PLAYER_MAPS ?? "village,harbor,quarry").split(",");
const version = await contentVersion();
const output = `artifacts/performance/multiplayer/players-${Date.now()}.json`;
const report = { endpoint, seconds, count, version, runs: [], errors: [], reconnects: [] };
const recover = process.env.SLOPPY_PLAYER_RECOVER === "1";
const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
function code() {
  return [...crypto.getRandomValues(new Uint8Array(8))].map((n) => alphabet[n & 31]).join("");
}
async function until(check, label, timeout = 10000) {
  const deadline = Date.now() + timeout;
  while (!check()) {
    if (report.errors.length) throw new Error(JSON.stringify(report.errors));
    if (Date.now() > deadline) throw new Error(`Timed out: ${label}`);
    await wait(20);
  }
}
class Player {
  mirror = new StateMirror();
  seq = 0;
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
  }
  async connect() {
    this.expectedClose = false;
    const socket = new WebSocket(`${endpoint}/room/${this.room}`, { origin });
    this.socket = socket;
    socket.on("error", (error) => report.errors.push(error.message));
    socket.on("close", (status, reason) => {
      if (this.socket === socket && !this.expectedClose) {
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
        this.bytes += raw.length;
        this.messages++;
        const m = JSON.parse(String(raw));
        if (m.type === "welcome") {
          this.welcome = m;
          this.token = m.token;
        }
        if (m.type === "lobby") {
          if (this.lobby?.roundId !== m.roundId) this.mirror = new StateMirror();
          this.lobby = m;
          this.rounds = Math.max(this.rounds, m.roundId);
        }
        if (m.type === "control") this.control = m;
        if (m.type === "full") {
          this.mirror.applyFull(m, this.lobby);
          this.fullBytes.push(raw.length);
        }
        if (m.type === "snapshot") {
          this.snapshotBytes.push(raw.length);
          for (const snap of m.snapshots) {
            assert.ok(this.mirror.applySnapshot(snap), "Contiguous valid snapshots");
            this.traces += snap.traces.length;
            this.events += snap.events.length;
            this.deaths += snap.events.filter((e) => e.event.type === "death").length;
            this.respawns += snap.events.filter((e) => e.event.type === "respawn").length;
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
    this.send("join", {
      version: 1,
      contentVersion: version,
      name: `Player ${this.index + 1}`,
      team: this.index % 2,
      kind: this.index % 2 ? "heavy" : "scout",
      token: this.token,
      roomEpoch: this.welcome?.roomEpoch,
    });
    await until(() => this.lobby && this.welcome, "welcome");
    this.timer = setInterval(() => {
      if (this.lobby?.phase !== "playing" || !this.mirror.state || !this.control) return;
      const state = this.mirror.render(this.control.tankId),
        tank = state.viewer;
      if (!tank.alive) return;
      const target = state.tanks
        .filter((t) => t.alive && t.team !== tank.team)
        .sort(
          (a, b) =>
            Math.hypot(a.position.x - tank.position.x, a.position.z - tank.position.z) -
            Math.hypot(b.position.x - tank.position.x, b.position.z - tank.position.z),
        )[0];
      const dx = -tank.position.x,
        dz = -tank.position.z,
        length = Math.hypot(dx, dz) || 1;
      this.send("input", {
        controlEpoch: this.control.controlEpoch,
        seq: ++this.seq,
        observedTick: this.mirror.tick,
        moveX: dx / length,
        moveZ: dz / length,
        aim: target ? { x: target.position.x, z: target.position.z } : { angle: 0 },
        fire: true,
        actions: [],
      });
    }, 50);
    this.ping = setInterval(
      () => this.send("ping", { t: Date.now(), observedTick: this.mirror.tick }),
      1000,
    );
  }
  send(type, fields = {}) {
    if (this.socket?.readyState === WebSocket.OPEN)
      this.socket.send(
        JSON.stringify({
          type,
          roomEpoch: this.welcome?.roomEpoch,
          roundId: this.lobby?.roundId,
          ...fields,
        }),
      );
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
    clearInterval(this.ping);
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
    const room = code(),
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
    const before = players[1]?.welcome.playerId;
    if (players[1]) {
      players[1].close();
      await wait(100);
      players[1].lobby = undefined;
      await players[1].connect();
      assert.equal(players[1].welcome.playerId, before, "Reconnection retains player identity");
      await until(() => players[1].mirror.state, "reconnect baseline");
    }
    await until(() => players.every((p) => !p.recovering && p.lobby), "all recovered");
    players.find((p) => p.lobby.hostId === p.welcome.playerId).send("end");
    await until(() => players.every((p) => p.lobby.phase === "results"), "shared results");
    const row = { map, room, wallMs: Date.now() - start, players: players.map((p) => p.summary()) };
    report.runs.push(row);
    for (const player of row.players) {
      assert.ok(player.fullMax < 160_000, "Full-state budget");
      assert.ok(player.snapshotMax < 128_000, "Snapshot batch budget");
      assert.ok(player.bytesPerSecond < 512_000, "Sustained JSON byte budget");
    }
    console.log(JSON.stringify(row));
    for (const player of players) {
      player.send("leave");
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
