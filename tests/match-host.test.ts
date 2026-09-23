import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { MatchHost } from "../src/net/match-host";
import { StateMirror, StateStream, type FullState, type Snapshot } from "../src/net/replication";
import {
  CONTENT_VERSION,
  PROTOCOL_VERSION,
  type ServerMessage,
  type Control,
  settingsReader,
} from "../src/net/protocol";
import { captureScene, projectScene } from "../src/net/scene-codec";
import { createMultiplayerSimulation } from "../src/net/multiplayer-simulation";
import { idleCommand } from "../src/game/types";
before(async () => {
  await RAPIER.init();
});
function harness() {
  let now = 0,
    token = 0;
  const messages = new Map<string, ServerMessage[]>(),
    closed: string[] = [];
  const host = new MatchHost(
    {
      roomEpoch: "test-room",
      nowMs: 0,
      token: () => "credential-" + String(++token).padStart(20, "0"),
      seed: 4242,
    },
    {
      send(connection, text) {
        const list = messages.get(connection) ?? [];
        list.push(JSON.parse(text) as ServerMessage);
        messages.set(connection, list);
      },
      close(connection) {
        closed.push(connection);
      },
    },
  );
  const send = (connection: string, message: object) =>
    host.receive(connection, JSON.stringify(message), now);
  const join = (connection: string, extra: object = {}) =>
    send(connection, {
      type: "join",
      version: PROTOCOL_VERSION,
      contentVersion: CONTENT_VERSION,
      name: connection,
      kind: "balanced",
      ...extra,
    });
  const action = (connection: string, type: string, extra: object = {}) =>
    send(connection, { type, roomEpoch: host.options.roomEpoch, roundId: host.roundId, ...extra });
  const latest = <T extends ServerMessage["type"]>(connection: string, type: T) =>
    messages
      .get(connection)!
      .slice()
      .reverse()
      .find((message) => message.type === type) as Extract<ServerMessage, { type: T }>;
  const advance = (ms = 50) => {
    now += ms;
    for (const connection of messages.keys())
      action(connection, "ping", { t: now, observedTick: host.tick });
    host.advance(now);
  };
  return {
    host,
    messages,
    closed,
    send,
    join,
    action,
    latest,
    advance,
    get now() {
      return now;
    },
    set now(value: number) {
      now = value;
    },
  };
}

test("humans-only handles pause, reconnect, late join, death and departures without fill bots", () => {
  const h = harness();
  try {
    h.join("alice", { team: 0 });
    h.join("bob", { team: 1 });
    h.action("alice", "settings", { mapMode: "harbor", difficulty: "normal", humansOnly: true });
    assert.equal(h.latest("bob", "lobby").settings.humansOnly, true);
    h.action("alice", "start");
    const sim = h.host.simulation!;
    assert.equal(sim.tanks.length, 2);
    const alice = sim.tanks.find(
      (tank) => tank.playerId === h.latest("alice", "welcome").playerId,
    )!;
    const bob = sim.tanks.find((tank) => tank.playerId === h.latest("bob", "welcome").playerId)!;
    h.action("alice", "suspend");
    assert.equal(h.latest("alice", "control").driver, "idle");
    for (let i = 0; i < 110; i++) h.advance();
    assert.ok(sim.tanks.every((tank) => tank.driver === "idle"));
    assert.equal(sim.shotsFired, 0);
    h.action("alice", "resume");
    assert.equal(alice.driver, "human");
    const token = h.latest("bob", "welcome").token;
    h.host.disconnect("bob", h.now);
    assert.equal(bob.driver, "idle");
    h.join("bob-again", { token, roomEpoch: "test-room" });
    assert.equal(h.latest("bob-again", "control").tankId, bob.id);
    assert.equal(bob.driver, "human");
    h.action("bob-again", "leave");
    assert.equal(sim.tanks.length, 1);
    assert.equal(bob.body.isValid(), false, "leaving removes the hull collider too");
    h.advance();
    const mirror = new StateMirror();
    const full = h.latest("alice", "full");
    mirror.applyFull(full, full);
    for (const message of h.messages.get("alice")!) {
      if (message.type === "snapshot" && message.snapshots[0].seq > full.seq)
        for (const snapshot of message.snapshots) assert.ok(mirror.applySnapshot(snapshot));
    }
    assert.equal(mirror.render(alice.id).tanks.length, 1, "mirror removes departed tanks");
    h.join("carol", { team: 1, kind: "heavy" });
    const carol = sim.tanks.find(
      (tank) => tank.playerId === h.latest("carol", "welcome").playerId,
    )!;
    assert.equal(sim.tanks.length, 2);
    assert.equal(carol.name, "carol");
    assert.equal(carol.kind, "heavy");
    assert.notEqual(carol.id, bob.id, "new occupant cannot inherit old ordnance ownership");
    carol.protection = 0;
    sim.damageTank(carol, 10000, alice.id, alice.team, alice.life);
    h.advance();
    assert.equal(carol.alive, false);
    h.action("carol", "leave");
    h.advance();
    assert.equal(sim.tanks.length, 1, "dead departed players cannot respawn");
    h.action("alice", "end");
    h.action("alice", "settings", { mapMode: "village", difficulty: "normal", humansOnly: false });
    h.action("alice", "start");
    assert.equal(h.host.simulation!.tanks.length, 12, "bots can be restored for the next round");
  } finally {
    h.host.dispose();
  }
});

