import assert from "node:assert/strict";
import { writeFile, mkdir } from "node:fs/promises";
import { setTimeout as wait } from "node:timers/promises";
import WebSocket from "ws";
import { contentVersion } from "./content-version.mjs";
const endpoint = process.env.SLOPPY_SERVER_URL ?? "ws://127.0.0.1:8787";
const version = await contentVersion(),
  sockets = [],
  report = { endpoint, checks: [] };
const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const room = [...crypto.getRandomValues(new Uint8Array(8))].map((n) => alphabet[n & 31]).join("");
async function until(check, label, ms = 10000) {
  const end = Date.now() + ms;
  while (!check()) {
    assert.ok(Date.now() < end, label);
    await wait(20);
  }
}
async function connect(name, previous, code = room) {
  const ws = new WebSocket(`${endpoint}/room/${code}`, { origin: "http://127.0.0.1:5175" });
  const client = {
    ws,
    messages: [],
    tick: 0,
    send(type, fields = {}) {
      if (ws.readyState === WebSocket.OPEN)
        ws.send(
          JSON.stringify({
            type,
            roomEpoch: client.welcome?.roomEpoch,
            roundId: client.lobby?.roundId,
            ...fields,
          }),
        );
    },
  };
  sockets.push(client);
  ws.on("message", (raw) => {
    const m = JSON.parse(String(raw));
    client.messages.push(m);
    if (m.type === "welcome") client.welcome = m;
    if (m.type === "lobby") client.lobby = m;
    if (m.type === "control") client.control = m;
    if (m.type === "full") client.tick = m.tick;
    if (m.type === "snapshot") client.tick = m.snapshots.at(-1).tick;
  });
  ws.on("close", (code) => (client.closeCode = code));
  await new Promise((resolve, reject) => {
    ws.once("open", resolve);
    ws.once("error", reject);
  });
  client.send("join", {
    version: 1,
    contentVersion: version,
    name,
    kind: "balanced",
    token: previous?.token,
    roomEpoch: previous?.roomEpoch,
  });
  await until(() => client.welcome, "welcome");
  client.ping = setInterval(
    () => client.send("ping", { t: Date.now(), observedTick: client.slow ? 0 : client.tick }),
    1000,
  );
  client.close = () => {
    clearInterval(client.ping);
    ws.close();
  };
  return client;
}
try {
  const a = await connect("Alice"),
    b = await connect("Bob");
  a.send("start");
  await until(() => a.tick > 600, "quiet room keeps advancing", 18000);
  assert.equal(a.control.driver, "bot");
  assert.equal(b.control.driver, "bot");
  report.checks.push("Quiet human clients become bots; timer continues beyond ten seconds");
  const epoch = a.welcome.roomEpoch,
    tokenA = a.welcome,
    tokenB = b.welcome;
  a.close();
  await until(() => b.lobby.hostId === b.welcome.playerId, "host transfer");
  b.close();
  await wait(10000);
  const backB = await connect("Bob", tokenB),
    backA = await connect("Alice", tokenA);
  assert.equal(backA.welcome.roomEpoch, epoch);
  assert.equal(backA.welcome.playerId, tokenA.playerId);
  assert.equal(backB.welcome.playerId, tokenB.playerId);
  assert.equal(backA.lobby.hostId, tokenB.playerId);
  assert.ok(backA.tick > 600);
  report.checks.push("Both clients reconnect during empty-room grace; host transfer persists");
  const replacement = await connect("Alice", backA.welcome);
  await until(() => backA.closeCode === 4001, "old socket revoked");
  backA.close();
  assert.equal(replacement.welcome.playerId, tokenA.playerId);
  report.checks.push("A seat token revokes its old socket");
  const slow = await connect("Slow reader");
  slow.slow = true;
  await until(() => slow.closeCode === 4002, "slow reader disconnected", 6000);
  slow.close();
  report.checks.push("A non-consuming reader is disconnected");
  const oldTick = backB.tick;
  backB.send("suspend");
  await until(() => backB.control.driver === "bot", "suspend");
  await wait(4000);
  backB.send("resume");
  await until(() => backB.tick > oldTick + 150 && backB.control.driver === "human", "fresh resume");
  report.checks.push("Suspended connection resumes after more than the slow-reader grace");
  replacement.close();
  backB.close();
  await wait(31500);
  const fresh = await connect("Alice", tokenA);
  assert.notEqual(fresh.welcome.roomEpoch, epoch);
  assert.equal(fresh.welcome.reset, true);
  assert.equal(fresh.lobby.phase, "lobby");
  report.checks.push(
    "Empty room expires and an old token enters a fresh lobby with a reset notice",
  );
  fresh.send("leave");
  fresh.close();
  console.log(JSON.stringify(report));
} finally {
  for (const c of sockets) c.close?.();
  await mkdir("artifacts/performance/multiplayer", { recursive: true });
  await writeFile(
    `artifacts/performance/multiplayer/lifecycle-${Date.now()}.json`,
    JSON.stringify(report, null, 2),
  );
}
