import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { VEHICLES } from "../src/game/data";
import { MatchHost } from "../src/net/match-host";
import {
  CONTENT_VERSION,
  MAX_SERVER_MESSAGE_BYTES,
  PROTOCOL_VERSION,
  type ServerMessage,
} from "../src/net/protocol";
import { StateMirror } from "../src/net/replication";
import { projectScene, SCENARIO_ROOMS } from "../src/net/scene-codec";
import { SUPERSTRESS_MAX_FRAGMENTS, SUPERSTRESS_SCALE } from "../src/superstress-level";

before(async () => {
  await RAPIER.init();
});

function room() {
  let now = 0;
  let token = 0;
  const texts = new Map<string, string[]>();
  const host = new MatchHost(
    {
      roomEpoch: "yard-room",
      nowMs: 0,
      token: () => "credential-" + String(++token).padStart(20, "0"),
      seed: 4242,
    },
    {
      send(connection, text) {
        texts.set(connection, [...(texts.get(connection) ?? []), text]);
      },
      close() {},
    },
  );
  const send = (connection: string, message: object) =>
    host.receive(connection, JSON.stringify(message), now);
  const messages = (connection: string) =>
    (texts.get(connection) ?? []).map((text) => JSON.parse(text) as ServerMessage);
  return {
    host,
    texts,
    messages,
    join: (connection: string, extra: object = {}) =>
      send(connection, {
        type: "join",
        version: PROTOCOL_VERSION,
        contentVersion: CONTENT_VERSION,
        name: connection,
        kind: "balanced",
        ...extra,
      }),
    action: (connection: string, type: string) => send(connection, { type, roundId: host.roundId }),
    lobby: (connection: string) =>
      messages(connection)
        .reverse()
        .find((message) => message.type === "lobby") as Extract<ServerMessage, { type: "lobby" }>,
    advance() {
      now += 50;
      for (const connection of texts.keys()) {
        send(connection, { type: "ping", roundId: host.roundId, t: now, observedTick: host.tick });
      }
      host.advance(now);
    },
  };
}

test("a room's first player decides its scenario, which its room listing carries", () => {
  const yard = room();
  const standard = room();
  try {
    yard.join("alice", { scenario: "superstress" });
    yard.join("bob");
    assert.equal(yard.lobby("bob").scenario, "superstress");
    assert.equal(yard.host.directoryEntry("YARDROOM").scenario, "superstress");
    yard.action("alice", "start");
    const sim = yard.host.simulation!;
    assert.equal(sim.mapName, "SCRAP YARD");
    assert.equal(sim.tanks.length, 30);
    assert.equal(SCENARIO_ROOMS.superstress.teamTanks * 2, 30, "lobby rosters count the bots");
    assert.equal(sim.maxFragments, SUPERSTRESS_MAX_FRAGMENTS);
    assert.ok(sim.afterStep, "the yard's rebuild and debris rules run in the room");
    const alice = sim.tanks.find((tank) => tank.name === "alice")!;
    assert.equal(sim.maxHealth(alice), VEHICLES.balanced.health, "players are not invulnerable");
    // A late player takes over one of the 30 bot tanks instead of adding a 31st.
    yard.join("erin", { scenario: "superstress" });
    assert.equal(sim.tanks.length, 30);
    assert.equal(sim.tanks.filter((tank) => tank.driver === "human").length, 3);
    assert.ok(sim.tanks.some((tank) => tank.name === "erin" && tank.human));

    standard.join("carol");
    standard.join("dave", { scenario: "superstress" });
    assert.equal(standard.lobby("dave").scenario, undefined);
    assert.ok(
      !("scenario" in JSON.parse(JSON.stringify(standard.host.directoryEntry("ROOMCODE")))),
    );
    standard.action("carol", "start");
    assert.equal(standard.host.simulation!.mapName, "PINE VILLAGE");
    assert.equal(standard.host.simulation!.tanks.length, 12);
    assert.equal(standard.host.simulation!.afterStep, undefined);
    assert.ok(
      standard.texts
        .get("carol")!
        .every((text) => !text.includes('"scenario"') && !text.includes('"scale"')),
      "standard rooms send the same lobby and scene fields as before",
    );
  } finally {
    yard.host.dispose("test");
    standard.host.dispose("test");
  }
});

test("a superstress room replicates the compact yard, its debris and rebuilt cover", () => {
  const yard = room();
  try {
    yard.join("alice", { scenario: "superstress" });
    yard.action("alice", "start");
    const mirror = new StateMirror();
    const identity = { roomEpoch: "yard-room", roundId: yard.host.roundId };
    let read = 0;
    let mostFragments = 0;
    let largestBatch = 0;
    const fallen = new Set<number>();
    const rebuilt = new Set<number>();
    for (let step = 0; step < 15 * 20; step++) {
      yard.advance();
      const texts = yard.texts.get("alice")!;
      for (; read < texts.length; read++) {
        const message = JSON.parse(texts[read]) as ServerMessage;
        assert.ok(texts[read].length < MAX_SERVER_MESSAGE_BYTES);
        if (message.type === "full") {
          mirror.applyFull(message, identity);
        } else if (message.type === "snapshot") {
          largestBatch = Math.max(largestBatch, message.snapshots.length);
          for (const snapshot of message.snapshots) {
            assert.ok(mirror.applySnapshot(snapshot), "every snapshot passes client validation");
          }
        }
      }
      const scene = mirror.state!;
      mostFragments = Math.max(mostFragments, scene.entities.fragments.length);
      for (const cover of scene.entities.covers) {
        if (!cover.alive) {
          fallen.add(cover.id);
        } else if (fallen.has(cover.id)) {
          rebuilt.add(cover.id);
        }
      }
    }
    assert.equal(yard.host.disposed, false, "the room keeps up with its fixed step");
    assert.ok(largestBatch <= 8, "clients accept every snapshot batch");
    const scene = mirror.state!;
    assert.equal(scene.map.theme, "superstress");
    assert.equal(scene.entities.tanks.length, 30);
    assert.ok(mostFragments > 128, `debris reached ${mostFragments} pieces`);
    assert.ok(rebuilt.size > 0, "rebuilt cover reaches clients with its identity");
    const view = projectScene(scene, scene.entities.tanks[0].id);
    assert.equal(view.mapScale, SUPERSTRESS_SCALE);
  } finally {
    yard.host.dispose("test");
  }
});
