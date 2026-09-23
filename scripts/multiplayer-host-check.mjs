import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { WebSocket } from "ws";

const base = process.env.SLOPPY_SERVER_URL ?? "ws://127.0.0.1:8787";
const directory = "artifacts/performance/multiplayer";
const key =
  process.env.SLOPPY_EXPERIMENT_KEY ??
  (await readFile(`${directory}/experiment-key`, "utf8")).trim();
const seconds = Number(process.env.SLOPPY_HOST_SECONDS ?? 20);
const count = Number(process.env.SLOPPY_HOST_CLIENTS ?? 4);
const maps = (process.env.SLOPPY_HOST_MAPS ?? "village,harbor,quarry").split(",");
const seed = 4242;
assert.ok(Number.isFinite(seconds) && seconds > 0 && seconds <= 1100);
assert.ok(Number.isInteger(count) && count >= 1 && count <= 8);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const stamp = new Date().toISOString().replaceAll(":", "-");
// Include new source files too: git diff alone misses an untracked server implementation.
const sourceHash = createHash("sha256");
const sources = execFileSync(
  "git",
  ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
  {
    encoding: "utf8",
  },
)
  .split("\0")
  .filter((path) => /\.(ts|mjs|json|jsonc)$/.test(path))
  .sort();
for (const path of sources) sourceHash.update(path + "\0").update(await readFile(path));
const result = {
  date: new Date().toISOString(),
  base,
  seed,
  seconds,
  clients: count,
  node: process.version,
  commit: execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim(),
  sourceSha256: sourceHash.digest("hex"),
  diffSha256: createHash("sha256")
    .update(execFileSync("git", ["diff", "HEAD"]))
    .digest("hex"),
  dependencies: JSON.parse(await readFile("package.json", "utf8")),
  runs: [],
  failures: [],
};
function stats(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return {
    samples: values.length,
    mean: values.reduce((a, b) => a + b, 0) / (values.length || 1),
    p99: sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.99))] ?? 0,
    max: sorted.at(-1) ?? 0,
  };
}
function connect(room, map, { quiet = false, acknowledge = true, sendInvalid = false } = {}) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(`${base}/room/${room}?map=${map}&seed=${seed}`, {
      headers: { Authorization: `Bearer ${key}` },
    });
    const client = {
      socket,
      tick: 0,
      epoch: "",
      seq: 0,
      inputSeq: 0,
      closed: false,
      closeCode: undefined,
      reset: undefined,
      snapshots: [],
      full: [],
      gaps: [],
      rtt: [],
      debt: [],
      firstMs: 0,
      lastMs: 0,
      firstTick: 0,
      error: undefined,
      send: (data) => {
        if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(data));
      },
      stop: () => {
        clearInterval(inputTimer);
        clearInterval(pingTimer);
        socket.close();
      },
    };
    let inputTimer;
    let pingTimer;
    const timeout = setTimeout(() => {
      socket.terminate();
      reject(new Error("Connection timed out"));
    }, 15000);
    socket.on("error", (error) => {
      clearTimeout(timeout);
      client.error = error.message;
      reject(error);
    });
    socket.on("close", (code) => {
      client.closed = true;
      client.closeCode = code;
      clearInterval(inputTimer);
      clearInterval(pingTimer);
      clearTimeout(timeout);
      if (!client.firstMs) reject(new Error(`Connection closed before full state: ${code}`));
    });
    socket.on("message", (bytes) => {
      try {
        const message = JSON.parse(bytes.toString());
        const now = performance.now();
        if (message.type === "welcome") client.epoch = message.roomEpoch;
        if (message.type === "full") {
          client.full.push(bytes.length);
          client.tick = message.tick;
          client.seq = message.seq;
          if (!client.firstMs) {
            client.firstMs = now;
            client.firstTick = message.tick;
            clearTimeout(timeout);
            if (!quiet)
              inputTimer = setInterval(
                () =>
                  client.send({
                    type: "input",
                    seq: ++client.inputSeq,
                    observedTick: acknowledge ? client.tick : client.firstTick,
                    moveX: 0,
                    moveZ: 0,
                    aim: 0,
                    fire: false,
                    actions: [],
                  }),
                50,
              );
            pingTimer = setInterval(
              () =>
                client.send({
                  type: "ping",
                  t: performance.now(),
                  observedTick: acknowledge ? client.tick : client.firstTick,
                }),
              1000,
            );
            if (sendInvalid) socket.send("{");
            resolve(client);
          }
        }
        if (message.type === "room-reset") client.reset = message.reason;
        if (message.type === "pong") client.rtt.push(now - message.t);
        if (message.snapshot) {
          assert.equal(message.snapshot.roomEpoch, client.epoch, "Unexpected room reset");
          assert.equal(message.snapshot.seq, client.seq + 1, "Snapshot sequence gap");
          client.seq = message.snapshot.seq;
          client.tick = message.snapshot.tick;
          if (client.lastMs) client.gaps.push(now - client.lastMs);
          client.lastMs = now;
          client.snapshots.push(bytes.length);
          client.debt.push(message.snapshot.debtMs);
        }
      } catch (error) {
        client.error = error.message;
        client.stop();
        reject(error);
      }
    });
  });
}
async function roomRun(map, clients = count, duration = seconds) {
  const room = `load-${randomUUID()}`;
  const peers = [];
  try {
    // Sequential joins also measure late full-state bursts while the timer is running.
    for (let i = 0; i < clients; i++)
      peers.push(await connect(room, map, { quiet: clients === 1 }));
    const collapsed = setTimeout(
      () => peers[0].send({ type: "collapse" }),
      Math.min(5000, duration * 250),
    );
    await sleep(duration * 1000);
    clearTimeout(collapsed);
    const measurements = peers.map((peer) => {
      const wallSeconds = (peer.lastMs - peer.firstMs) / 1000;
      const simulatedSeconds = (peer.tick - peer.firstTick) / 60;
      return {
        error: peer.error,
        closed: peer.closed,
        closeCode: peer.closeCode,
        reset: peer.reset,
        wallSeconds,
        simulatedSeconds,
        bytesPerSecond: peer.snapshots.reduce((a, b) => a + b, 0) / wallSeconds,
        snapshotBytes: stats(peer.snapshots),
        fullBytes: peer.full,
        gapMs: stats(peer.gaps),
        rttMs: stats(peer.rtt),
        debtMs: stats(peer.debt),
        raw: { snapshotBytes: peer.snapshots, gapMs: peer.gaps },
      };
    });
    result.runs.push({ room, map, clients, duration, measurements });
    for (const measurement of measurements) {
      assert.equal(measurement.error, undefined);
      assert.equal(
        measurement.closed,
        false,
        `Unexpected disconnect: ${measurement.reset ?? measurement.closeCode}`,
      );
      assert.ok(
        Math.abs(measurement.simulatedSeconds - measurement.wallSeconds) < 0.5,
        `Clock drift: ${measurement.simulatedSeconds - measurement.wallSeconds}s`,
      );
    }
    console.log(
      JSON.stringify({
        map,
        clients,
        duration,
        bytesPerSecond: measurements[0].bytesPerSecond,
        maxGapMs: measurements[0].gapMs.max,
        maxDebtMs: measurements[0].debtMs.max,
      }),
    );
  } finally {
    peers.forEach((peer) => peer.stop());
  }
}
async function lifecycle() {
  const room = `lifecycle-${randomUUID()}`;
  let peer = await connect(room, "village");
  const epoch = peer.epoch;
  peer.stop();
  await sleep(1200);
  peer = await connect(room, "village");
  assert.equal(peer.epoch, epoch, "Empty-room grace must retain the match");
  assert.ok(peer.tick >= 60, "Quiet timer must advance without incoming messages");
  peer.send({ type: "stall" });
  await sleep(800);
  assert.equal(peer.reset, "overload");
  peer.stop();
  peer = await connect(room, "village");
  assert.notEqual(peer.epoch, epoch);
  const restartedEpoch = peer.epoch;
  peer.send({ type: "restart" });
  await sleep(200);
  assert.equal(peer.reset, "deliberate-restart");
  peer.stop();
  peer = await connect(room, "village");
  assert.notEqual(peer.epoch, restartedEpoch);
  const expiryEpoch = peer.epoch;
  peer.stop();
  const slow = await connect(`slow-${randomUUID()}`, "village", { acknowledge: false });
  await sleep(4000);
  assert.equal(slow.closed, true, "An unacknowledged stream must be bounded");
  slow.stop();
  const invalid = await connect(`invalid-${randomUUID()}`, "village", { sendInvalid: true });
  await sleep(200);
  assert.equal(invalid.closeCode, 1008);
  invalid.stop();
  await sleep(27000);
  peer = await connect(room, "village");
  assert.notEqual(peer.epoch, expiryEpoch, "Expired room must be recreated");
  peer.stop();
  result.lifecycle =
    "passed: grace, quiet ticks, overload, restart, slow reader, malformed input, expiry";
  console.log(result.lifecycle);
}
try {
  for (const map of maps) await roomRun(map);
  if (process.env.SLOPPY_HOST_EXTENDED === "1") {
    await roomRun("village", 8, 30);
    await roomRun("quarry", 1, 15);
    await Promise.all([roomRun("harbor", 4, 30), roomRun("quarry", 4, 30)]);
    await lifecycle();
  }
} catch (error) {
  result.failures.push(error.stack);
  console.error(error);
  process.exitCode = 1;
} finally {
  await mkdir(directory, { recursive: true });
  const path = `${directory}/host-${stamp}.json`;
  await writeFile(path, JSON.stringify(result, null, 2));
  console.log(`Evidence: ${path}`);
}
