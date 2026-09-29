// The TypeScript twin of `crates/core/examples/trace.rs`: a per-tick trace of a seeded match.
// Diff the two outputs to find the first tick where the engines differ:
//   node --import tsx crates/core/tests/fixtures/trace.ts <seed> <map|solo|mp> <ticks>
import RAPIER from "@dimforge/rapier3d-simd-compat";
import { EXTRA_LEVELS } from "../../../../src/extra-levels";
import { singlePlayerRules } from "../../../../src/game/level-rules";
import type { MapId } from "../../../../src/game/map-options";
import { Simulation, type SimulationSetup } from "../../../../src/game/simulation";
import { idleCommand, type PlayerAssignment } from "../../../../src/game/types";

await RAPIER.init();
const seed = Number(process.argv[2] ?? 12345);
const map = process.argv[3] ?? "village";
const ticks = Number(process.argv[4] ?? 600);
// Two seats: a heavy driving north and firing every 50 ticks, and an idle scout.
const players: PlayerAssignment[] = [
  { playerId: "a", name: "ALPHA", team: 0, slot: 1, kind: "heavy" },
  { playerId: "b", name: "BRAVO", team: 1, slot: 0, kind: "scout" },
];
const setup: SimulationSetup =
  map === "mp"
    ? { gameMode: "team", roundCount: 12, players }
    : map === "solo"
      ? { gameMode: "solo" }
      : map === "stress-test" || map === "superstress"
        ? { mapMode: map, ...singlePlayerRules(EXTRA_LEVELS[map]) }
        : { mapMode: map as MapId };
const s = new Simulation(seed, setup);
s.start();
const fixed = (value: number) => value.toFixed(3);
for (let tick = 1; tick <= ticks; tick++) {
  if (map === "mp") {
    const seat = s.tanks.find((tank) => tank.playerId === "a")!;
    s.stepWith(new Map([[seat.id, { ...idleCommand(), moveZ: 1, fire: tick % 50 === 0 }]]));
  } else {
    s.step(idleCommand(), true);
  }
  const tanks = s.tanks
    .map((tank) =>
      tank.alive
        ? `${fixed(tank.body.translation().x)},${fixed(tank.body.translation().z)}`
        : "dead",
    )
    .join(" ");
  console.log(
    `${tick} rng=${s.rng.state} shots=${s.shots.length} frags=${s.fragments.length} destroyed=${s.destroyed} ${tanks}`,
  );
}
s.dispose();
