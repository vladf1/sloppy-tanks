import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { seedGame, startRound } from "./browser-helpers.mjs";

const out = "artifacts/performance/multiplayer/seats";
await mkdir(out, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless: false });
const errors = [];
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  await seedGame(page, 4242);
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  await page.addInitScript(() => {
    let frame;
    const raf = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (callback) => {
      if (callback.name !== "loop") return raf(callback);
      frame = callback;
      return 1;
    };
    window.advanceFrame = () => frame(performance.now());
  });
  await page.goto(process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/");
  await startRound(page);
  const advance = () =>
    page.evaluate(() => {
      for (let i = 0; i < 4; i++) window.advanceFrame();
    });
  await page.evaluate(() => {
    const { sim } = window.sloppy;
    const victim = sim.tanks.find((tank) => !tank.human && tank.team !== sim.human.team);
    victim.name = "<img src=x onerror=alert(1)>";
    victim.protection = 0;
    sim.damageTank(victim, 10000, sim.human.id, sim.human.team, sim.human.life);
    for (let i = 0; i < 4; i++) window.advanceFrame();
  });
  assert.equal(await page.locator("#feed img").count(), 0);
  assert.match(await page.locator("#feed").innerText(), /YOU.*<img src=x onerror=alert\(1\)>/);
  await page.locator("#pause").click();
  await advance();
  await page.locator("#tank-speed").focus();
  await page.keyboard.press("Home");
  for (let i = 0; i < 20; i++) await page.keyboard.press("ArrowRight");
  await page.locator("#bullet-speed").focus();
  await page.keyboard.press("Home");
  const tuning = await page.evaluate(() => window.sloppy.sim.speedTuning);
  assert.deepEqual(tuning, { "tank-speed": 1.5, "bullet-speed": 0.5 });
  await page.locator("#resume").click();
  await advance();
  await page.locator("#pause").click();
  await advance();
  assert.equal(await page.locator("#tank-speed").inputValue(), "1.5");
  assert.equal(await page.locator("#bullet-speed").inputValue(), "0.5");

  const result = await page.evaluate(async () => {
    const { createMultiplayerSimulation } =
      await import("/sloppy-tanks/src/net/multiplayer-simulation.ts");
    const { captureRenderState } = await import("/sloppy-tanks/src/net/render-timeline.ts");
    const { PlayerControls } = await import("/sloppy-tanks/src/net/player-controls.ts");
    const sim = createMultiplayerSimulation(4242, [
      { playerId: "alice", name: "Alice", team: 0, slot: 0, kind: "scout" },
      { playerId: "bob", name: "Bob", team: 1, slot: 0, kind: "heavy" },
    ]);
    const tanks = sim.tanks.filter((tank) => tank.human);
    const controls = tanks.map((tank) => new PlayerControls(tank, 0));
    const starts = tanks.map((tank) => ({ ...tank.body.translation() }));
    sim.start();
    for (let tick = 1; tick <= 90; tick++) {
      const now = (tick * 1000) / 60;
      if (tick % 3 === 1)
        controls.forEach((control, i) =>
          control.accept(
            {
              controlEpoch: control.controlEpoch,
              seq: tick,
              observedTick: tick - 1,
              moveX: i === 0 ? 1 : -1,
              moveZ: 0,
              aim: { angle: i === 0 ? Math.PI / 2 : -Math.PI / 2 },
              fire: false,
              actions: [],
            },
            tick - 1,
            now,
          ),
        );
      sim.stepWith(
        new Map(controls.map((control) => [control.tank.id, control.command(tick, now)])),
      );
    }
    const before = JSON.stringify(sim.snapshot()),
      rng = sim.rng.state;
    const view = window.sloppy.view;
    const samples = tanks.map((tank) => captureRenderState(sim, tank.id));
    view.reset(samples[0]);
    await view.prepare(samples[0]);
    document.querySelectorAll("#overlay,#hud,#fps").forEach((el) => (el.style.display = "none"));
    window.seatFixture = {
      draw(index) {
        view.render(samples[index], 1, 1 / 60);
        return {
          viewerId: samples[index].viewerId,
          follow: { x: view.follow.x, z: view.follow.z },
          poses: tanks.map((tank) => ({
            id: tank.id,
            body: { ...tank.body.translation() },
            model: { ...view.tankMeshes.get(tank.id).position },
            barHeight: view.bars.get(tank.id).position.y,
          })),
          unchanged: JSON.stringify(sim.snapshot()) === before && sim.rng.state === rng,
          speedTuning: sim.speedTuning,
        };
      },
      dispose() {
        sim.dispose();
      },
    };
    return { starts, ends: tanks.map((tank) => ({ ...tank.body.translation() })) };
  });
  const views = [];
  for (let i = 0; i < 2; i++) {
    const view = await page.evaluate((i) => window.seatFixture.draw(i), i);
    assert.equal(view.unchanged, true);
    assert.deepEqual(view.speedTuning, { "tank-speed": 1, "bullet-speed": 1 });
    const own = view.poses.find((tank) => tank.id === view.viewerId);
    assert.ok(Math.hypot(own.body.x - view.follow.x, own.body.z - view.follow.z) < 1e-6);
    for (const tank of view.poses) {
      assert.ok(Math.hypot(tank.body.x - tank.model.x, tank.body.z - tank.model.z) < 1e-6);
      assert.equal(tank.barHeight, tank.id === view.viewerId ? 2.85 : 2.15);
    }
    assert.ok(Math.abs(result.ends[i].x - result.starts[i].x) > 1);
    await page.screenshot({ path: `${out}/viewer-${i}.png` });
    views.push(view);
  }
  await page.evaluate(() => window.seatFixture.dispose());
  assert.deepEqual(errors, []);
  await writeFile(`${out}/browser.json`, JSON.stringify({ result, views, errors }, null, 2));
  console.log(
    "Two independently controlled tanks and plain render-state viewers passed; tuning isolation and literal player-name feed passed.",
  );
} finally {
  await browser.close();
}
