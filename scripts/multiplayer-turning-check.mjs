import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
const output = "artifacts/performance/multiplayer/turning";
await mkdir(output, { recursive: true });
const label = process.env.SLOPPY_TURNING_LABEL ?? "current";
assert.match(label, /^[a-z0-9-]+$/);
const browser = await chromium.launch({ channel: "chrome", headless: false });
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const url = new URL(process.env.SLOPPY_URL ?? "http://127.0.0.1:5175/sloppy-tanks/");
  url.searchParams.set(
    "room",
    [...crypto.getRandomValues(new Uint8Array(8))]
      .map((n) => "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"[n & 31])
      .join(""),
  );
  if (process.env.SLOPPY_LATENCY) url.searchParams.set("latency", process.env.SLOPPY_LATENCY);
  await page.goto(url.href);
  await page.locator("#player-name").fill("Turning check");
  await page.locator("#join-room").click();
  await page.locator("#host-settings").waitFor({ state: "visible" });
  await page.locator("#room-humans-only").check();
  await page.waitForFunction(() => document.querySelector("#room-difficulty").disabled);
  await page.locator("#start-match").click();
  await page.waitForFunction(
    () =>
      window.sloppyMultiplayer?.control?.controlEpoch >= 3 &&
      window.sloppyMultiplayer.control.driver === "human",
    null,
    { timeout: 60000 },
  );
  await page.evaluate(() => {
    window.turningFrames = [];
    window.captureTurning = true;
    const sample = (now) => {
      const game = window.sloppyMultiplayer;
      window.turningFrames.push({
        now,
        tick: game.mirror.tick,
        heading: game.display.viewer.heading,
      });
      if (window.captureTurning) requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  });
  for (let i = 0; i < 16; i++) {
    const key = ["d", "w", "a", "s"][i % 4];
    await page.keyboard.down(key);
    await page.waitForTimeout(300);
    await page.keyboard.up(key);
  }
  const frames = await page.evaluate(() => {
    window.captureTurning = false;
    return window.turningFrames;
  });
  const changes = frames
    .slice(1)
    .map((frame, i) =>
      Math.abs(
        Math.atan2(
          Math.sin(frame.heading - frames[i].heading),
          Math.cos(frame.heading - frames[i].heading),
        ),
      ),
    );
  const moving = changes.filter((change) => change > 0.00001).length;
  const report = {
    errors,
    frames,
    movingFraction: moving / changes.length,
    maxStepRadians: Math.max(...changes),
    durationMs: frames.at(-1).now - frames[0].now,
  };
  await writeFile(`${output}/${label}.json`, JSON.stringify(report, null, 2));
  await page.screenshot({ path: `${output}/${label}.png` });
  console.log(JSON.stringify({ ...report, frames: frames.length }, null, 2));
  assert.deepEqual(errors, []);
  if (process.env.SLOPPY_TURNING_ASSERT === "1") {
    assert.ok(report.movingFraction > 0.75, "Local hull turns between network updates");
    await page.keyboard.press("n");
    await page.waitForFunction(
      () => document.querySelector("#nerd-stats button")?.getAttribute("aria-expanded") === "true",
    );
    await page.waitForFunction(() =>
      [...document.querySelectorAll("#nerd-stats pre")].some((row) =>
        /^FPS\s+\d+/.test(row.textContent),
      ),
    );
    const stats = await page.locator("#nerd-stats-details").innerText();
    assert.match(stats, /Network/);
    assert.match(stats, /RTT/);
    assert.match(stats, /Update CPU/);
    assert.doesNotMatch(stats, /Sim CPU/);
    await page.screenshot({ path: `${output}/${label}-stats.png` });
    await page.keyboard.press("n");
    assert.equal(await page.locator("#nerd-stats-details").isVisible(), false);
  }
  await page.locator("#pause").click();
  await page.locator("#network-end").click();
} finally {
  await browser.close();
}
