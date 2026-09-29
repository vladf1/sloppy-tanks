// Generates initial-state.jsonl (one case per line), the TypeScript reference for the Rust port's construction
// parity test. Run from the repository root:
//   node --import tsx crates/core/tests/fixtures/initial-state.ts
import RAPIER from "@dimforge/rapier3d-simd-compat";
import { writeFileSync } from "node:fs";
import { Simulation, type SimulationSetup } from "../../../../src/game/simulation";
import { EXTRA_LEVELS } from "../../../../src/extra-levels";
import { singlePlayerRules } from "../../../../src/game/level-rules";
import { idleCommand } from "../../../../src/game/types";

await RAPIER.init();

const cases: { seed: number; label: string; setup: SimulationSetup }[] = [];
for (const seed of [12345, 237, 7]) {
  for (const mapMode of ["village", "harbor", "quarry"] as const) {
    cases.push({ seed, label: mapMode, setup: { mapMode } });
  }
  cases.push({ seed, label: "solo", setup: { gameMode: "solo" } });
  for (const level of ["stress-test", "superstress"] as const) {
    cases.push({
      seed,
      label: level,
      setup: { mapMode: level, ...singlePlayerRules(EXTRA_LEVELS[level]) },
    });
  }
}

const round = (value: number) => Math.round(value * 1e6) / 1e6;
const results = cases.map(({ seed, label, setup }) => {
  const s = new Simulation(seed, setup);
  const initial = {
    seed,
    label,
    humanTeam: s.humanTeam,
    rngState: s.rng.state,
    nextId: s.nextId,
    tanks: s.tanks.map((tank) => ({
      id: tank.id,
      name: tank.name,
      team: tank.team,
      kind: tank.kind,
      human: tank.human,
      hp: tank.hp,
      x: round(tank.previous.x),
      z: round(tank.previous.z),
      personality: tank.brain.personality,
      ultraAggressive: tank.brain.ultraAggressive,
    })),
    // Layouts do not depend on the seed, so one seed carries the cover detail.
    covers: (seed === 12345 ? s.covers : []).map((cover) => ({
      id: cover.id,
      kind: cover.kind,
      x: round(cover.x),
      z: round(cover.z),
      w: round(cover.w),
      d: round(cover.d),
      h: round(cover.h),
      hp: Number.isFinite(cover.hp) ? cover.hp : null,
    })),
    pickups: s.pickups.map((pickup) => ({
      id: pickup.id,
      kind: pickup.kind,
      x: round(pickup.x),
      z: round(pickup.z),
      available: pickup.available,
    })),
    navBlocked: [...s.nav.blocked].reduce((sum, cell) => sum + cell, 0),
    navHash: [...s.nav.blocked].reduce((hash, cell, i) => (hash * 31 + cell * (i + 1)) >>> 0, 7),
    rngAfter: [] as number[],
  };
  s.start();
  for (let tick = 1; tick <= 30; tick++) {
    s.step(idleCommand(), true);
    if (tick === 1 || tick === 5 || tick === 30) {
      initial.rngAfter.push(s.rng.state);
    }
  }
  s.dispose();
  return initial;
});
writeFileSync(
  new URL("./initial-state.jsonl", import.meta.url),
  results.map((result) => JSON.stringify(result) + "\n").join(""),
);