test("humans-only removes expired reservations and accepts new occupants without growing the roster", () => {
  const h = harness();
  try {
    h.join("alice", { team: 0 });
    h.join("bob", { team: 1 });
    h.action("alice", "settings", { mapMode: "village", difficulty: "easy", humansOnly: true });
    h.action("alice", "start");
    const sim = h.host.simulation!;
    const bob = sim.tanks[1];
    h.host.disconnect("bob", h.now);
    for (let i = 0; i < 601; i++) {
      h.advance();
      for (const messages of h.messages.values())
        messages.splice(0, Math.max(0, messages.length - 12));
    }
    assert.equal(sim.tanks.length, 1);
    assert.equal(bob.body.isValid(), false);
    const bodies = sim.world.bodies.len();
    for (let i = 0; i < 10; i++) {
      const name = `late-${i}`;
      h.join(name, { team: 1 });
      assert.equal(sim.tanks.length, 2);
      assert.equal(sim.world.bodies.len(), bodies + 1);
      h.action(name, "leave");
      assert.equal(sim.tanks.length, 1);
      assert.equal(sim.world.bodies.len(), bodies);
    }
  } finally {
    h.host.dispose();
  }
});

for (const mapMode of ["village", "harbor", "quarry"] as const) {
  test(
    mapMode +
      ": full and field deltas round-trip through JSON, including destruction and late joins",
    () => {
      const sim = createMultiplayerSimulation(
        4242,
        [{ playerId: "one", name: "One", team: 0, slot: 0, kind: "balanced" }],
        { mapMode },
      );
      try {
        sim.start();
        const identity = { roomEpoch: "room", roundId: 1 };
        const stream = new StateStream(identity),
          mirror = new StateMirror();
        const full = stream.full(captureScene(sim), 0, 0);
        assert.ok(Buffer.byteLength(JSON.stringify(full)) < 160_000, "Full-state wire budget");
        mirror.applyFull(JSON.parse(JSON.stringify(full)), identity);
        let removed = false;
        for (let tick = 1; tick <= 180; tick++) {
          if (tick === 30)
            for (const cover of sim.covers
              .filter((cover) => cover.alive && cover.destructible)
              .slice(0, 8))
              sim.damageCover(cover, 10000, -1, 0);
          if (tick === 90) for (const fragment of sim.fragments) fragment.life = 0;
          sim.stepWith(new Map([[sim.human.id, { ...idleCommand(), moveX: 1, fire: true }]]));
          sim.events = [];
          if (tick % 3) continue;
          const scene = captureScene(sim),
            snap = stream.snapshot(scene, tick, [], []);
          assert.ok(
            Buffer.byteLength(JSON.stringify(snap)) < 128_000,
            "Burst snapshot wire budget",
          );
          removed ||= snap.removed.length > 0;
          assert.ok(mirror.applySnapshot(JSON.parse(JSON.stringify(snap))));
          assert.deepEqual(mirror.state, scene);
          assert.deepEqual(mirror.render(sim.human.id), projectScene(scene, sim.human.id));
          if (tick === 60 || tick === 93) {
            const late = new StateMirror();
            late.applyFull(JSON.parse(JSON.stringify(stream.full(scene, tick, 0))), identity);
            assert.deepEqual(late.render(sim.human.id), mirror.render(sim.human.id));
          }
        }
        assert.equal(removed, true);
        assert.ok(mirror.render(sim.human.id).covers.some((cover) => cover.maxHp === Infinity));
        assert.doesNotMatch(JSON.stringify(mirror.state), /"body"|"collider"|Infinity|NaN/);
      } finally {
        sim.dispose();
      }
    },
  );
}
test("mirror rejects corrupt or skipped deltas atomically and a full baseline repairs it", () => {
  const sim = createMultiplayerSimulation(4242, []);
  try {
    const identity = { roomEpoch: "r", roundId: 1 },
      state = captureScene(sim),
      stream = new StateStream(identity),
      mirror = new StateMirror();
    const full = stream.full(state, 0, 0);
    mirror.applyFull(full, identity);
    const before = JSON.stringify(mirror.state),
      bad: Snapshot = {
        ...stream.snapshot(state, 3, [], []),
        updates: [{ kind: "tanks", id: sim.tanks[0].id, set: { hp: "bad" } }],
      };
    assert.equal(mirror.applySnapshot(bad), undefined);
    assert.equal(JSON.stringify(mirror.state), before);
    assert.equal(mirror.needsFull, true);
    mirror.applyFull(full, identity);
    assert.equal(mirror.applySnapshot({ ...bad, seq: 9 }), undefined);
    assert.equal(JSON.stringify(mirror.state), before);
    mirror.applyFull(stream.full(state, 3, 0), identity);
    assert.equal(mirror.needsFull, false);
  } finally {
    sim.dispose();
  }
});
test("two seats drive independently, reconnect revokes the old socket, and host transfer persists", () => {
  const h = harness();
  try {
    h.join("alice", { kind: "scout", team: 0 });
    h.join("bob", { kind: "heavy", team: 1 });
    h.action("alice", "start");
    const token = h.latest("alice", "welcome").token;
    const first = h.latest("alice", "control"),
      second = h.latest("bob", "control");
    const sim = h.host.simulation!,
      a = sim.tanks.find((t) => t.id === first.tankId)!,
      b = sim.tanks.find((t) => t.id === second.tankId)!;
    for (const cover of sim.covers) if (cover.body.isValid()) sim.world.removeRigidBody(cover.body);
    sim.covers = [];
    sim.movableCovers = [];
    sim.coverByCollider.clear();
    sim.nav.rebuild([]);
    a.body.setTranslation({ x: -10, y: 0.65, z: 0 }, true);
    b.body.setTranslation({ x: 10, y: 0.65, z: 0 }, true);
    for (let seq = 1; seq <= 20; seq++) {
      for (const [client, control, moveZ] of [
        ["alice", first, 1],
        ["bob", second, -1],
      ] as const)
        h.action(client, "input", {
          controlEpoch: control.controlEpoch,
          seq,
          observedTick: h.host.tick,
          moveX: 0,
          moveZ,
          aim: { angle: 0 },
          fire: false,
          actions: [],
        });
      h.advance();
    }
    assert.ok(a.body.translation().z > 3);
    assert.ok(b.body.translation().z < -2);
    h.host.disconnect("alice", h.now);
    assert.equal(a.driver, "bot");
    assert.equal(h.latest("bob", "lobby").hostId, h.latest("bob", "welcome").playerId);
    h.join("alice-new", { token, roomEpoch: "test-room", team: 1, kind: "heavy" });
    assert.equal(a.kind, "scout");
    assert.equal(a.team, 0);
    assert.equal(a.driver, "human");
    assert.equal(h.latest("alice-new", "lobby").hostId, h.latest("bob", "welcome").playerId);
    h.join("alice-newer", { token, roomEpoch: "test-room" });
    assert.ok(h.closed.includes("alice-new"));
    assert.ok(h.latest("alice-newer", "control").controlEpoch > first.controlEpoch);
    h.action("alice-new", "input", {
      ...first,
      seq: 99,
      observedTick: h.host.tick,
      moveX: 1,
      moveZ: 0,
      fire: true,
      aim: { angle: 0 },
      actions: [],
    });
    assert.equal(a.driver, "human", "revoked socket cannot suspend the replacement socket");
  } finally {
    h.host.dispose();
  }
});
test("round/epoch boundaries, suspension, seat expiry, and empty room cleanup are bounded", () => {
  const h = harness();
  try {
    h.join("alice");
    h.action("alice", "start");
    const sim = h.host.simulation!,
      control = h.latest("alice", "control"),
      tank = sim.human;
    h.action("alice", "suspend");
    assert.equal(tank.driver, "bot");
    h.action("alice", "resume");
    assert.equal(tank.driver, "human");
    h.action("alice", "input", {
      roundId: 0,
      controlEpoch: control.controlEpoch,
      seq: 1,
      observedTick: 0,
      moveX: 1,
      moveZ: 0,
      aim: { angle: 0 },
      fire: true,
      actions: [{ type: "mine" }],
    });
    h.advance();
    assert.equal(tank.command.fire, false);
    assert.equal(sim.mines.length, 0);
    h.host.disconnect("alice", h.now);
    h.now += 30000;
    h.host.advance(h.now);
    assert.equal(h.host.disposed, true);
    assert.equal(h.host.simulation, undefined);
  } finally {
    h.host.dispose();
  }
});
test("a heartbeat in flight from the previous round cannot disconnect a player", () => {
  const h = harness();
  try {
    h.join("alice");
    h.action("alice", "start");
    for (let i = 0; i < 10; i++) h.advance();
    const oldTick = h.host.tick;
    h.action("alice", "end");
    h.action("alice", "start");
    h.action("alice", "ping", { roundId: 1, observedTick: oldTick, t: h.now });
    assert.equal(h.host.connections, 1);
    assert.deepEqual(h.closed, []);
    h.action("alice", "ping", { observedTick: 0, t: h.now });
    assert.equal(h.latest("alice", "pong").tick, 0);
  } finally {
    h.host.dispose();
  }
});
test("suspended clients receive no snapshot backlog and resume with a current baseline", () => {
  const h = harness();
  try {
    h.join("alice");
    h.action("alice", "start");
    h.action("alice", "suspend");
    for (let i = 0; i < 80; i++) h.advance();
    assert.equal(h.messages.get("alice")!.filter((m) => m.type === "snapshot").length, 0);
    h.action("alice", "resume");
    assert.equal(h.latest("alice", "full").tick, h.host.tick);
    h.advance();
    assert.equal(h.host.connections, 1);
    assert.ok(h.latest("alice", "snapshot"));
  } finally {
    h.host.dispose();
  }
});
test("late join starts a fresh chassis and score; departed ordnance cannot credit the newcomer", () => {
  const h = harness();
  try {
    h.join("alice", { team: 0 });
    h.join("bob", { team: 1 });
    h.action("alice", "start");
    const sim = h.host.simulation!,
      a = sim.tanks.find((t) => t.playerId === h.latest("alice", "welcome").playerId)!,
      oldLife = a.life;
    h.action("alice", "leave");
    h.join("carol", { team: 0, kind: "scout" });
    const c = sim.tanks.find((t) => t.playerId === h.latest("carol", "welcome").playerId)!;
    assert.equal(c.id, a.id);
    assert.ok(c.life > oldLife);
    assert.equal(c.kind, "scout");
    assert.equal(c.kills, 0);
    const victim = sim.tanks.find((t) => t.team === 1)!;
    victim.protection = 0;
    sim.damageTank(victim, 10000, c.id, c.team, oldLife);
    h.advance();
    assert.equal(c.kills, 0);
    assert.equal(c.xp, 0);
    h.action("bob", "end");
    const score = h.latest("bob", "lobby").scoreboard;
    assert.equal(score.find((p) => p.name === "alice")?.kills, 1);
    assert.equal(score.find((p) => p.name === "carol")?.kills, 0);
    h.action("bob", "start");
    assert.equal(h.host.roundId, 2);
    assert.equal(h.host.simulation!.tanks.length, 12);
  } finally {
    h.host.dispose();
  }
});
test("a full room, full team, invalid kind and incompatible build are rejected before authority", () => {
  const h = harness();
  try {
    h.join("bad-version", { version: 999 });
    assert.equal(h.latest("bad-version", "error").code, "incompatible");
    h.join("bad-kind", { kind: "humvee" });
    assert.ok(h.closed.includes("bad-kind"));
    for (let i = 0; i < 6; i++) h.join("p" + i, { team: 0 });
    h.join("full-team", { team: 0 });
    assert.equal(h.latest("full-team", "error").code, "team-full");
    h.join("p6", { team: 1 });
    h.join("p7", { team: 1 });
    h.join("ninth");
    assert.equal(h.latest("ninth", "error").code, "room-full");
    h.action("p0", "start");
    assert.equal(h.host.simulation!.tanks.filter((t) => t.human).length, 8);
  } finally {
    h.host.dispose();
  }
});
test("real host messages apply to mirrors and projectile traces survive an impact between snapshots", () => {
  const h = harness();
  try {
    h.join("alice");
    h.action("alice", "start");
    const full = h.latest("alice", "full") as FullState,
      mirror = new StateMirror();
    mirror.applyFull(full, full);
    const control = h.latest("alice", "control") as Control,
      sim = h.host.simulation!;
    const tank = sim.human;
    tank.protection = 0;
    tank.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    for (const cover of sim.covers) if (cover.body.isValid()) sim.world.removeRigidBody(cover.body);
    sim.covers = [];
    sim.movableCovers = [];
    sim.coverByCollider.clear();
    sim.nav.rebuild([]);
    sim.addCover({ kind: "concrete", x: 0, z: 4, w: 10, h: 4, d: 1, hp: 100, color: 0x999999 });
    h.action("alice", "input", {
      controlEpoch: control.controlEpoch,
      seq: 1,
      observedTick: 0,
      moveX: 0,
      moveZ: 0,
      aim: { angle: 0 },
      fire: true,
      actions: [],
    });
    h.advance();
    const snaps = h.latest("alice", "snapshot").snapshots;
    for (const snap of snaps) assert.ok(mirror.applySnapshot(snap));
    assert.deepEqual(mirror.state, captureScene(sim));
    assert.ok(
      snaps
        .flatMap((snap) => snap.traces)
        .some(
          (trace) =>
            trace.shot.owner === tank.id && !sim.shots.some((shot) => shot.id === trace.shot.id),
        ),
    );
    assert.ok(snaps.flatMap((snap) => snap.events).some((event) => event.event.type === "impact"));
  } finally {
    h.host.dispose();
  }
});
test("slow readers disconnect and overload terminates the room instead of skipping physics", () => {
  const slow = harness();
  try {
    slow.join("alice");
    slow.action("alice", "start");
    for (let i = 0; i < 70; i++) {
      slow.now += 50;
      slow.host.advance(slow.now);
    }
    assert.equal(slow.host.connections, 0);
    assert.ok(slow.closed.includes("alice"));
  } finally {
    slow.host.dispose();
  }
  const overloaded = harness();
  try {
    overloaded.join("alice");
    overloaded.action("alice", "start");
    overloaded.now = 1000;
    overloaded.host.advance(overloaded.now);
    assert.equal(overloaded.host.disposed, true);
    assert.equal(overloaded.latest("alice", "room-reset").reason, "overload");
  } finally {
    overloaded.host.dispose();
  }
});
test("resync skips events already included in its baseline and repeated rounds retain 12 slots", () => {
  const h = harness();
  try {
    h.join("alice");
    for (let round = 1; round <= 12; round++) {
      h.action("alice", "start");
      assert.equal(h.host.roundId, round);
      assert.equal(h.host.simulation!.tanks.length, 12);
      assert.equal(h.latest("alice", "control").controlEpoch, 1);
      h.advance();
      h.action("alice", "end");
    }
    const state = captureScene(h.host.simulation!),
      identity = { roomEpoch: "r", roundId: 1 };
    const stream = new StateStream(identity),
      mirror = new StateMirror();
    mirror.applyFull(stream.full(state, 0, 2), identity);
    const result = mirror.applySnapshot(
      stream.snapshot(
        state,
        3,
        [
          { eventId: 1, tick: 1, event: { type: "impact", x: 0, z: 0 } },
          { eventId: 2, tick: 2, event: { type: "impact", x: 0, z: 0 } },
          { eventId: 3, tick: 3, event: { type: "impact", x: 0, z: 0 } },
        ],
        [],
      ),
    );
    assert.deepEqual(
      result?.events.map((e) => e.eventId),
      [3],
    );
  } finally {
    h.host.dispose();
  }
});

