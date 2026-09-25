import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { MatchHost } from "../src/net/match-host";
import { CONTENT_VERSION, PROTOCOL_VERSION, ROOM_CODE } from "../src/net/protocol";
import { BotPlayer, openSeats, randomRoomCode } from "../bots/bot-player";
import { Random } from "../src/game/math";

before(async () => {
  await RAPIER.init();
});

const server = { version: PROTOCOL_VERSION, contentVersion: CONTENT_VERSION };

test("traffic bots create a room, drive with accepted input and stay connected", () => {
  let now = 0;
  const inbox = new Map<string, string[]>();
  const closed: string[] = [];
  const host = new MatchHost(
    { roomEpoch: "bot-room", nowMs: 0, token: () => "seat-" + crypto.randomUUID(), seed: 7 },
    {
      send: (connection, text) => inbox.get(connection)?.push(text),
      close: (connection) => closed.push(connection),
    },
  );
  const rng = new Random(99);
  const bots = ["a", "b", "c"].map((id) => {
    const bot = new BotPlayer("bot-" + id, () => rng.next());
    inbox.set(id, []);
    return { id, bot };
  });
  const deliver = () => {
    for (const { id, bot } of bots) {
      for (const text of inbox.get(id)!.splice(0)) {
        bot.receive(text, now);
      }
    }
  };
  bots.forEach(({ id, bot }, index) => {
    bot.join((text) => host.receive(id, text, now), server, now, index === 0);
    deliver();
  });
  assert.equal(host.phase, "playing", "The creating bot starts the first round");

  const acks = new Map<string, number>();
  const spawn = bots.map(({ bot }) => {
    const tank = host.simulation!.tanks.find((tank) => tank.playerId === bot.playerId)!;
    return { ...tank.body.translation() };
  });
  for (let step = 0; step < 200; step++) {
    now += 50;
    for (const { bot } of bots) {
      bot.update(now);
    }
    host.advance(now);
    for (const { id } of bots) {
      const snapshot = inbox
        .get(id)!
        .filter((text) => text.startsWith('{"type":"snapshot"'))
        .at(-1);
      if (snapshot) {
        acks.set(id, (JSON.parse(snapshot) as { ack: number }).ack);
      }
    }
    deliver();
  }

  assert.deepEqual(closed, [], "No bot was dropped for stale ticks or invalid messages");
  for (const [index, { id, bot }] of bots.entries()) {
    assert.equal(bot.lastError, undefined);
    assert.ok(bot.stats.inputs > 100, "Driving bots send active-rate input");
    assert.ok((acks.get(id) ?? 0) > 0, "The server applied the bot's input");
    const tank = host.simulation!.tanks.find((tank) => tank.playerId === bot.playerId)!;
    assert.equal(tank.driver, "human");
    const position = tank.body.translation();
    assert.ok(
      Math.hypot(position.x - spawn[index].x, position.z - spawn[index].z) > 1,
      "Random driving moves the tank",
    );
  }
  host.dispose();
});

test("open seat planning fills occupied compatible rooms first and counts pending bots", () => {
  const rooms = [
    { room: "AAAAAAAA", contentVersion: "v1", reserved: 2 },
    { room: "BBBBBBBB", contentVersion: "v1", reserved: 6 },
    { room: "CCCCCCCC", contentVersion: "old", reserved: 1 },
    { room: "DDDDDDDD", contentVersion: "v1", reserved: 8 },
    { room: "EEEEEEEE", contentVersion: "v1", reserved: 3 },
  ];
  assert.deepEqual(openSeats(rooms, "v1", new Map([["BBBBBBBB", 2]]), new Set(["EEEEEEEE"])), [
    { room: "AAAAAAAA", free: 6 },
  ]);
  assert.deepEqual(
    openSeats(rooms, "v1", new Map(), new Set()).map((room) => room.room),
    ["BBBBBBBB", "EEEEEEEE", "AAAAAAAA"],
  );
  assert.match(randomRoomCode(), ROOM_CODE);
});
