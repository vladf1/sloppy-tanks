import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { StateMirror } from "../src/net/replication.ts";
import {
  checkMultiplayerMenu,
  click,
  launchChrome,
  openMultiplayerTab,
  recordRoomFrames,
} from "./multiplayer-helpers.mjs";
const base = process.env.SLOPPY_PUBLIC_URL ?? "https://sloppy-tanks-dev.fridman.me/";
const output = "artifacts/performance/multiplayer/public";
await mkdir(output, { recursive: true });
const browser = await launchChrome();
const errors = [],
  clients = [];
try {
  const first = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  const second = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  for (const page of [first, second]) {
    const client = recordRoomFrames(page, errors, { mirror: new StateMirror() });
    client.page = page;
    clients.push(client);
    await page.addInitScript(() => Object.defineProperty(document, "hidden", { get: () => false }));
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error") errors.push(message.text());
    });
  }
  // The host creates a room on Battle Setup; its link opens Battle Setup for the guest
  // with the room selected.
  await first.goto(base);
  await openMultiplayerTab(first);
  await first.locator("#player-name").fill("W".repeat(24));
  await first.locator('input[name="playerTeam"][value="0"]').check();
  await click(first, "#create-room");
  await first.waitForURL(/[?&]room=/);
  await second.goto(first.url());
  await second.locator("#join-room:enabled").waitFor();
  await second.locator("#player-name").fill("Public Bob");
  await second.locator('input[name="playerTeam"][value="1"]').check();
  await click(second, "#join-room");
  await Promise.all(
    clients.map((c) =>
      c.page.waitForFunction(() => document.querySelector("#hud")?.style.opacity === "1", null, {
        timeout: 60000,
      }),
    ),
  );
  // The live HUD can appear before arena preparation and its resume baseline finish.
  const ready = (c) => c.fullEpoch >= 2 && c.inputs.at(-1)?.controlEpoch === c.fullEpoch;
  const readyDeadline = Date.now() + 60000;
  while (!clients.every(ready) && Date.now() < readyDeadline) await first.waitForTimeout(50);
  assert.ok(clients.every(ready), "Both arenas are ready for input after resume");
  for (const [index, c] of clients.entries()) {
    // Focus loss clears held controls; drive each visible player independently.
    await c.page.bringToFront();
    const before = { ...c.mirror.render(c.control.tankId).viewer.position };
    const inputStart = c.inputs.length;
    const key = index ? "a" : "d";
    await c.page.keyboard.down(key);
    await c.page.waitForTimeout(800);
    await c.page.keyboard.up(key);
    await c.page.waitForTimeout(150);
    const after = c.mirror.render(c.control.tankId).viewer.position;
    assert.ok(Math.hypot(after.x - before.x, after.z - before.z) > 0.4, "Public player movement");
    assert.ok(
      c.inputs.length - inputStart >= 8,
      `Held movement retains active input cadence (${c.inputs.length - inputStart} packets)`,
    );
    assert.ok(c.snapshots > 5);
    assert.equal(
      await c.page.evaluate(() => "sloppyMultiplayer" in window),
      false,
      "Production diagnostics absent",
    );
    await c.page.screenshot({ path: `${output}/player-${index}.png` });
    await c.page.locator("#nerd-stats button").click();
    assert.match(await c.page.locator("#nerd-stats-details").innerText(), /Network/);
    assert.match(await c.page.locator("#nerd-stats-details").innerText(), /RTT/);
    await c.page.keyboard.press("n");
    assert.equal(await c.page.locator("#nerd-stats-details").isVisible(), false);
  }
  await first.locator("#pause").click();
  await first.locator("#network-end").click();
  await Promise.all(clients.map((c) => c.page.locator("#network-scoreboard h2").waitFor()));
  await checkMultiplayerMenu(first);
  assert.equal(await first.locator("#network-resume").isVisible(), false);
  assert.equal(await first.locator("#network-end").isVisible(), false);
  await first.screenshot({ path: `${output}/results.png` });
  await first.locator("#leave-room").click();
  await first.locator("#room-list").waitFor();
  const directory = await first.goto(new URL("test-pages.html", base).href);
  assert.ok(directory.ok());
  assert.ok((await first.locator('a[href*="tests/"]').count()) > 3);
  for (const path of [
    "build-info.json",
    "tests/reinforcements.browser.html",
    "tools/tank-surface-check.html",
  ]) {
    const response = await first.request.get(new URL(path, base).href);
    assert.equal(response.status(), 200, path);
  }
  await first.goto(new URL("tests/reinforcements.browser.html", base).href);
  await first.locator("canvas").waitFor();
  await first.waitForTimeout(2000);
  await first.screenshot({ path: `${output}/fixture.png` });
  assert.deepEqual(errors, []);
  console.log(
    "Public dev site: create, room link and join, two real players, movement, results, leave, test directory, fixture and build metadata passed.",
  );
} finally {
  for (const { page } of clients) {
    if (
      await page
        .locator("#pause")
        .isVisible()
        .catch(() => false)
    )
      await page
        .locator("#pause")
        .click()
        .catch(() => {});
    if (
      await page
        .locator("#leave-room")
        .isVisible()
        .catch(() => false)
    )
      await page
        .locator("#leave-room")
        .click()
        .catch(() => {});
  }
  await writeFile(
    `${output}/result.json`,
    JSON.stringify(
      {
        base,
        errors,
        clients: clients.map((c) => ({
          inputs: c.inputs.length,
          snapshots: c.snapshots,
          tick: c.mirror.tick,
        })),
      },
      null,
      2,
    ),
  );
  await browser.close();
}
