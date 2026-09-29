// Generates net-golden.json, the TypeScript MatchHost's messages for the scripted room in
// net-golden-script.json. The Rust port's wire-compatibility test replays the same script.
// Run from the repository root:
//   node --import tsx crates/core/tests/fixtures/net-golden.ts
//
// Recorded per connection: the message type sequence (closes as "close:<code>"), every
// welcome/lobby/control/pong/error/room-reset in full, and for baselines and snapshots
// the set of JSON paths and value types they used (values there depend on physics).
import RAPIER from "@dimforge/rapier3d-simd-compat";
import { readFileSync, writeFileSync } from "node:fs";
import { format, resolveConfig } from "prettier";
import { MatchHost } from "../../../../src/net/match-host";
import { PROTOCOL_VERSION } from "../../../../src/net/protocol";

await RAPIER.init();

const here = new URL(".", import.meta.url);
const script = JSON.parse(readFileSync(new URL("net-golden-script.json", here), "utf8")) as {
  roomEpoch: string;
  seed: number;
  steps: Record<string, unknown>[];
};
const CONTENT = "golden-content";
const EXACT = new Set(["welcome", "lobby", "control", "pong", "error", "room-reset"]);

type Entry = { type: string; exact?: unknown };
const record = new Map<string, Entry[]>();
const signatures = { full: new Set<string>(), snapshot: new Set<string>() };
const welcomed: string[] = [];
const closed = new Set<string>();
const latest = new Map<string, Map<string, Record<string, unknown>>>();

/** Paths and JSON types, with entity-id keys as `*` and events named by their type. */
function signature(value: unknown, path: string, out: Set<string>): void {
  const type = value === null ? "null" : Array.isArray(value) ? "array" : typeof value;
  out.add(path + ":" + type);
  if (Array.isArray(value)) {
    for (const item of value) signature(item, path + "[]", out);
  } else if (value && typeof value === "object") {
    const fields = value as Record<string, unknown>;
    const named =
      path.endsWith(".event") && typeof fields.type === "string" ? `(${fields.type})` : "";
    for (const [key, item] of Object.entries(fields)) {
      signature(item, path + named + "." + (/^\d+$/.test(key) ? "*" : key), out);
    }
  }
}

let token = 0;
let now = 0;
const host = new MatchHost(
  {
    roomEpoch: script.roomEpoch,
    nowMs: 0,
    token: () => "credential-" + String(++token).padStart(20, "0"),
    seed: script.seed,
    contentVersion: CONTENT,
  },
  {
    send(connection, text) {
      const message = JSON.parse(text) as Record<string, unknown>;
      const type = String(message.type);
      const entry: Entry = { type };
      if (EXACT.has(type)) entry.exact = message;
      if (type === "full") signature(message, "full", signatures.full);
      if (type === "snapshot") signature(message, "snapshot", signatures.snapshot);
      if (type === "welcome" && !welcomed.includes(connection)) welcomed.push(connection);
      if (!record.has(connection)) record.set(connection, []);
      record.get(connection)!.push(entry);
      if (!latest.has(connection)) latest.set(connection, new Map());
      latest.get(connection)!.set(type, message);
    },
    close(connection, code) {
      if (!record.has(connection)) record.set(connection, []);
      record.get(connection)!.push({ type: "close:" + code });
      closed.add(connection);
    },
  },
);
const seqs = new Map<string, number>();
const receive = (connection: string, message: object) =>
  host.receive(connection, JSON.stringify(message), now);

for (const step of script.steps) {
  const conn = step.conn as string;
  switch (step.op) {
    case "join": {
      const join: Record<string, unknown> = {
        type: "join",
        version: PROTOCOL_VERSION,
        contentVersion: CONTENT,
        name: conn,
        kind: "balanced",
        ...(step.extra as object),
      };
      if (step.tokenOf) {
        const welcome = latest.get(step.tokenOf as string)!.get("welcome")!;
        join.token = welcome.token;
        join.roomEpoch = welcome.roomEpoch;
      }
      closed.delete(conn);
      receive(conn, join);
      break;
    }
    case "action":
      receive(conn, { type: step.type, roundId: host.roundId, ...(step.extra as object) });
      break;
    case "input": {
      const seq = (seqs.get(conn) ?? 0) + 1;
      seqs.set(conn, seq);
      const input: Record<string, unknown> = {
        type: "input",
        roundId: host.roundId,
        controlEpoch: latest.get(conn)!.get("control")!.controlEpoch,
        seq,
        observedTick: host.tick,
        moveX: step.moveX,
        moveZ: step.moveZ,
        aim: step.aim,
      };
      if (step.fire) input.fire = true;
      if (step.actions) input.actions = step.actions;
      receive(conn, input);
      break;
    }
    case "advance":
      for (let i = 0; i < (step.count as number); i++) {
        now += 50;
        for (const connection of welcomed) {
          if (!closed.has(connection)) {
            receive(connection, {
              type: "ping",
              roundId: host.roundId,
              t: now,
              observedTick: host.tick,
            });
          }
        }
        host.advance(now);
      }
      break;
    case "disconnect":
      host.disconnect(conn, now);
      closed.add(conn);
      break;
    case "dispose":
      host.dispose(step.reason as string);
      break;
    case "raw":
      host.receive(conn, step.text as string, now);
      break;
    default:
      throw new Error("Unknown step " + String(step.op));
  }
}

const connections = Object.fromEntries(record);
const output = new URL("net-golden.json", here);
const text = JSON.stringify({
  connections,
  signatures: {
    full: [...signatures.full].sort(),
    snapshot: [...signatures.snapshot].sort(),
  },
});
// Written in the repository's Prettier style so the format check stays clean.
const options = await resolveConfig(output.pathname);
writeFileSync(output, await format(text, { ...options, parser: "json" }));
console.log(
  "Recorded",
  Object.values(connections).reduce((count, entries) => count + entries.length, 0),
  "messages for",
  record.size,
  "connections",
);
