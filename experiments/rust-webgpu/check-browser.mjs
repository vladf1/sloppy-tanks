import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { launchGame } from "../../scripts/browser-helpers.mjs";

const url = process.env.RUST_LAB_URL ?? "http://127.0.0.1:5188/";
const output = fileURLToPath(new URL("../../artifacts/performance/rust-webgpu/", import.meta.url));
await mkdir(output, { recursive: true });
const { browser, page, errors } = await launchGame({
  viewport: { width: 1440, height: 960 },
  consoleErrors: true,
});
const requests = [];
page.on("request", (request) => requests.push(request.url()));
async function click(selector) {
  const box = await page.locator(selector).boundingBox();
  assert.ok(box, `${selector} is visible`);
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}
async function frames(count = 5) {
  await page.evaluate(
    (count) =>
      new Promise((resolve) => {
        const frame = () => {
          if (--count <= 0) resolve();
          else requestAnimationFrame(frame);
        };
        requestAnimationFrame(frame);
      }),
    count,
  );
}
try {
  await page.goto(url);
  await page.waitForFunction(() => ["ready", "error"].includes(document.body.dataset.state), null, {
    timeout: 60000,
  });
  assert.equal(
    await page.locator("body").getAttribute("data-state"),
    "ready",
    await page.locator("#status").textContent(),
  );
  assert.ok(
    await page.evaluate(
      () =>
        window.rustLab.featureChecks.length === 11 &&
        window.rustLab.featureChecks.every((n) => n > 0),
    ),
    "Game-used Rapier APIs execute in release Wasm",
  );
  await page.waitForFunction(() => window.rustLab.snapshot().ticks > 30);
  assert.equal(await page.evaluate(() => window.rustLab.snapshot().bodies), 24);
  await page.screenshot({ path: `${output}/initial.png` });
  await click("#pause");
  const ticks = await page.evaluate(() => window.rustLab.snapshot().ticks);
  await frames();
  assert.equal(await page.evaluate(() => window.rustLab.snapshot().ticks), ticks);
  await click("#launch");
  assert.equal(await page.evaluate(() => window.rustLab.snapshot().bodies), 25);
  await click("#pause");
  await page.waitForFunction(() => window.rustLab.snapshot().displacement > 3);
  await page.screenshot({ path: `${output}/impact.png` });
  await click("#pause");
  await click("#reset");
  assert.deepEqual(
    await page.evaluate(() => {
      const s = window.rustLab.snapshot();
      return [s.bodies, s.ticks, s.displacement];
    }),
    [24, 0, 0],
  );
  const canvas = await page.locator("canvas").boundingBox();
  const beforeOrbit = await page.locator("canvas").screenshot();
  await page.mouse.move(canvas.x + canvas.width / 2, canvas.y + canvas.height / 2);
  await page.mouse.down();
  await page.mouse.move(canvas.x + canvas.width / 2 + 160, canvas.y + canvas.height / 2 + 35, {
    steps: 12,
  });
  await page.mouse.up();
  await page.mouse.wheel(0, -180);
  await frames();
  assert.notDeepEqual(
    await page.locator("canvas").screenshot(),
    beforeOrbit,
    "Orbit and zoom change the image",
  );
  await page.keyboard.press("Space");
  assert.equal(await page.evaluate(() => window.rustLab.snapshot().bodies), 25);
  await page.keyboard.press("r");
  assert.equal(await page.evaluate(() => window.rustLab.snapshot().bodies), 24);
  for (let i = 0; i < 45; i++) await page.keyboard.press("Space");
  assert.equal(await page.evaluate(() => window.rustLab.snapshot().bodies), 40);
  for (let i = 0; i < 10; i++) {
    await page.keyboard.press("r");
    await frames(1);
    assert.equal(await page.evaluate(() => window.rustLab.snapshot().bodies), 24);
  }
  await page.setViewportSize({ width: 1100, height: 760 });
  await frames();
  const size = await page
    .locator("canvas")
    .evaluate((canvas) => [
      canvas.width,
      canvas.height,
      Math.round(canvas.clientWidth),
      Math.round(canvas.clientHeight),
    ]);
  assert.deepEqual(size.slice(0, 2), size.slice(2));
  await page.screenshot({ path: `${output}/resized.png` });
  assert.ok(
    requests.some((r) => r.includes(".wasm")),
    "Loads the Rust binary",
  );
  assert.ok(
    !requests.some((r) => /three|src\/game|rapier_wasm/.test(r)),
    "No game or Three.js dependencies",
  );
  assert.deepEqual(errors, []);
  const unavailable = await browser.newPage();
  await unavailable.addInitScript(() =>
    Object.defineProperty(navigator, "gpu", { value: undefined }),
  );
  await unavailable.goto(url);
  await unavailable.waitForFunction(() => document.body.dataset.state === "error");
  assert.match(await unavailable.locator("#status").textContent(), /WebGPU is required/);
  assert.equal(await unavailable.locator("#launch").isDisabled(), true);
  await unavailable.close();
  console.log(
    "PASS: Rust/Wasm startup, real launch/reset/pause clicks, collisions, keyboard, orbit/zoom, bounded bodies, repeated reset, resize, no GPU errors.",
  );
  console.log(`Screenshots: ${output}`);
} finally {
  await browser.close();
}
