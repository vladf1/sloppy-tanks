// The first frames of a round on every map and for both teams, through the real game
// loop at chosen animation-frame timestamps: callbacks queued before the round began
// must neither run time backwards nor advance play, tanks appear at their physics
// spawns without overlapping the player, and stationary spawns leave no arrival tracks.
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import {
  chooseMap,
  freezeLoop,
  gameUrl as url,
  launchGame,
  seedGame,
  startRound,
} from "./browser-helpers.mjs";

const output = "artifacts/performance/startup";
mkdirSync(output, { recursive: true });
const { browser, context, errors } = await launchGame({
  viewport: { width: 1440, height: 900 },
  consoleErrors: true,
});
// The seed's first draw picks the player's team (424242 blue, 500000 red): cover both.
const runs = [
  ["village", 424242],
  ["harbor", 500000],
  ["quarry", 424242],
];
const results = [];
try {
  for (const [map, seed] of runs) {
    const page = await context.newPage();
    await seedGame(page, seed);
    // Prepare normally, then drive the real game loop with chosen RAF timestamps.
    await freezeLoop(page);
    await page.goto(url);
    await page.waitForFunction(
      () => document.querySelector("#startup-overlay")?.dataset.state === "ready",
    );
    await chooseMap(page, map);
    await page.evaluate(() => {
      window.beforeStart = performance.now();
    });
    // Other maps rebuild the arena after GO; replay the stale frames only once
    // the round is live, or the loop ignores them and nothing is checked.
    await startRound(page);
    assert.equal(await page.evaluate(() => window.sloppy.sim.seed), seed);
    const result = await page.evaluate(() => {
      const { sloppy, engine } = window;
      const frames = [];
      const frame = (timestamp) => {
        window.runLoop(timestamp);
        const state = engine.state();
        const view = engine.view();
        const models = new Map(view.tanks.map((tank) => [tank.id, tank]));
        frames.push({
          frames: sloppy.frames,
          time: state.view.time,
          elapsed: state.elapsed,
          tracks: view.effects.trackMarks,
          tanks: state.tanks
            .filter((tank) => tank.alive)
            .map((tank) => {
              const model = models.get(tank.id)?.position ?? [NaN, NaN, NaN];
              return {
                id: tank.id,
                human: tank.human,
                body: { x: tank.x, z: tank.z },
                model: { x: model[0], z: model[2] },
              };
            }),
        });
      };
      const counted = sloppy.frames;
      // Emulate callbacks queued before a slow arena rebuild, including a second
      // old timestamp: neither may undo the reset clock or advance gameplay.
      frame(window.beforeStart - 1000);
      frame(window.beforeStart - 900);
      const now = performance.now();
      for (let i = 0; i < 30; i++) frame(now + (i * 1000) / 120);
      return {
        map: sloppy.sim.mapMode,
        team: sloppy.sim.humanTeam,
        rendered: sloppy.frames - counted,
        frames,
      };
    });
    results.push(result);
    assert.equal(result.map, map);
    assert.equal(result.rendered, 32, `${map}: every replayed frame must run`);
    for (const frame of result.frames.slice(0, 2)) {
      assert.equal(frame.time, 0, `${map}: stale frame must not advance animation time`);
      assert.equal(frame.elapsed, 0, `${map}: stale frame must not advance simulation`);
      assert.equal(frame.tracks, 0, `${map}: stationary spawn must not draw arrival tracks`);
      for (const tank of frame.tanks) {
        assert.ok(
          Math.hypot(tank.body.x - tank.model.x, tank.body.z - tank.model.z) < 0.001,
          `${map}: tank ${tank.id} must appear at its physics spawn`,
        );
      }
      const human = frame.tanks.find((tank) => tank.human);
      for (const tank of frame.tanks.filter((tank) => !tank.human)) {
        assert.ok(
          Math.hypot(tank.model.x - human.model.x, tank.model.z - human.model.z) > 2,
          `${map}: another tank overlaps the player's spawn`,
        );
      }
    }
    let previous = 0;
    for (const frame of result.frames) {
      const dt = frame.time - previous;
      previous = frame.time;
      assert.ok(dt >= 0 && dt <= 0.1 + 1e-9, `${map}: frame duration ${dt} must stay bounded`);
      for (const tank of frame.tanks) {
        assert.ok(
          Math.hypot(tank.body.x - tank.model.x, tank.body.z - tank.model.z) < 0.5,
          `${map}: rendered tank must follow its physics body`,
        );
      }
    }
    assert.ok(result.frames.at(-1).elapsed > 0, `${map}: the later frames play`);
    await page.screenshot({ path: `${output}/map-start-${map}-${result.team}.png` });
    console.log(
      `${map}, team ${result.team}: ${result.frames.length} frames; no negative time, spawn overlap, or arrival trails`,
    );
    await page.close();
  }
  assert.deepEqual([...new Set(results.map((result) => result.team))].sort(), [0, 1]);
  assert.deepEqual(errors, []);
} finally {
  writeFileSync(
    `${output}/map-start-regression.json`,
    JSON.stringify({ results, errors }, null, 2),
  );
  await browser.close();
}
