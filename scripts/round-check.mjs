// Rounds from Battle Setup to the battle report and back: every standard map starts
// with its tanks shown at their spawns, apart from the player's, and stale frames
// before the first step play nothing; Solo Assault starts and ends when the player is
// destroyed; the overview camera and a collapse of every tower and drum render; END
// BATTLE and a finished round show the report, PLAY AGAIN starts the next round and
// BATTLE SETUP goes back. Repeated resets leave the physics body count and the shell's
// Wasm memory where the first reset left them (a round reset must not leak).
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import {
  clickWidget,
  launchGame,
  nextFrame,
  openShell,
  shellDebug,
  untilState,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/round-check";
mkdirSync(out, { recursive: true });
/** Resets of one arena whose bodies and memory must hold. */
const RESETS = 5;
/** Wasm memory may still grow this much between the second reset and the last (pages
 * the allocator had not reached yet), but not by a round's worth each time. */
const MEMORY_SLACK = 8 * 1024 * 1024;

const { browser, context, page, errors } = await launchGame({
  viewport: { width: 1280, height: 800 },
  consoleErrors: true,
});
// The shell's linear memory: the page's own instance of the largest module it starts.
await context.addInitScript(() => {
  const keep = (result) => {
    const memory = (result.instance ?? result).exports?.memory;
    if (memory instanceof WebAssembly.Memory) {
      if (!window.shellMemory || memory.buffer.byteLength >= window.shellMemory.buffer.byteLength)
        window.shellMemory = memory;
    }
    return result;
  };
  const streaming = WebAssembly.instantiateStreaming;
  WebAssembly.instantiateStreaming = (...args) => streaming(...args).then(keep);
  const instantiate = WebAssembly.instantiate;
  WebAssembly.instantiate = (...args) => instantiate(...args).then(keep);
});
const shot = (name) => page.screenshot({ path: `${out}/${name}.png` });
const memory = () => page.evaluate(() => window.shellMemory?.buffer.byteLength ?? null);
const frames = async (count) => {
  for (let index = 0; index < count; index += 1) await nextFrame(page);
};
const noErrors = async (what) => {
  assert.equal((await nextFrame(page)).error, null, `${what}: renderer errors`);
  assert.deepEqual(errors, [], `${what}: page errors`);
};

/** GO with the shell's clock held, so the round's first frames step nothing: the tanks
 * stand where they spawned. Returns that first frame's debug state; the clock runs on. */
async function startHeld(what) {
  const { now } = await nextFrame(page);
  await page.evaluate((ms) => window.sloppyShell.hold(ms), now);
  await clickWidget(page, "GO!");
  const state = await untilState(page, (state) => state.inRound && state.phase === "playing", {
    extras: ["debug"],
    what: `${what}: the round`,
  });
  await frames(3);
  const held = (await nextFrame(page, ["debug"])).extras.debug;
  assert.equal(held.elapsed, 0, `${what}: held frames step nothing`);
  const tanks = held.tanks.filter((tank) => tank.alive);
  const human = tanks.find((tank) => tank.human);
  assert.ok(human, `${what}: the player's tank is on the field`);
  for (const tank of tanks) {
    assert.equal(tank.shown, true, `${what}: tank ${tank.id} is drawn before its first step`);
    assert.ok(Number.isFinite(tank.x) && Number.isFinite(tank.z), `${what}: tank ${tank.id}`);
    if (tank !== human) {
      const apart = Math.hypot(tank.x - human.x, tank.z - human.z);
      assert.ok(apart > 2, `${what}: tank ${tank.id} spawned ${apart} m from the player`);
    }
  }
  await page.evaluate(() => window.sloppyShell.hold(null));
  await untilState(page, (state) => state.extras.debug.elapsed > 0.5, {
    extras: ["debug"],
    what: `${what}: the round playing on`,
  });
  return { ...state.extras.debug, held };
}

/** Pause and END BATTLE: the battle report, ended early. */
async function endBattle(what) {
  await page.keyboard.press("Escape");
  await untilState(page, (state) => state.phase === "paused", { what: `${what}: the pause` });
  await clickWidget(page, "END BATTLE");
  const state = await untilState(page, (state) => state.phase === "results", {
    extras: ["debug"],
    what: `${what}: the battle report`,
  });
  assert.equal(state.extras.debug.match.endedEarly, true, `${what}: the report says ended early`);
  return state;
}

async function battleSetup(what) {
  await clickWidget(page, "BATTLE SETUP");
  await untilState(page, (state) => !state.inRound && state.ready, {
    what: `${what}: Battle Setup ready again`,
  });
}

try {
  await openShell(page);
  await untilState(page, (state) => state.ready, { what: "the arena ready" });
  await clickWidget(page, "Team Battle");

  // Every standard map starts with its tanks at their spawns.
  for (const [label, map] of [
    ["Pine Village", "village"],
    ["Harbor Havoc", "harbor"],
    ["Dusty Dig", "quarry"],
  ]) {
    await clickWidget(page, label);
    await untilState(page, (state) => state.prefs.map === map && state.ready, {
      timeout: 60000,
      what: `${label}'s arena`,
    });
    const round = await startHeld(label);
    assert.equal(round.mapMode, map, `${label}: the arena`);
    await shot(`start-${map}`);
    await noErrors(label);
    await endBattle(label);
    await battleSetup(label);
  }

  // The overview camera and a collapse of every tower and drum render.
  const village = await (async () => {
    await clickWidget(page, "Pine Village");
    await untilState(page, (state) => state.prefs.map === "village" && state.ready, {
      what: "Pine Village again",
    });
    return startHeld("the village");
  })();
  await shellDebug(page, "overview", true);
  await frames(10);
  assert.equal((await nextFrame(page, ["debug"])).extras.debug.overview, true, "the overview");
  await shot("overview");
  await shellDebug(page, "collapse");
  await frames(90);
  await shot("collapse");
  await shellDebug(page, "overview", false);
  await frames(5);
  await noErrors("the overview and the collapse");

  // END BATTLE, then PLAY AGAIN again and again: the same bodies at each start, and
  // memory that stops growing.
  const bodies = village.held.bodies;
  const sizes = [];
  for (let reset = 1; reset <= RESETS; reset += 1) {
    await endBattle(`reset ${reset}`);
    if (reset === 1) {
      await page.waitForTimeout(800);
      await shot("report");
    }
    await clickWidget(page, "PLAY AGAIN");
    const state = await untilState(
      page,
      (state) => state.phase === "playing" && state.extras.debug.elapsed < 1,
      { extras: ["debug"], what: `reset ${reset}: PLAY AGAIN's round` },
    );
    assert.equal(state.extras.debug.bodies, bodies, `reset ${reset}: bodies after the reset`);
    // Fight a little so the round leaves debris, shells and effects behind.
    await shellDebug(page, "autoplay", true);
    await page.waitForTimeout(2500);
    await shellDebug(page, "autoplay", false);
    sizes.push(await memory());
  }
  assert.ok(
    sizes.every((size) => size > 0),
    `the shell's memory is found: ${sizes}`,
  );
  const growth = sizes.at(-1) - sizes[1];
  assert.ok(growth <= MEMORY_SLACK, `memory over resets 2-${RESETS}: ${sizes.join(", ")}`);
  await endBattle("the last reset");
  await battleSetup("the last reset");

  // Solo Assault starts, and ends when the player's tank is destroyed.
  await clickWidget(page, "Solo Assault");
  await untilState(page, (state) => state.prefs.gameMode === "solo" && state.ready, {
    what: "Solo Assault's arena",
  });
  const solo = await startHeld("Solo Assault");
  assert.equal(solo.gameMode, "solo", "Solo Assault's rules");
  await shellDebug(page, "killHuman");
  const report = await untilState(page, (state) => state.phase === "results", {
    extras: ["debug"],
    what: "Solo Assault's report",
  });
  assert.notEqual(
    report.extras.debug.match.endedEarly,
    true,
    "a finished round is not ended early",
  );
  assert.notEqual(report.extras.debug.match.winner, report.extras.debug.humanTeam);
  await page.waitForTimeout(800);
  await shot("solo-report");
  await clickWidget(page, "PLAY AGAIN");
  await untilState(page, (state) => state.phase === "playing" && state.alive, {
    what: "Solo Assault again",
  });
  await endBattle("Solo Assault again");
  await battleSetup("Solo Assault");
  await clickWidget(page, "Team Battle");

  await noErrors("the rounds");
  const megabytes = sizes.map((size) => (size / 1048576).toFixed(0)).join(", ");
  console.log(
    `round-check: three maps, Solo Assault, overview, collapse, reports and ${RESETS} resets (${bodies} bodies, memory ${megabytes} MiB) passed`,
  );
} finally {
  await browser.close();
}