test("Auto team balances human seats, honors explicit teams and excludes the player changing teams", () => {
  const h = harness();
  try {
    h.join("alice", { team: 1 });
    h.join("bob");
    h.join("carol");
    let players = h.latest("alice", "lobby").players;
    assert.deepEqual(
      players.map((player) => player.team),
      [1, 0, 0],
    );
    h.action("carol", "choose", { kind: "balanced", team: 1 });
    h.join("dave");
    players = h.latest("alice", "lobby").players;
    assert.deepEqual(
      players.map((player) => player.team),
      [1, 0, 1, 0],
    );
    h.action("alice", "choose", { kind: "balanced" });
    assert.equal(h.latest("alice", "lobby").players[0].team, 1);
  } finally {
    h.host.dispose();
  }
});
test("create starts a selected humans-only map immediately and subsequent players join the running battle", () => {
  const h = harness();
  try {
    const create = { mapMode: "quarry", difficulty: "normal", humansOnly: true };
    h.join("alice", { create });
    assert.equal(h.host.phase, "playing");
    assert.equal(h.host.settings.mapMode, "quarry");
    assert.equal(h.host.simulation!.tanks.length, 1);
    assert.equal(h.latest("alice", "full").roundId, 1);
    h.join("collision", { create });
    assert.equal(h.latest("collision", "error").code, "room-exists");
    h.join("bob", { existingRoom: true });
    assert.equal(h.host.simulation!.tanks.length, 2);
    assert.equal(h.latest("bob", "full").roundId, 1);
    assert.deepEqual(
      h.host.simulation!.tanks.map((tank) => tank.team),
      [0, 1],
    );
    assert.deepEqual(h.host.directoryEntry("ABCDEFGH"), {
      room: "ABCDEFGH",
      contentVersion: CONTENT_VERSION,
      ...create,
      roundMinutes: 10,
      players: 2,
      reserved: 2,
      phase: "playing",
      roundId: 1,
      time: 600,
      scores: [0, 0],
    });
    h.action("alice", "leave");
    assert.equal(h.host.disposed, false);
    h.action("bob", "leave");
    assert.equal(h.host.disposed, true);
    assert.equal(h.host.simulation, undefined);
    assert.equal(h.host.directoryEntry("ABCDEFGH").players, 0);
  } finally {
    h.host.dispose();
  }
});
test("a stale directory selection cannot recreate an empty room; a dropped connection retains its grace period", () => {
  const h = harness();
  try {
    h.join("stale", { existingRoom: true });
    assert.equal(h.latest("stale", "error").code, "room-gone");
    assert.equal(h.host.connections, 0);
    h.join("alice", { create: { mapMode: "harbor", difficulty: "easy", humansOnly: true } });
    const token = h.latest("alice", "welcome").token;
    h.host.disconnect("alice", 0);
    assert.equal(h.host.disposed, false);
    assert.equal(h.host.directoryEntry("ABCDEFGH").players, 0);
    assert.equal(h.host.directoryEntry("ABCDEFGH").reserved, 1);
    h.join("back", { token, roomEpoch: "test-room", existingRoom: true });
    assert.equal(h.host.connections, 1);
    assert.equal(h.host.roundId, 1);
  } finally {
    h.host.dispose();
  }
});

