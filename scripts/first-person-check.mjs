import { gameUrl, launchGame, startRound } from "./browser-helpers.mjs";
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
const output = "artifacts/performance/first-person";
mkdirSync(output, { recursive: true });
const { browser, page, errors } = await launchGame();
const turn = (from, to) => Math.atan2(Math.sin(to - from), Math.cos(to - from));
try {
  await page.goto(gameUrl, { waitUntil: "domcontentloaded", timeout: 60000 });
  await page.waitForFunction(() => !!window.sloppy);
  await page.locator('[data-kind="balanced"]').click();
  await startRound(page);
  await page.mouse.move(800, 300);
  await page.waitForTimeout(300);
  const overheadAim = await page.evaluate(() => window.sloppy.sim.human.aim);

  await page.keyboard.press("v");
  await page.waitForFunction(() => window.sloppy.view.inFirstPerson);
  const seated = await page.evaluate(() => {
    const { sim, view } = window.sloppy;
    const eye = view.camera.position;
    const tank = sim.human.body.translation();
    return {
      yaw: view.firstPerson.yaw,
      eyeHeight: eye.y,
      eyeDistance: Math.hypot(eye.x - tank.x, eye.z - tank.z),
      hud: document.querySelector("#hud").classList.contains("first-person"),
      pressed: document.querySelector("#view-mode").getAttribute("aria-pressed"),
      ownModel: view.tankMeshes.get(sim.human.id).visible,
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
  const start = await page.evaluate(() => window.sloppy.sim.human.body.translation());
  await page.keyboard.down("w");
  await page.waitForTimeout(1500);
  await page.keyboard.up("w");
  const end = await page.evaluate(() => window.sloppy.sim.human.body.translation());
  const moved = Math.hypot(end.x - start.x, end.z - start.z);
  const along =
    ((end.x - start.x) * Math.sin(turned.yaw) + (end.z - start.z) * Math.cos(turned.yaw)) / moved;
  assert.ok(moved > 1 && along > 0.8, `W follows the view: ${JSON.stringify({ moved, along })}`);
  await page.screenshot({ path: `${output}/driving.png` });

  // A physical press fires, and captures the pointer where the browser allows it.
  await page.mouse.move(800, 450);
  await page.mouse.down();
  await page.waitForFunction(() => window.sloppy.sim.human.cooldown > 0);
  await page.mouse.up();
  const locked = await page.evaluate(
    () => document.pointerLockElement === document.querySelector("#game"),
  );

  const phase = () => page.evaluate(() => window.sloppy.sim.match.phase);
  const captured = () =>
    page.evaluate(() => document.pointerLockElement === document.querySelector("#game"));
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
  await page.locator("#resume").click();
  await page.waitForFunction(() => window.sloppy.sim.match.phase === "playing");

  // A death keeps the pointer captured, so the respawn needs no click or new lock notice.
  // The HUD hides the pause menu a few frames after play resumes; click the arena, not it.
  await page.waitForFunction(
    () => getComputedStyle(document.querySelector("#overlay")).display === "none",
  );
  await page.mouse.click(800, 450);
  if (locked) {
    await page.waitForFunction(
      () => document.pointerLockElement === document.querySelector("#game"),
    );
  }
  const destroy = () =>
    page.evaluate(() => {
      const { sim } = window.sloppy;
      Object.assign(sim.human, { protection: 0, shield: 0, shieldPoints: 0 });
      sim.damageTank(sim.human, 999, sim.human.id, sim.human.team);
    });
  const heavy = page.locator("#overlay .respawn [data-kind='heavy']");
  await destroy();
  await heavy.waitFor({ state: "visible" });
  await page.waitForFunction(() => window.sloppy.sim.human.alive, null, { timeout: 10000 });
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
  const choice = await heavy.boundingBox();
  await page.mouse.click(choice.x + choice.width / 2, choice.y + choice.height / 2);
  assert.equal(await page.evaluate(() => window.sloppy.sim.humanKind), "heavy");
  await page.waitForFunction(() => window.sloppy.sim.human.alive, null, { timeout: 10000 });
  const respawned = await page.evaluate(() => ({
    kind: window.sloppy.sim.human.kind,
    firstPerson: window.sloppy.view.inFirstPerson,
  }));
  assert.deepEqual(respawned, { kind: "heavy", firstPerson: true });

  await page.locator("#view-mode").click();
  await page.waitForFunction(() => !window.sloppy.view.inFirstPerson);
  const overhead = await page.evaluate(() => ({
    fov: window.sloppy.view.camera.fov,
    height: window.sloppy.view.camera.position.y,
    reticle: window.sloppy.view.crosshair.position.y,
    hud: document.querySelector("#hud").classList.contains("first-person"),
  }));
  assert.ok(overhead.height > 15, `overhead camera restored: ${overhead.height}`);
  assert.equal(overhead.fov, 43);
  assert.equal(overhead.reticle, 1.05);
  assert.equal(overhead.hud, false);
  await page.screenshot({ path: `${output}/overhead.png` });
  assert.deepEqual(errors, []);
  console.log(`First-person check passed (pointer lock ${locked ? "captured" : "unavailable"}).`);
} finally {
  await browser.close();
}
