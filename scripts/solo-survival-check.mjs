import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const base = "http://127.0.0.1:5179/sloppy-tanks/";
const browser = await chromium.launch({ channel: "chrome", headless: false });
const errors = [];
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.on("pageerror", e => errors.push(e.message));
  await page.goto(`${base}tests/reinforcements.browser.html`);
  await page.waitForFunction(() => /PASS|FAIL/.test(document.querySelector("#result").textContent));
  const lifecycle = await page.locator("#result").innerText();
  assert.match(lifecycle, /^PASS/);
  await page.goto(base); await page.waitForFunction(() => !!window.sloppy);
  await page.locator('input[value="solo"]').check();
  assert.match(await page.locator(".menu-foot").innerText(), /10 MINUTES/);
  await page.locator('[data-kind="balanced"]').click();
  await page.waitForFunction(() => document.querySelector("#label0").textContent === "KILLS");
  assert.match(await page.locator("#time").innerText(), /10:00|9:59/);
  assert.equal(await page.locator("#score0").innerText(), "0");
  await page.evaluate(() => {
    const s = window.sloppy.sim; s.human.protection = 999;
    for (let i = 0; i < 55; i++) {
      const t = s.tanks.find(t => !t.human && t.alive); t.protection = 0;
      s.damageTank(t, 9999, s.human.id, s.humanTeam);
      s.reinforcementDelay = 0; s.reinforceSolo();
    }
  });
  await page.waitForFunction(() => document.querySelector("#score0").textContent === "55");
  assert.equal(await page.evaluate(() => window.sloppy.sim.match.phase), "playing");
  assert.equal(await page.locator("#score1").innerText(), "6");
  mkdirSync("artifacts/performance/solo-survival", { recursive: true });
  await page.screenshot({ path: "artifacts/performance/solo-survival/scoreboard.png" });
  await page.keyboard.press("Escape");
  const frozen = await page.evaluate(() => window.sloppy.sim.match.time);
  await page.waitForTimeout(200);
  assert.equal(await page.evaluate(() => window.sloppy.sim.match.time), frozen);
  await page.evaluate(() => { window.sloppy.sim.match.time = 0.001; });
  await page.locator("#resume").click();
  await page.waitForFunction(() => document.querySelector("#overlay h2")?.textContent === "SURVIVED");
  assert.equal(await page.locator(".result-score").innerText(), "55");
  await page.screenshot({ path: "artifacts/performance/solo-survival/results.png" });
  await page.locator("#restart").click();
  await page.locator('[data-kind="balanced"]').click();
  await page.evaluate(() => {
    const s = window.sloppy.sim; s.human.protection = 0;
    s.damageTank(s.human, 99999, s.tanks[1].id, s.tanks[1].team);
  });
  await page.waitForFunction(() => document.querySelector("#overlay h2")?.textContent === "TANK DESTROYED");
  assert.equal(await page.locator(".result-score").innerText(), "0");
  assert.deepEqual(errors, []);
  const result = { lifecycle, checks: ["10-minute start", "55 kills without early finish", "live kill/active scoreboard", "pause freezes timer", "time limit preserves final kills", "death ends run", "restart clears kills"], errors };
  writeFileSync("artifacts/solo-survival-results.json", JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result, null, 2));
} finally { await browser.close(); }