test("round length defaults to ten minutes, validates bounds and is controlled by the host between rounds", () => {
  const settings = { mapMode: "village", difficulty: "normal", humansOnly: true };
  assert.equal(settingsReader.read(settings).roundMinutes, 10);
  for (const roundMinutes of [0, 21, 1.5, null, "10", Infinity, NaN]) {
    assert.throws(() => settingsReader.read({ ...settings, roundMinutes }));
  }
  const h = harness();
  try {
    h.join("alice", { create: settings });
    h.join("bob");
    assert.equal(h.host.simulation!.match.time, 600);
    assert.equal(h.latest("bob", "lobby").settings.roundMinutes, 10);
    h.action("alice", "end");
    h.action("bob", "settings", { ...settings, roundMinutes: 1 });
    assert.equal(h.host.settings.roundMinutes, 10, "Guest cannot change the next round");
    h.join("bob", { token: h.latest("bob", "welcome").token, roomEpoch: "test-room" });
    h.action("alice", "settings", { ...settings, roundMinutes: 1 });
    h.action("alice", "start");
    assert.equal(h.host.simulation!.match.time, 60);
    assert.equal(h.latest("bob", "lobby").settings.roundMinutes, 1);
    h.host.simulation!.match.scores = [1, 0];
    for (let i = 0; i < 1201; i++) h.advance();
    assert.equal(h.host.phase, "results");
    assert.equal(h.host.simulation!.match.winner, 0);
    assert.equal(h.host.simulation!.match.time, 0);
    h.action("alice", "start");
    h.action("alice", "settings", { ...settings, roundMinutes: 20 });
    assert.equal(h.host.settings.roundMinutes, 1, "Cannot change a running match");
  } finally {
    h.host.dispose();
  }
});

