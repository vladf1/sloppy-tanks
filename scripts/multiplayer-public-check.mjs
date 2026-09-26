import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { headless } from "./browser-helpers.mjs";
import { StateMirror } from "../src/net/replication.ts";
import { checkMultiplayerMenu, openMultiplayerTab } from "./multiplayer-ui-assertions.mjs";
const base = process.env.SLOPPY_PUBLIC_URL ?? "https://sloppy-tanks-dev.fridman.me/";
const output = "artifacts/performance/multiplayer/public";
await mkdir(output, { recursive: true });
const browser = await chromium.launch({
  channel: "chrome",
  headless,
  args: [
    "--disable-background-timer-throttling",
    "--disable-renderer-backgrounding",
    "--disable-backgrounding-occluded-windows",
  ],
});
const errors = [],
  clients = [];
try {
  const first = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  await first.goto(base);
  await openMultiplayerTab(first);
  await first.locator("#join-room").waitFor();
  const inviteURL = new URL(first.url());
  inviteURL.searchParams.delete("multiplayer");
  inviteURL.searchParams.set(
    "room",
    [...crypto.getRandomValues(new Uint8Array(8))]
      .map((n) => "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"[n & 31])
      .join(""),
  );
  const invite = inviteURL.href;
  for (const [index, page] of [
    first,
    await browser.newPage({ viewport: { width: 1200, height: 800 } }),
  ].entries()) {
    const client = { page, mirror: new StateMirror(), inputs: 0, snapshots: 0 };
    clients.push(client);
    await page.addInitScript(() => Object.defineProperty(document, "hidden", { get: () => false }));
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error") errors.push(message.text());
    });
    page.on("websocket", (socket) => {
      socket.on("framereceived", ({ payload }) => {
        try {
          const m = JSON.parse(String(payload));
          if (m.type === "lobby") client.lobby = m;
          if (m.type === "control") client.control = m;
          if (m.type === "full") {
            client.mirror.applyFull(m, client.lobby);
            client.fullEpoch = client.control?.controlEpoch;
          }
          if (m.type === "snapshot") {
            client.snapshots++;
            for (const s of m.snapshots) assert.ok(client.mirror.applySnapshot(s));
          }
          if (m.type === "error" || m.type === "room-reset") errors.push(m);
        } catch (error) {
          errors.push(error.message);
        }
      });
      socket.on("framesent", ({ payload }) => {
        const m = JSON.parse(String(payload));
        if (m.type === "input") {
          client.inputs++;
          client.ready = client.fullEpoch >= 2 && m.controlEpoch === client.fullEpoch;
        }
      });
    });
    await page.goto(invite);
    await page.locator("#player-name").fill(index ? "Public Bob" : "W".repeat(24));
    await page.locator("#player-team").selectOption(String(index));
    await page.locator("#join-room").click();
    await page.locator("#host-settings").waitFor({ state: "visible" });
    await checkMultiplayerMenu(page);
    assert.equal(await page.locator("#join-room").isVisible(), false);
    assert.equal(await page.locator("#network-resume").isVisible(), false);
    assert.equal(await page.locator("#network-end").isVisible(), false);
  }
  await first.screenshot({ path: `${output}/lobby.png` });
  await first.locator("#start-match").click();
  await Promise.all(
    clients.map((c) =>
      c.page.waitForFunction(
        () => document.querySelector("#network-status").textContent === "",
        null,
        { timeout: 60000 },
      ),
    ),
  );
  // The live HUD can appear before arena preparation and its resume baseline finish.
  const readyDeadline = Date.now() + 60000;
  while (!clients.every((c) => c.ready) && Date.now() < readyDeadline)
    await first.waitForTimeout(50);
  assert.ok(
    clients.every((c) => c.ready),
    "Both arenas are ready for input after resume",
  );
  for (const [index, c] of clients.entries()) {
    // Focus loss clears held controls; drive each visible player independently.
    await c.page.bringToFront();
    const before = { ...c.mirror.render(c.control.tankId).viewer.position };
    const inputStart = c.inputs;
    const key = index ? "a" : "d";
    await c.page.keyboard.down(key);
    await c.page.waitForTimeout(800);
    await c.page.keyboard.up(key);
    await c.page.waitForTimeout(150);
    const after = c.mirror.render(c.control.tankId).viewer.position;
    assert.ok(Math.hypot(after.x - before.x, after.z - before.z) > 0.4, "Public player movement");
    assert.ok(
      c.inputs - inputStart >= 8,
      `Held movement retains active input cadence (${c.inputs - inputStart} packets)`,
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
    "Public dev site: entry/share/join, two real players, movement, results, leave, test directory, fixture and build metadata passed.",
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
          inputs: c.inputs,
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
