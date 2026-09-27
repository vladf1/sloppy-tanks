import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { renderState } from "../src/game/render-state";
import type { Simulation } from "../src/game/simulation";
import { DEBRIS_CLEANUP_SECONDS } from "../src/game/debris-cleanup";
import { idleCommand } from "../src/game/types";
import { createMultiplayerSimulation } from "../src/net/multiplayer-simulation";
import { StateStream, type FieldChanges } from "../src/net/replication";
import {
  captureScene,
  ENTITY_TYPES,
  rounded,
  sceneReader,
  type Scene,
} from "../src/net/scene-codec";

before(async () => {
  await RAPIER.init();
});

/** The wire scene defined by the schema: readers select and order the fields, then the
 * generic pass rounds each number by its field name. The server's capture must match it. */
function referenceScene(sim: Simulation): Scene {
  const view = renderState(sim, undefined, sim.tanks[0].id);
  return rounded(
    sceneReader.read({
      entities: {
        tanks: view.tanks,
        covers: view.covers.map((cover) => ({
          ...cover,
          hp: Number.isFinite(cover.hp) ? cover.hp : null,
          maxHp: Number.isFinite(cover.maxHp) ? cover.maxHp : null,
        })),
        fragments: view.fragments.map((fragment) => ({
          ...fragment,
          life: Math.min(fragment.life, DEBRIS_CLEANUP_SECONDS),
        })),
        shots: view.shots,
        mines: view.mines,
        pickups: view.pickups,
      },
      elapsed: view.elapsed,
      match: view.match,
      map: {
        theme: view.mapTheme,
        floor: view.mapFloor,
        outerFloor: view.mapOuterFloor,
        outerFloorExtent: view.mapOuterFloorExtent,
        scale: view.mapScale === 1 ? undefined : view.mapScale,
      },
    }),
  );
}
/** Fields whose JSON differs, in current-then-deleted order; deletions are null. */
function referenceChanges(previous: object | undefined, next: object): FieldChanges | undefined {
  const before = previous as Record<string, unknown> | undefined;
  const current = next as Record<string, unknown>;
  const changes: FieldChanges = {};
  for (const field of new Set([...Object.keys(current), ...Object.keys(before ?? {})])) {
    if (!before || JSON.stringify(before[field]) !== JSON.stringify(current[field])) {
      changes[field] = current[field] ?? null;
    }
  }
  return Object.keys(changes).length ? changes : undefined;
}
function referenceDelta(previous: Scene, next: Scene) {
  const delta: {
    match?: FieldChanges;
    updates?: Record<string, Record<string, FieldChanges>>;
    removed?: Record<string, number[]>;
  } = {};
  const match = referenceChanges(previous.match, next.match);
  if (match) delta.match = match;
  for (const kind of ENTITY_TYPES) {
    const earlier = new Map<number, object>(
      previous.entities[kind].map((entity) => [entity.id, entity]),
    );
    for (const entity of next.entities[kind]) {
      const changes = referenceChanges(earlier.get(entity.id), entity);
      if (changes) ((delta.updates ??= {})[kind] ??= {})[entity.id] = changes;
    }
    const ids = new Set(next.entities[kind].map((entity) => entity.id));
    for (const id of earlier.keys())
      if (!ids.has(id)) ((delta.removed ??= {})[kind] ??= []).push(id);
  }
  return delta;
}
/** Optional wire fields the seeded matches must exercise, so the comparison covers them. */
function seenFields(scene: Scene, seen: Set<string>) {
  for (const kind of ENTITY_TYPES)
    for (const entity of scene.entities[kind])
      for (const field of Object.keys(entity)) seen.add(kind + "." + field);
  for (const cover of scene.entities.covers) if (cover.hp === null) seen.add("covers.hp=null");
}

const ROOMS = [
  { mapMode: "village", ticks: 450 },
  { mapMode: "harbor", ticks: 450 },
  { mapMode: "quarry", ticks: 450 },
  { scenario: "superstress", ticks: 300 },
] as const;
const EXPECTED_FIELDS = [
  "covers.debrisSeed",
  "covers.timberHits",
  "covers.timberJoin",
  "covers.motion",
  "covers.hp=null",
  "fragments.shape",
  "fragments.dimensions",
  "fragments.material",
  "fragments.sourceKind",
  "fragments.timberPart",
  "fragments.createdAt",
  "fragments.wreck",
  "fragments.part",
  "fragments.team",
  "shots.y",
  "shots.visualY",
  "mines.ownerLife",
  "pickups.cooldownDuration",
];

test("captured scenes and field deltas match the schema reference byte for byte", () => {
  const seen = new Set<string>();
  let removals = 0;
  for (const room of ROOMS) {
    const name = "scenario" in room ? room.scenario : room.mapMode;
    const sim = createMultiplayerSimulation(
      4242,
      [{ playerId: "one", name: "One", team: 0, slot: 0, kind: "balanced" }],
      "mapMode" in room ? { mapMode: room.mapMode } : {},
      "scenario" in room ? room.scenario : undefined,
    );
    try {
      sim.start();
      const stream = new StateStream({ roomEpoch: "room", roundId: 1 });
      let previous = captureScene(sim);
      assert.equal(JSON.stringify(previous), JSON.stringify(referenceScene(sim)), name);
      stream.full(previous, 0, 0);
      for (let tick = 1; tick <= room.ticks; tick++) {
        if (tick === 30) {
          // Collapsed towers leave seeded rubble; the rest leave debris and removals.
          const destructible = sim.covers.filter((cover) => cover.alive && cover.destructible);
          for (const cover of [
            ...destructible.filter((cover) => cover.kind === "tower"),
            ...destructible.slice(0, 12),
          ])
            if (cover.alive) sim.damageCover(cover, 10000, -1, 0);
        }
        const command = {
          ...idleCommand(),
          moveX: Math.sin(tick / 40),
          moveZ: Math.cos(tick / 55),
          aim: tick / 25,
          fire: true,
          mine: tick % 60 === 0,
        };
        sim.stepWith(new Map([[sim.human.id, command]]));
        sim.events = [];
        if (tick % 3) continue;
        const scene = captureScene(sim);
        const text = JSON.stringify(scene);
        assert.equal(text, JSON.stringify(referenceScene(sim)), `${name} scene at tick ${tick}`);
        const { match, updates, removed } = stream.snapshot(scene, tick, [], []);
        assert.equal(
          JSON.stringify({ match, updates, removed }),
          JSON.stringify(referenceDelta(previous, JSON.parse(text) as Scene)),
          `${name} delta at tick ${tick}`,
        );
        removals += Object.values(removed ?? {}).flat().length;
        seenFields(scene, seen);
        previous = JSON.parse(text) as Scene;
      }
    } finally {
      sim.dispose();
    }
  }
  assert.ok(removals > 0, "Seeded matches remove entities");
  assert.deepEqual(
    EXPECTED_FIELDS.filter((field) => !seen.has(field)),
    [],
    "Seeded matches exercise optional wire fields",
  );
});
