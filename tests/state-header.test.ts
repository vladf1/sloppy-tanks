import { test } from "node:test";
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
import { BotPlayer } from "../bots/bot-player";
import { readStateHeader } from "../bots/state-header";

// Hand-built headers (`crates/core/src/net/replication.rs`): type byte, then LEB128 varints.
// 300 = 0xac 0x02 and 1000 = 0xe8 0x07 take two bytes each; the bytes after the header
// stand in for the room epoch, scene or frames.
const FULL = Uint8Array.of(1, 0xac, 0x02, 0xe8, 0x07, 5, 9, 2, 0x61, 0x62);
const SNAPSHOT = Uint8Array.of(2, 0xac, 0x02, 0xe8, 0x07, 0x81, 0x01, 0x90, 0x03, 3, 0xff);

test("reads a full baseline's round and tick", () => {
  assert.deepEqual(readStateHeader(FULL), { type: "full", roundId: 300, tick: 1000 });
});

test("reads a snapshot's round, newest tick, ack and frame count", () => {
  assert.deepEqual(readStateHeader(SNAPSHOT), {
    type: "snapshot",
    roundId: 300,
    tick: 1000,
    ack: 129,
    frames: 3,
  });
});

test("rejects unknown types and truncated headers", () => {
  assert.throws(() => readStateHeader(Uint8Array.of(7, 1, 1)), /Unknown state message/);
  assert.throws(() => readStateHeader(Uint8Array.of(2, 0xac)), /Truncated/);
  assert.throws(() => readStateHeader(new Uint8Array()), /Unknown state message/);
});

test("a bot observes binary ticks of its own round and counts their bytes", () => {
  const bot = new BotPlayer("bot-test", () => 0.5);
  const sent: Record<string, unknown>[] = [];
  const send = (text: string) => sent.push(JSON.parse(text) as Record<string, unknown>);
  bot.join(send, { version: 2, contentVersion: "test" }, 0);
  const lobby = JSON.stringify({ type: "lobby", roundId: 300, phase: "lobby", hostId: "h" });
  bot.receive(lobby, 0);
  bot.receive(SNAPSHOT.slice().buffer, 0);
  // Another round's baseline at a later tick is ignored.
  const otherRound = Uint8Array.of(1, 7, 0xf4, 0x03);
  bot.receive(otherRound, 0);
  bot.update(2000);
  assert.deepEqual(
    { type: sent.at(-1)?.type, observedTick: sent.at(-1)?.observedTick },
    { type: "ping", observedTick: 1000 },
  );
  assert.equal(bot.stats.messagesIn, 3);
  assert.equal(bot.stats.bytesIn, lobby.length + SNAPSHOT.length + otherRound.length);
});

test("reads the headers of the Rust host's pinned binary messages", () => {
  // `crates/core/tests/net_golden.rs` writes each binary message's header fields and
  // header bytes from a scripted room, so the bots' reader and the encoder cannot drift.
  const fixture = JSON.parse(
    readFileSync(
      new URL("../crates/core/tests/fixtures/net-golden-binary.json", import.meta.url),
      "utf8",
    ),
  ) as {
    messages: { type: "full" | "snapshot"; roundId: number; tick: number; header: string }[];
  };
  assert.ok(fixture.messages.some((message) => message.type === "full"));
  assert.ok(fixture.messages.some((message) => message.type === "snapshot"));
  for (const message of fixture.messages) {
    const header = readStateHeader(Buffer.from(message.header, "hex"));
    assert.equal(header.type, message.type);
    assert.equal(header.roundId, message.roundId);
    assert.equal(header.tick, message.tick);
  }
});