test("live snapshots carry each player's authoritative kills and preserve them through respawn", () => {
  const h = harness();
  try {
    h.join("alice", {
      team: 0,
      create: { mapMode: "village", difficulty: "normal", humansOnly: true },
    });
    h.join("bob", { team: 1 });
    const sim = h.host.simulation!,
      alice = sim.tanks[0],
      bob = sim.tanks[1];
    const mirror = new StateMirror();
    mirror.applyFull(h.latest("alice", "full"), { roomEpoch: "test-room", roundId: 1 });
    bob.protection = 0;
    sim.damageTank(bob, 10000, alice.id, alice.team, alice.life);
    h.advance();
    for (const snapshot of h.latest("alice", "snapshot").snapshots) mirror.applySnapshot(snapshot);
    assert.equal(mirror.render(alice.id).viewer.kills, 1);
    assert.equal(mirror.render(bob.id).viewer.deaths, 1);
    sim.respawn(alice);
    h.advance();
    for (const snapshot of h.latest("alice", "snapshot").snapshots) mirror.applySnapshot(snapshot);
    assert.equal(mirror.render(alice.id).viewer.kills, 1);
    h.join("carol");
    assert.equal(
      h.latest("carol", "lobby").players.find((player) => player.name === "alice")!.kills,
      1,
    );
  } finally {
    h.host.dispose();
  }
});
