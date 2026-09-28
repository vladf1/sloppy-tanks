import RAPIER from "@dimforge/rapier3d-simd-compat";
import { EXTRA_LEVELS } from "../src/extra-levels";
import { STANDARD_RULES, singlePlayerRules } from "../src/game/level-rules";
import type { MapId } from "../src/game/map-options";
import { Simulation, type SimulationSetup } from "../src/game/simulation";
import { idleCommand } from "../src/game/types";

// Headless seeded autoplay: per-tick wall time and the share spent inside World.step.
// Usage: node --import tsx scripts/simulation-benchmark.ts <map> <seed> [count]
// "count" tallies rapier.js calls per tick instead; the wrapping distorts timing.
const [map = "village", seedText = "79", mode = "time"] = process.argv.slice(2);
const WARMUP_TICKS = 600;
const MEASURE_TICKS = 3600;

await RAPIER.init();

const calls = new Map<string, number>();
if (mode === "count") {
  for (const [name, value] of Object.entries(RAPIER)) {
    if (typeof value !== "function" || !value.prototype) continue;
    const prototype = value.prototype as Record<string, unknown>;
    for (const key of Object.getOwnPropertyNames(prototype)) {
      const method = Object.getOwnPropertyDescriptor(prototype, key)?.value as unknown;
      if (key === "constructor" || typeof method !== "function") continue;
      const label = `${name}.${key}`;
      prototype[key] = function (this: unknown, ...args: unknown[]) {
        calls.set(label, (calls.get(label) ?? 0) + 1);
        return (method as (...args: unknown[]) => unknown).apply(this, args);
      };
    }
  }
}

let worldStepMs = 0;
if (mode === "time") {
  const step = RAPIER.World.prototype.step;
  RAPIER.World.prototype.step = function (...args: Parameters<typeof step>) {
    const start = performance.now();
    step.apply(this, args);
    worldStepMs += performance.now() - start;
  };
}

const level = EXTRA_LEVELS[map as keyof typeof EXTRA_LEVELS];
const setup: SimulationSetup = {
  ...(level ? singlePlayerRules(level) : STANDARD_RULES),
  mapMode: map as MapId,
};
const simulation = new Simulation(Number(seedText), setup);
simulation.start();
const command = idleCommand();
const tickMs: number[] = [];
const stepMs: number[] = [];
let maxBodies = 0;
let maxFragments = 0;
for (let tick = 0; tick < WARMUP_TICKS + MEASURE_TICKS; tick++) {
  if (simulation.match.phase !== "playing") break;
  if (tick === WARMUP_TICKS) calls.clear();
  const stepBefore = worldStepMs;
  const start = performance.now();
  simulation.step(command, true);
  const elapsed = performance.now() - start;
  simulation.events.length = 0;
  if (tick < WARMUP_TICKS) continue;
  tickMs.push(elapsed);
  stepMs.push(worldStepMs - stepBefore);
  maxBodies = Math.max(maxBodies, simulation.world.bodies.len());
  maxFragments = Math.max(maxFragments, simulation.fragments.length);
}
simulation.dispose();

const ticks = tickMs.length;
const sorted = [...tickMs].sort((a, b) => a - b);
const percentile = (p: number) => sorted[Math.min(ticks - 1, Math.floor(p * ticks))];
const sum = (values: number[]) => values.reduce((total, value) => total + value, 0);
console.log(
  JSON.stringify({
    map,
    seed: Number(seedText),
    ticks,
    tanks: simulation.tanks.length,
    maxBodies,
    maxFragments,
    ...(mode === "count"
      ? {
          callsPerTick: sum([...calls.values()]) / ticks,
          topCalls: [...calls]
            .sort((a, b) => b[1] - a[1])
            .slice(0, 25)
            .map(([name, count]) => [name, Number((count / ticks).toFixed(1))]),
        }
      : {
          meanMs: sum(tickMs) / ticks,
          p50: percentile(0.5),
          p95: percentile(0.95),
          p99: percentile(0.99),
          max: sorted.at(-1),
          worldStepMeanMs: sum(stepMs) / ticks,
        }),
  }),
);
