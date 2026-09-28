import RAPIER from "@dimforge/rapier3d-compat";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { createMultiplayerSimulation } from "../src/net/multiplayer-simulation";
import { StateStream } from "../src/net/replication";
import { captureScene } from "../src/net/scene-codec";

// Per-room server work for one 50 ms host interval: three fixed steps, then the capture,
// field diff and serialization MatchHost.captureFrame and broadcastSnapshot perform.
const WARMUP_TICKS = 1200;
const INTERVALS = 400;
const STEPS_PER_INTERVAL = 3;
const SEED = 4242;
const ROOMS = [
  { name: "village", mapMode: "village" },
  { name: "harbor", mapMode: "harbor" },
  { name: "quarry", mapMode: "quarry" },
  { name: "stress-grid", mapMode: "stress-test" },
  { name: "scrap-yard", mapMode: "superstress" },
] as const;
const STAGES = ["physics", "capture", "diff", "stringify"] as const;

const output = process.argv[2] ?? "artifacts/performance/capture-benchmark.json";
await RAPIER.init();

function summary(samples: number[]) {
  const sorted = [...samples].sort((a, b) => a - b);
  const at = (fraction: number) =>
    sorted[Math.min(sorted.length - 1, Math.floor(fraction * sorted.length))];
  const round = (value: number) => Math.round(value * 1000) / 1000;
  return {
    n: samples.length,
    mean: round(samples.reduce((sum, value) => sum + value, 0) / samples.length),
    p50: round(at(0.5)),
    p95: round(at(0.95)),
    max: round(sorted.at(-1)!),
  };
}

const results: Record<string, Record<string, ReturnType<typeof summary>>> = {};
for (const room of ROOMS) {
  const sim = createMultiplayerSimulation(
    SEED,
    [{ playerId: "one", name: "One", team: 0, slot: 0, kind: "balanced" }],
    { mapMode: room.mapMode },
  );
  sim.start();
  const idle = new Map();
  for (let tick = 0; tick < WARMUP_TICKS; tick++) {
    sim.stepWith(idle);
    sim.events = [];
  }
  const stream = new StateStream({ roomEpoch: "benchmark", roundId: 1 });
  stream.full(captureScene(sim), WARMUP_TICKS, 0);
  const samples = Object.fromEntries(STAGES.map((stage) => [stage, [] as number[]]));
  let tick = WARMUP_TICKS;
  let bytes = 0;
  for (let interval = 0; interval < INTERVALS; interval++) {
    let start = performance.now();
    for (let step = 0; step < STEPS_PER_INTERVAL; step++) {
      sim.stepWith(idle);
      sim.events = [];
      tick++;
    }
    let end = performance.now();
    samples.physics.push(end - start);
    start = end;
    const scene = captureScene(sim);
    end = performance.now();
    samples.capture.push(end - start);
    start = end;
    const frame = stream.snapshot(scene, tick, [], []);
    end = performance.now();
    samples.diff.push(end - start);
    start = end;
    bytes += JSON.stringify(frame).length;
    samples.stringify.push(performance.now() - start);
  }
  const counts = `${sim.tanks.length} tanks, ${sim.covers.length} covers, ${sim.fragments.length} fragments`;
  sim.dispose();
  results[room.name] = Object.fromEntries(STAGES.map((stage) => [stage, summary(samples[stage])]));
  const total = STAGES.slice(1).map((stage) => results[room.name][stage].mean);
  console.log(
    `${room.name} (${counts}; ${INTERVALS} intervals, ${Math.round(bytes / INTERVALS)} B/frame)`,
  );
  for (const stage of STAGES) {
    const s = results[room.name][stage];
    console.log(`  ${stage.padEnd(9)} mean ${s.mean} ms  p50 ${s.p50}  p95 ${s.p95}  max ${s.max}`);
  }
  console.log(`  capture+diff+stringify mean ${total.reduce((a, b) => a + b, 0).toFixed(3)} ms`);
}
mkdirSync(dirname(output), { recursive: true });
writeFileSync(
  output,
  JSON.stringify(
    { node: process.version, date: new Date().toISOString(), WARMUP_TICKS, INTERVALS, results },
    null,
    2,
  ),
);
console.log("Wrote " + output);
