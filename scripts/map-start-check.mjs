import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import {
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
    await page.locator(`input[name="mapMode"][value="${map}"]`).check();
    await page.evaluate(() => {
      window.beforeStart = performance.now();
      window.startFrames = [];
      const render = window.sloppy.view.render.bind(window.sloppy.view);
      window.sloppy.view.render = (sim, alpha, dt, overview) => {
        render(sim, alpha, dt, overview);
        // Warm-up passes the read-only view; compare against actual physics in both paths.
        const authority = window.sloppy.sim;
        window.startFrames.push({
          alpha,
          dt,
          elapsed: authority.elapsed,
          tracks: window.sloppy.view.tracks.mesh.count,
          tanks: authority.tanks.map((tank) => {
            const body = tank.body.translation();
            const model = window.sloppy.view.tankMeshes.get(tank.id).position;
            return {
              id: tank.id,
              human: tank.human,
              body: { x: body.x, z: body.z },
              model: { x: model.x, z: model.z },
            };
          }),
        });
      };
    });
    // Other maps rebuild the arena after GO; replay the stale frames only once
    // the round is live, or the loop ignores them and nothing is checked.
    await startRound(page);
    assert.equal(await page.evaluate(() => window.sloppy.sim.seed), seed);
    const result = await page.evaluate(() => {
      window.startFrames.length = 0; // Discard the preparation renders.
      // Emulate callbacks queued before a slow arena rebuild, including a second
      // old timestamp: neither may undo the reset clock or advance gameplay.
      window.runLoop(window.beforeStart - 1000);
      window.runLoop(window.beforeStart - 900);
      const now = performance.now();
      for (let i = 0; i < 30; i++) window.runLoop(now + (i * 1000) / 120);
      return {
        map: window.sloppy.sim.mapMode,
        team: window.sloppy.sim.humanTeam,
        frames: window.startFrames,
      };
    });
    results.push(result);
    assert.equal(result.map, map);
    assert.equal(result.frames.length, 32, `${map}: every replayed frame must render`);
    for (const frame of result.frames.slice(0, 2)) {
      assert.equal(frame.dt, 0, `${map}: stale frame must not reverse animation time`);
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
    for (const frame of result.frames) {
      assert.ok(
        frame.alpha >= 0 && frame.alpha <= 1,
        `${map}: interpolation ${frame.alpha} must remain bounded`,
      );
      assert.ok(frame.dt >= 0 && frame.dt <= 0.1, `${map}: frame duration must remain bounded`);
      for (const tank of frame.tanks) {
        assert.ok(
          Math.hypot(tank.body.x - tank.model.x, tank.body.z - tank.model.z) < 0.5,
          `${map}: rendered tank must follow its physics body`,
        );
      }
    }
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
