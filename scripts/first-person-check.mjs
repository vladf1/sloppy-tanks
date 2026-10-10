import { click, gameUrl, launchGame, startRound } from "./browser-helpers.mjs";
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
const output = "artifacts/performance/first-person";
mkdirSync(output, { recursive: true });
const { browser, page, errors } = await launchGame();
const turn = (from, to) => Math.atan2(Math.sin(to - from), Math.cos(to - from));
// Hold the game's `loop` animation callback on demand, so a check can act between
// two engine frames.
await page.addInitScript(() => {
  const requestFrame = window.requestAnimationFrame.bind(window);
  let held;
  window.holdLoop = () => (held ??= []);
  window.releaseLoop = () => {
    const callbacks = held ?? [];
    held = undefined;
    for (const callback of callbacks) requestFrame(callback);
  };
  window.requestAnimationFrame = (callback) => {
    if (held && callback.name === "loop") {
      held.push(callback);
      return 0;
    }
    return requestFrame(callback);
  };
});
try {
  await page.goto(gameUrl, { waitUntil: "domcontentloaded", timeout: 60000 });
  await page.waitForFunction(() => !!window.sloppy);
  await page.locator('[data-kind="balanced"]').click();
  await startRound(page);
  await page.mouse.move(800, 300);
  await page.waitForTimeout(300);
  const overheadAim = await page.evaluate(() => window.sloppy.sim.human.aim);

  await page.keyboard.press("v");
  // The camera flies down into the turret before the cockpit HUD appears.
  await page.waitForFunction(() => {
    const { view } = window.sloppy;
    return !view.inFirstPerson && view.camera.fov > 43 && view.camera.position[1] > 4;
  });
  await page.screenshot({ path: `${output}/entering.png` });
  await page.waitForFunction(() => window.sloppy.view.inFirstPerson);
  const seated = await page.evaluate(() => {
    const { sim, view } = window.sloppy;
    const [x, y, z] = view.camera.position;
    const tank = sim.human;
    return {
      yaw: view.firstPerson.yaw,
      eyeHeight: y,
      eyeDistance: Math.hypot(x - tank.x, z - tank.z),
      hud: document.querySelector("#hud").classList.contains("first-person"),
      pressed: document.querySelector("#view-mode").getAttribute("aria-pressed"),
      ownModel: window.engine.view().tanks.find((view) => view.id === tank.id).shown,
    };
  });
  assert.ok(Math.abs(turn(overheadAim, seated.yaw)) < 0.05, "entering keeps the turret aim");
  assert.ok(seated.eyeHeight > 1 && seated.eyeHeight < 3, `turret-top eye: ${seated.eyeHeight}`);
  assert.ok(seated.eyeDistance < 1.5, `eye rides the player's tank: ${seated.eyeDistance}`);
  assert.deepEqual([seated.hud, seated.pressed, seated.ownModel], [true, "true", true]);
  await page.screenshot({ path: `${output}/entered.png` });

  // Physical mouse travel to the right turns the view and turret right (lower yaw).
  for (let x = 820; x <= 1000; x += 20) {
    await page.mouse.move(x, 300);
    await page.waitForTimeout(16);
  }
  await page.waitForTimeout(200);
  const turned = await page.evaluate(() => ({
    yaw: window.sloppy.view.firstPerson.yaw,
    aim: window.sloppy.sim.human.aim,
  }));
  assert.ok(turn(seated.yaw, turned.yaw) < -0.3, `mouse right turns right: ${turned.yaw}`);
  assert.ok(Math.abs(turn(turned.yaw, turned.aim)) < 0.05, "the turret follows the view");

  // W drives where the turret looks, not toward the top of the screen.
  const start = await page.evaluate(() => window.sloppy.sim.human);
  await page.keyboard.down("w");
  await page.waitForTimeout(1500);
  await page.keyboard.up("w");
  const end = await page.evaluate(() => window.sloppy.sim.human);
  const moved = Math.hypot(end.x - start.x, end.z - start.z);
  const along =
    ((end.x - start.x) * Math.sin(turned.yaw) + (end.z - start.z) * Math.cos(turned.yaw)) / moved;
  assert.ok(moved > 1 && along > 0.8, `W follows the view: ${JSON.stringify({ moved, along })}`);
  await page.screenshot({ path: `${output}/driving.png` });

  const captured = () =>
    page.evaluate(() => document.pointerLockElement === document.querySelector("#game"));
  // A physical press fires, and captures the pointer where the browser allows it.
  await page.mouse.move(800, 450);
  await page.mouse.down();
  await page.waitForFunction(() => window.sloppy.sim.human.cooldown > 0);
  await page.mouse.up();
  const locked = await captured();

  const phase = () => page.evaluate(() => window.sloppy.sim.match.phase);
  const freed = () =>
    page.waitForFunction(() => document.pointerLockElement === null, null, { timeout: 2000 });

  // Esc first frees the cursor and keeps the round running; Esc again opens the menu.
  await page.keyboard.press("Escape");
  await freed();
  assert.equal(await phase(), "playing");
  if (locked) {
    // The HUD shows the hint on its next frame.
    await page.locator("#cockpit .aim-hint").waitFor({ state: "visible", timeout: 2000 });
    const yaw = await page.evaluate(() => window.sloppy.view.firstPerson.yaw);
    await page.mouse.move(1100, 300, { steps: 5 });
    await page.waitForTimeout(100);
    const after = await page.evaluate(() => window.sloppy.view.firstPerson.yaw);
    assert.equal(after, yaw, "a free cursor reaches the HUD without turning the view");
    await page.screenshot({ path: `${output}/cursor-free.png` });
  }
  // Past the window in which a browser's own Esc release and key count as one press.
  await page.waitForTimeout(300);
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => window.sloppy.sim.match.phase === "paused");
  // RESUME hides the pause menu and wants the pointer again at once: a physical click
  // on the arena before the next engine frame takes it back and does not fire.
  await page.evaluate(async () => {
    window.holdLoop();
    // The frame already requested runs once more before the loop is held.
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  });
  const heldFrames = await page.evaluate(() => window.sloppy.frames);
  await click(page, "#resume");
  assert.equal(await phase(), "playing");
  await page.mouse.move(800, 450);
  await page.mouse.down();
  const fired = await page.evaluate(() => window.sloppy.controls.fire);
  await page.mouse.up();
  if (locked) {
    await page.waitForFunction(
      () => document.pointerLockElement === document.querySelector("#game"),
    );
    assert.equal(fired, false, "the click that takes the pointer back does not fire");
  }
  assert.equal(
    await page.evaluate(() => window.sloppy.frames),
    heldFrames,
    "the click after RESUME lands before the next engine frame",
  );
  await page.evaluate(() => window.releaseLoop());

  // A death keeps the pointer captured, so the respawn needs no click or new lock notice.
  const destroy = () =>
    page.evaluate(() => {
      window.engine.setHuman({ shieldPoints: 0 });
      window.sloppy.killHuman();
    });
  const heavy = page.locator("#overlay .respawn [data-kind='heavy']");
  await destroy();
  await heavy.waitFor({ state: "visible" });
  // Destroyed, the view stays in the turret instead of flying overhead and back.
  assert.deepEqual(
    await page.evaluate(() => [window.sloppy.view.inFirstPerson, window.sloppy.view.camera.fov]),
    [true, 58],
    "a destroyed player keeps the first-person view",
  );
  await page.screenshot({ path: `${output}/destroyed.png` });
  await page.waitForFunction(() => window.sloppy.sim.human.alive, null, { timeout: 10000 });
  await page.waitForFunction(() => window.sloppy.view.inFirstPerson, null, { timeout: 2000 });
  assert.deepEqual(
    [await captured(), await page.evaluate(() => window.sloppy.view.inFirstPerson)],
    [locked, true],
    "the respawn is back in first person with the pointer still captured",
  );

  // Esc while destroyed frees the cursor without pausing, so a physical click picks a tank.
  await destroy();
  await heavy.waitFor({ state: "visible" });
  await page.keyboard.press("Escape");
  await freed();
  assert.equal(await phase(), "playing");
  await page.screenshot({ path: `${output}/respawn.png` });
  await click(page, heavy);
  assert.equal(await page.evaluate(() => window.sloppy.sim.humanKind), "heavy");
  await page.waitForFunction(() => window.sloppy.sim.human.alive, null, { timeout: 10000 });
  await page.waitForFunction(() => window.sloppy.view.inFirstPerson, null, { timeout: 2000 });
  const respawned = await page.evaluate(() => ({
    kind: window.sloppy.sim.human.kind,
    firstPerson: window.sloppy.view.inFirstPerson,
  }));
  assert.deepEqual(respawned, { kind: "heavy", firstPerson: true });

  await page.locator("#view-mode").click();
  await page.waitForFunction(() => window.sloppy.view.camera.fov === 43);
  const overhead = await page.evaluate(() => ({
    fov: window.sloppy.view.camera.fov,
    height: window.sloppy.view.camera.position[1],
    reticle: window.sloppy.view.crosshair.position[1],
    hud: document.querySelector("#hud").classList.contains("first-person"),
  }));
  assert.ok(overhead.height > 15, `overhead camera restored: ${overhead.height}`);
  assert.equal(overhead.fov, 43);
  assert.ok(Math.abs(overhead.reticle - 1.05) < 1e-6, `reticle height ${overhead.reticle}`);
  assert.equal(overhead.hud, false);
  await page.screenshot({ path: `${output}/overhead.png` });
  assert.deepEqual(errors, []);
  console.log(`First-person check passed (pointer lock ${locked ? "captured" : "unavailable"}).`);
} finally {
  await browser.close();
}
