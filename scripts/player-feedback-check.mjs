// One real room connection and deterministic DOM feedback fixtures over its live arena.
// Fixture screenshots prove presentation only; they do not represent network combat.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { click, collectErrors, gameUrl, launchChrome } from "./browser-helpers.mjs";
import { openMultiplayerTab, recordRoomFrames } from "./multiplayer-helpers.mjs";

const output = "artifacts/performance/player-ux";
await mkdir(output, { recursive: true });
const url = new URL(gameUrl);
if (process.env.SLOPPY_SERVER) url.searchParams.set("server", process.env.SLOPPY_SERVER);
url.searchParams.set("multiplayer", "");
const browser = await launchChrome();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const errors = [];
const room = recordRoomFrames(page, errors);
const checks = [];
collectErrors(page, errors, { consoleErrors: true });
try {
  await page.goto(url.href);
  await openMultiplayerTab(page);
  await page.locator("#player-name").fill("Feedback check");
  await page.locator('input[name="playerTeam"][value="0"]').check();
  await click(page, "#create-room");
  await page.waitForFunction(
    () => {
      const game = window.sloppyMultiplayer;
      return (
        game?.connection.connected &&
        game.control?.driver === "human" &&
        game.view &&
        game.hud?.match.phase === "playing"
      );
    },
    null,
    { timeout: 60000 },
  );
  const before = await page.evaluate(() => window.sloppyMultiplayer.mirror.tick);
  await page.waitForFunction((tick) => window.sloppyMultiplayer.mirror.tick > tick + 3, before);
  assert.ok(room.snapshots > 0, "The room receives real server snapshots");
  assert.equal(await page.evaluate(() => !!window.sloppyMultiplayer.game.error()), false);
  await page.screenshot({ path: `${output}/multiplayer-live.png` });
  checks.push(
    "Live local room joins through physical clicks, renders, and receives advancing snapshots",
  );

  // Keep the arena and transport live, but isolate DOM fixtures from incoming UI updates.
  // This intercepts the existing development handle; production code has no fixture path.
  await page.evaluate(() => {
    const game = window.sloppyMultiplayer;
    const ui = game.ui;
    const update = ui.update.bind(ui);
    const event = ui.event.bind(ui);
    const base = structuredClone(game.hud);
    ui.update = () => {};
    ui.event = () => {};
    window.playerFeedbackFixture = {
      base,
      /** Show an event about the player, then the HUD it left. */
      announce(hud, fields) {
        event(
          {
            id: hud.human.id,
            x: 0,
            z: 0,
            own: true,
            playerHit: false,
            damageAngle: null,
            ...fields,
          },
          hud,
        );
        update(hud, 0, true);
      },
      restore() {
        ui.update = update;
        ui.event = event;
        ui.resetFeedback();
        update(game.hud, 0, game.connection.connected);
      },
      draw(patch) {
        const hud = structuredClone(base);
        Object.assign(hud.human, patch);
        update(hud, 0, true);
        return hud;
      },
    };
  });

  await page.evaluate(() => {
    window.playerFeedbackFixture.draw({
      alive: true,
      hp: 18,
      healthRatio: 0.18,
      healthColor: 0xff6655,
      rank: 2,
      rankName: "Elite",
      rankDamage: 1.2,
      rankFireRate: 1.15,
      rankHealth: 1.15,
      rankRepair: 0.01,
      repairDelay: 5,
      protection: 0,
      shield: 4.2,
      shieldPoints: 80,
      rapid: 3.2,
      speed: 2.2,
      laser: 1.2,
      selfRepair: true,
    });
  });
  assert.equal(
    await page.locator("#effects").textContent(),
    "◇ SHIELD 80 HP · 5s  » RAPID 4s  ϟ BOOST 3s  ✧ LASER DEFENSE 2s  SELF-REPAIR",
  );
  assert.equal(await page.locator(".status.critical-health").count(), 1);
  assert.equal(await page.locator("#rank").getAttribute("data-rank"), "2");
  assert.match(await page.locator("#rank").getAttribute("title"), /\+20% damage/);
  assert.match(await page.locator("#rank").getAttribute("title"), /repairs 1% hull\/s after 5s/);
  await page.screenshot({
    path: `${output}/multiplayer-fixture-effects.png`,
    animations: "disabled",
  });
  checks.push(
    "Injected HUD: power-up countdowns, repair status, rank benefits, and critical-hull warning",
  );

  await page.evaluate(() => window.playerFeedbackFixture.draw({ alive: true, healthRatio: 0.25 }));
  assert.equal(await page.locator(".status.critical-health").count(), 0, "25% is not critical");
  await page.evaluate(() => {
    const fixture = window.playerFeedbackFixture;
    const hud = structuredClone(fixture.base);
    Object.assign(hud.human, { alive: false, hp: 0, healthRatio: 0, respawn: 2.4 });
    const owner = Math.max(...hud.scoreboard.map((tank) => tank.id)) + 1;
    hud.scoreboard.push({ id: owner, name: "Rival <b>literal</b>", team: 1, kills: 1, deaths: 0 });
    fixture.announce(hud, {
      type: "death",
      owner,
      damageAngle: Math.PI / 3,
      damageSource: { cause: "rocket", origin: { x: 1, z: 1 } },
    });
    fixture.dead = hud;
  });
  assert.equal(await page.locator("#network-respawn").isVisible(), true);
  assert.equal(await page.locator("#network-respawn-count").textContent(), "Respawn in 3");
  assert.equal(
    await page.locator("#network-death-cause").textContent(),
    "Rival <b>literal</b> killed you with a rocket blast.",
  );
  assert.equal(await page.locator("#network-death-cause b, #toast b, #feed b").count(), 0);
  assert.equal(
    await page.locator(".status.critical-health").count(),
    0,
    "Destroyed hull does not pulse",
  );
  // Keep the escaping assertion above, and show an ordinary player name in the screenshot.
  await page.evaluate(() => {
    const fixture = window.playerFeedbackFixture;
    const hud = fixture.dead;
    const killer = hud.scoreboard.at(-1);
    killer.name = "Iron Badger";
    window.sloppyMultiplayer.ui.resetFeedback();
    fixture.announce(hud, {
      type: "death",
      owner: killer.id,
      damageSource: { cause: "rocket", origin: { x: 1, z: 1 } },
    });
  });
  await page.screenshot({
    path: `${output}/multiplayer-fixture-death.png`,
    animations: "disabled",
  });
  checks.push(
    "Injected death event: killer and weapon, countdown, literal HTML-like name, no critical pulse",
  );

  await page.evaluate(() => {
    const fixture = window.playerFeedbackFixture;
    fixture.announce(fixture.base, { type: "respawn" });
  });
  assert.equal(await page.locator("#network-respawn").isVisible(), false);
  assert.equal(await page.locator("#network-death-cause").textContent(), "");
  assert.equal(await page.locator("#damage-direction").isVisible(), false);
  assert.equal(
    await page.locator("#toast").evaluate((toast) => toast.classList.contains("visible")),
    false,
  );
  checks.push("Injected respawn event clears death cause, damage direction, and death toast");

  await page.evaluate(() => {
    const fixture = window.playerFeedbackFixture;
    fixture.announce(fixture.dead, {
      type: "death",
      owner: fixture.dead.human.id,
      damageAngle: 0,
      damageSource: { cause: "mine", origin: { x: 0, z: 0 } },
    });
  });
  assert.equal(
    await page.locator("#network-death-cause").textContent(),
    "You destroyed yourself with a mine explosion.",
  );
  await page.evaluate(() => window.sloppyMultiplayer.ui.resetFeedback());
  assert.equal(await page.locator("#network-death-cause").textContent(), "");
  assert.equal(await page.locator("#network-respawn").isVisible(), false);
  assert.equal(await page.locator("#feed").textContent(), "");
  checks.push("Injected self-death identifies the mine; reset clears respawn and feed feedback");

  await page.evaluate(() => window.playerFeedbackFixture.restore());
  assert.equal(await page.evaluate(() => !!window.sloppyMultiplayer.game.error()), false);
  assert.deepEqual(errors, [], "No page, console, protocol, or GPU errors");
  await writeFile(
    `${output}/multiplayer-feedback-results.json`,
    JSON.stringify(
      {
        checks,
        snapshots: room.snapshots,
        errors,
        screenshots: {
          "multiplayer-live.png": "Live authoritative local room",
          "multiplayer-fixture-effects.png": "Injected deterministic HUD over live rendered arena",
          "multiplayer-fixture-death.png":
            "Injected deterministic death event over live rendered arena",
        },
      },
      null,
      2,
    ) + "\n",
  );
  console.log(JSON.stringify({ checks, snapshots: room.snapshots, errors }, null, 2));
} finally {
  await browser.close();
}
