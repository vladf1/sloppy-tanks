import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";

const output = "artifacts/performance/recap";
mkdirSync(output, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless: false });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1100 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";
  async function open() {
    await page.goto(url);
    await page.waitForFunction(() => !!window.sloppy);
    await page.locator("#startup-overlay #start").click();
    await page.locator("#startup-overlay").waitFor({ state: "hidden" });
  }
  async function finish(mode, kills) {
    await page.evaluate(
      ({ mode, kills }) => {
        const debug = window.sloppy;
        debug.sim.gameMode = mode;
        debug.start();
        const sim = debug.sim;
        sim.elapsed = 300;
        Object.assign(sim.human, {
          kills,
          damageDealt: 2840,
          bestLifeKills: 8,
          highestRank: 3,
          deaths: 3,
        });
        Object.assign(sim.combatRecord, {
          lifeStarted: 265,
          longestLife: 132,
          busiestMinute: 9,
          multikill: 4,
          clutchKills: 3,
          revengeKills: 2,
          posthumousKills: 1,
          mineKills: 3,
          coverDestroyed: 23,
          pickups: 14,
          shots: 112,
          directHits: 68,
          damageTaken: 386,
          shieldAbsorbed: 240,
        });
        sim.match.scores = sim.humanTeam === 0 ? [52, 39] : [39, 52];
        sim.match.time = 0;
        sim.match.winner = sim.humanTeam;
        sim.match.phase = "results";
      },
      { mode, kills },
    );
    await page.locator(".recap-stats").waitFor();
  }
  async function click(selector) {
    const locator = page.locator(selector);
    await locator.scrollIntoViewIfNeeded();
    const box = await locator.boundingBox();
    assert.ok(box);
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  }
  await open();
  for (const mode of ["team", "solo"]) {
    await page.evaluate((mode) => {
      const d = window.sloppy;
      d.sim.gameMode = mode;
      d.start();
    }, mode);
    await page.keyboard.press("Escape");
    await page.locator("#end-battle").waitFor();
    assert.equal(await page.locator("#overlay #restart").count(), 0);
    const before = await page.evaluate(() => ({
      elapsed: window.sloppy.sim.elapsed,
      kills: window.sloppy.sim.human.kills,
    }));
    await click("#end-battle");
    await page.locator(".recap-stats").waitFor();
    assert.equal(await page.locator(".results h2").innerText(), "BATTLE ENDED");
    assert.deepEqual(
      await page.evaluate(() => ({
        elapsed: window.sloppy.sim.elapsed,
        kills: window.sloppy.sim.human.kills,
      })),
      before,
    );
    await click("#play-again");
    await page.waitForFunction(() => window.sloppy.sim.match.phase === "playing");
  }
  await page.evaluate(() => {
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith("sloppy-records-v1:")) {
        localStorage.removeItem(key);
      }
    }
  });
  await finish("team", 17);
  assert.match(await page.locator(".recap-note").innerText(), /First records/);
  assert.match(await page.locator(".recap-feats").innerText(), /ONE-TANK ARMY/);
  assert.equal(await page.locator(".recap-stat").count(), 6);
  assert.equal(await page.locator(".recap-detail").count(), 12);
  await click("#play-again");
  await page.waitForFunction(() => window.sloppy.sim.match.phase === "playing");
  assert.equal(await page.evaluate(() => window.sloppy.sim.combatRecord.busiestMinute), 0);
  await finish("team", 21);
  assert.match(await page.locator(".recap-heading").innerText(), /NEW PERSONAL BEST/);
  await page.screenshot({ path: `${output}/battle-report.png` });
  await open();
  await finish("team", 19);
  assert.match(await page.locator(".recap-stat").first().innerText(), /BEST 21/);
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  const top = await page.locator(".results").boundingBox();
  assert.ok(top.y >= 0, "scrollable report must not clip its heading above the viewport");
  await page.screenshot({ path: `${output}/battle-report-mobile.png` });
  const mobile = await page
    .locator("#overlay")
    .evaluate((element) => ({ width: element.clientWidth, scrollWidth: element.scrollWidth }));
  assert.equal(mobile.scrollWidth, mobile.width, "report must fit inside the scroll container");
  await page.locator("#restart").scrollIntoViewIfNeeded();
  await page.screenshot({ path: `${output}/battle-report-mobile-bottom.png` });
  await click("#restart");
  await page.waitForFunction(() => window.sloppy.sim.match.phase === "ready");
  await page.setViewportSize({ width: 1440, height: 1100 });
  await page.evaluate(() => {
    const d = window.sloppy;
    d.sim.gameMode = "solo";
    d.start();
    d.sim.elapsed = 42;
    const enemy = d.sim.tanks.find((t) => t.team !== d.sim.human.team);
    d.sim.human.protection = 0;
    d.sim.damageTank(d.sim.human, 9999, enemy.id, enemy.team);
  });
  await page.locator(".recap-stats").waitFor();
  assert.match(await page.locator(".results h2").innerText(), /TANK DESTROYED/);
  assert.match(await page.locator(".recap-stat").nth(2).innerText(), /0:42/);
  assert.match(await page.locator(".recap-feats").innerText(), /GLORIOUS PILE OF SCRAP/);
  await page.screenshot({ path: `${output}/battle-report-solo.png` });
  await click("#play-again");
  await page.waitForFunction(() => window.sloppy.sim.match.phase === "playing");
  assert.deepEqual(errors, []);
  console.log(
    "PASS: 18 stats, earned feats, record persistence, team/solo outcomes, mobile scrolling, coordinate-click replay/setup, no browser errors",
  );
} finally {
  await browser.close();
}
