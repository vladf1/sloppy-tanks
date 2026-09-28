import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-simd-compat";
import { VEHICLES } from "../src/game/data";
import { MatchHost } from "../src/net/match-host";
import {
  CONTENT_VERSION,
  MAX_SERVER_MESSAGE_BYTES,
  PROTOCOL_VERSION,
  type ServerMessage,
} from "../src/net/protocol";
import { StateMirror } from "../src/net/replication";
import { projectScene } from "../src/net/scene-codec";
import { MAP_OPTIONS } from "../src/game/map-options";
import { MAPS } from "../src/game/maps";
import { EXTRA_LEVELS } from "../src/extra-levels";
import {
  STRESS_AMMO_CRATE_MULTIPLIER,
  STRESS_PLAYER_HEALTH_MULTIPLIER,
  STRESS_POWER_UP_MULTIPLIER,
  STRESS_TEST_MAP,
} from "../src/stress-test-level";
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
    action: (connection: string, type: string, extra: object = {}) =>
      send(connection, { type, roundId: host.roundId, ...extra }),
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

/** Room settings for a new room on `mapMode`, as Battle Setup's CREATE ROOM sends them. */
const create = (mapMode: string) => ({
  create: { mapMode, difficulty: "normal", humansOnly: false, roundMinutes: 5 },
});

test("every extra level offered by the menu has a matching level with its roster", () => {
  for (const option of MAP_OPTIONS) {
    if (!("extra" in option)) {
      assert.ok(
        MAPS.some((map) => map.id === option.id),
        `${option.id} has a standard layout`,
      );
      continue;
    }
    const level = EXTRA_LEVELS[option.id];
    // Its map id names the replicated theme, which clients validate against map ids.
    assert.equal(level.customMap?.id, option.id);
    assert.equal(level.customMap?.name, option.name);
    assert.equal(level.customMap?.description, option.description);
    assert.equal(option.teamTanks * 2, level.roundCount, "lobby rosters count the bots");
  }
});

test("a Scrap Yard room plays the yard's rules, and its host can switch back to a standard map", () => {
  const yard = room();
  try {
    yard.join("alice", create("superstress"));
    yard.join("bob");
    assert.equal(yard.lobby("bob").settings.mapMode, "superstress");
    assert.equal(yard.host.directoryEntry("YARDROOM").mapMode, "superstress");
    const sim = yard.host.simulation!;
    assert.equal(sim.mapName, "SCRAP YARD");
    assert.equal(sim.tanks.length, 30);
    assert.equal(sim.maxFragments, SUPERSTRESS_MAX_FRAGMENTS);
    assert.ok(sim.afterStep, "the yard's rebuild and debris rules run in the room");
    assert.equal(sim.endlessMatch, false, "rooms keep their match length");
    const boosted = VEHICLES.balanced.health * STRESS_PLAYER_HEALTH_MULTIPLIER;
    const alice = sim.tanks.find((tank) => tank.name === "alice")!;
    assert.equal(sim.maxHealth(alice), boosted, "players are nearly invulnerable");
    assert.equal(alice.hp, boosted);
    const bot = sim.tanks.find((tank) => !tank.human)!;
    assert.equal(sim.maxHealth(bot), VEHICLES[bot.kind].health, "bots keep normal health");
    assert.equal(sim.powerUpDurationMultiplier, STRESS_POWER_UP_MULTIPLIER);
    assert.equal(sim.ammoCrateMultiplier, STRESS_AMMO_CRATE_MULTIPLIER);
    // A late player takes over one of the 30 bot tanks instead of adding a 31st.
    yard.join("erin");
    assert.equal(sim.tanks.length, 30);
    assert.equal(sim.tanks.filter((tank) => tank.driver === "human").length, 3);
    const erin = sim.tanks.find((tank) => tank.name === "erin" && tank.human)!;
    assert.equal(erin.hp, boosted, "a taken-over bot tank respawns with the player's hull");

    // Between battles the host picks any map, standard or extra, like any room setting.
    yard.action("alice", "end");
    yard.action("alice", "settings", {
      mapMode: "village",
      difficulty: "normal",
      humansOnly: false,
    });
    yard.action("alice", "start");
    const village = yard.host.simulation!;
    assert.equal(village.mapName, "PINE VILLAGE");
    assert.equal(village.tanks.length, 12);
    assert.equal(village.afterStep, undefined);
    const host = village.tanks.find((tank) => tank.name === "alice")!;
    assert.equal(village.maxHealth(host), VEHICLES.balanced.health);
    assert.equal(yard.host.directoryEntry("YARDROOM").mapMode, "village");
  } finally {
    yard.host.dispose("test");
  }
});

test("standard rooms send the same lobby and scene fields as before", () => {
  const standard = room();
  try {
    standard.join("carol");
    standard.action("carol", "start");
    assert.equal(standard.host.simulation!.mapName, "PINE VILLAGE");
    assert.equal(standard.host.simulation!.tanks.length, 12);
    assert.ok(
      !("scenario" in JSON.parse(JSON.stringify(standard.host.directoryEntry("ROOMCODE")))),
    );
    assert.ok(
      standard.texts
        .get("carol")!
        .every((text) => !text.includes('"scenario"') && !text.includes('"scale"')),
    );
  } finally {
    standard.host.dispose("test");
  }
});

/** Play `ticks` host intervals and mirror the scene as a client would, checking each message. */
function mirrorRoom(level: ReturnType<typeof room>, ticks: number) {
  const mirror = new StateMirror();
  const identity = { roomEpoch: "yard-room", roundId: level.host.roundId };
  let read = 0;
  let mostFragments = 0;
  let largestBatch = 0;
  const fallen = new Set<number>();
  const rebuilt = new Set<number>();
  for (let step = 0; step < ticks; step++) {
    level.advance();
    const texts = level.texts.get("alice")!;
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
  assert.equal(level.host.disposed, false, "the room keeps up with its fixed step");
  assert.ok(largestBatch <= 8, "clients accept every snapshot batch");
  return { scene: mirror.state!, mostFragments, rebuilt };
}

test("a Scrap Yard room replicates the compact yard, its debris and rebuilt cover", () => {
  const yard = room();
  try {
    yard.join("alice", create("superstress"));
    const { scene, mostFragments, rebuilt } = mirrorRoom(yard, 15 * 20);
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

test("a Stress Grid room fields its 30 tanks on the full-size grid", () => {
  const grid = room();
  try {
    grid.join("alice", create("stress-test"));
    const sim = grid.host.simulation!;
    assert.equal(sim.mapName, "STRESS GRID");
    const alice = sim.tanks.find((tank) => tank.name === "alice")!;
    assert.equal(sim.maxHealth(alice), VEHICLES.balanced.health * STRESS_PLAYER_HEALTH_MULTIPLIER);
    const { scene } = mirrorRoom(grid, 5 * 20);
    assert.equal(scene.map.theme, "stress-test");
    assert.equal(scene.map.floor, STRESS_TEST_MAP.floor);
    assert.equal(scene.entities.tanks.length, EXTRA_LEVELS["stress-test"].roundCount);
    assert.equal(projectScene(scene, alice.id).mapScale, 1);
  } finally {
    grid.host.dispose("test");
  }
});
