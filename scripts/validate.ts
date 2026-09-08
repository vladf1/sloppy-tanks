import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { idleCommand } from "../src/game/types";
import { writeFileSync } from "node:fs";
await RAPIER.init();
const rounds = [];
let maxBodies = 0,
  maxFragments = 0;
const started = performance.now();
for (let seed = 1; seed <= 10; seed++) {
  const s = new Simulation(seed * 79);
  const initialBodies = s.world.bodies.len();
  s.start();
  let steps = 0;
  const veterancy = { promotions: 0, botPromotions: 0, elite: 0, heroic: 0 };
  while (s.match.phase === "playing" && steps < 60 * 360) {
    s.step(idleCommand(), seed > 3);
    for (const e of s.events.splice(0)) if (e.type === "promotion") {
      veterancy.promotions++;
      if (e.id !== s.human.id) veterancy.botPromotions++;
      if (e.label === "PROMOTED TO ELITE") veterancy.elite++;
      if (e.label === "PROMOTED TO HEROIC") veterancy.heroic++;
    }
    maxBodies = Math.max(maxBodies, s.world.bodies.len());
    maxFragments = Math.max(maxFragments, s.fragments.length);
    steps++;
  }
  rounds.push({
    seed: s.seed,
    humanIdle: seed <= 3,
    seconds: steps / 60,
    scores: s.match.scores,
    winner: s.match.winner,
    destroyed: s.destroyed,
    reroutes: s.botReroutes,
    breachShots: s.botBreachShots,
    veterancy,
    towersRemaining: s.covers.filter((c) => c.kind === "tower" && c.alive)
      .length,
  });
  s.reset();
  if (s.world.bodies.len() !== initialBodies)
    throw new Error(`Reset count ${s.world.bodies.len()}`);
  s.dispose();
  console.log(JSON.stringify(rounds.at(-1)));
}
const result = {
  type: "accelerated simulation, no rendering",
  wallSeconds: (performance.now() - started) / 1000,
  rounds,
  maxBodies,
  maxFragments,
};
writeFileSync(
  "artifacts/simulation-results.json",
  JSON.stringify(result, null, 2),
);
