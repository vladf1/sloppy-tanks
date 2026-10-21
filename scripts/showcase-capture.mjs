// Showcase stills for judging the game's look build against build: every standard map
// after a seeded stretch of autoplay, through the follow camera (with HUD), a hero
// close-up of the player's tank, a low vista, the overview, first person and a blast,
// plus Battle Setup. The same seed and frame count give the same battle on a build
// that leaves gameplay alone, so two builds' stills line up shot for shot.
//
//   node scripts/showcase-capture.mjs --out artifacts/showcase/candidate [--maps village,quarry]
//     [--shots follow,hero] [--frames 900] [--seed 4242] [--webgl]
//
// It serves this checkout's `src/generated/engine/` through its own Vite dev server on a
// free port (run `pnpm run wasm` first), so parallel checkouts never share a port.
import fs from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";
import { createServer } from "vite";
import { freezeLoop, launchGame, menuReady, seedGame } from "./browser-helpers.mjs";

const { values } = parseArgs({
  options: {
    out: { type: "string", default: "artifacts/showcase/latest" },
    maps: { type: "string", default: "village,harbor,quarry" },
    shots: { type: "string", default: "" },
    frames: { type: "string", default: "900" },
    seed: { type: "string", default: "4242" },
    width: { type: "string", default: "1600" },
    height: { type: "string", default: "900" },
    webgl: { type: "boolean", default: false },
  },
});
const WIDTH = Number(values.width);
const HEIGHT = Number(values.height);
const FRAMES = Number(values.frames);
const SEED = Number(values.seed);
const wanted = new Set(values.shots.split(",").filter(Boolean));
const take = (shot) => wanted.size === 0 || wanted.has(shot);
const out = path.resolve(values.out);
fs.mkdirSync(out, { recursive: true });

const server = await createServer({
  configFile: path.resolve("vite.config.ts"),
  server: { port: 0, host: "127.0.0.1", strictPort: false },
  logLevel: "error",
});
await server.listen();
const { port } = server.httpServer.address();
const base = `http://127.0.0.1:${port}/sloppy-tanks/`;
const webglQuery = values.webgl ? "&webgl" : "";
const { browser, context, errors } = await launchGame({
  viewport: { width: WIDTH, height: HEIGHT },
  consoleErrors: true,
});
const report = { seed: SEED, frames: FRAMES, base, shots: [], errors };

/** Run `count` real loop frames at 60 Hz, yielding now and then so the page stays live. */
async function play(page, count) {
  await page.evaluate(async (count) => {
    for (let i = 0; i < count; i++) {
      window.advanceFrame(1000 / 60);
      if (i % 60 === 59) await new Promise(requestAnimationFrame);
    }
  }, count);
}

/** Draw `count` frames of `dt` seconds without stepping the simulation (effects animate). */
async function animate(page, count, dt = 1 / 60) {
  await page.evaluate(
    ({ count, dt }) => {
      for (let i = 0; i < count; i++) {
        window.sloppy.game.debug_render(1, dt, false, new Float32Array());
      }
    },
    { count, dt },
  );
}

async function shoot(page, map, shot, hud = false) {
  await page.evaluate((hud) => {
    for (const element of document.querySelectorAll("#hud")) {
      element.style.display = hud ? "" : "none";
    }
  }, hud);
  const file = path.join(out, `${map}-${shot}.png`);
  await page.screenshot({ path: file });
  const stats = await page.evaluate(() => {
    const { drawCalls, triangles, pipelines, latePipelines } = window.sloppy.stats();
    return { drawCalls, triangles, pipelines, latePipelines, error: window.sloppy.error() };
  });
  report.shots.push({ map, shot, file, stats });
  console.log(`${map}-${shot}`, JSON.stringify(stats));
}

try {
  if (take("menu")) {
    const page = await context.newPage();
    await seedGame(page, SEED);
    await page.goto(`${base}?map=village${webglQuery}`);
    await menuReady(page, { timeout: 120_000 });
    await page.waitForTimeout(1500);
    const file = path.join(out, "menu.png");
    await page.screenshot({ path: file });
    report.shots.push({ map: "menu", shot: "menu", file });
    await page.close();
  }

  for (const map of values.maps.split(",").filter(Boolean)) {
    const page = await context.newPage();
    await seedGame(page, SEED);
    await freezeLoop(page);
    await page.goto(`${base}?autoplay&map=${map}${webglQuery}`);
    await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing", null, {
      timeout: 120_000,
    });
    await page.evaluate(() => {
      for (const element of document.querySelectorAll("#overlay")) element.style.display = "none";
      window.runLoop(1000);
    });
    await play(page, FRAMES);

    if (take("follow")) {
      await page.evaluate(() => window.engine.draw());
      await shoot(page, map, "follow", true);
    }
    const { x: hx, z: hz, heading } = await page.evaluate(() => window.sloppy.sim.human);
    // Three-quarter front of the hull, low like a photographer kneeling beside it.
    const fx = Math.sin(heading);
    const fz = Math.cos(heading);
    if (take("hero")) {
      const eye = [hx + fx * 6.5 - fz * 4.5, 2.6, hz + fz * 6.5 + fx * 4.5];
      await page.evaluate((pose) => window.engine.draw(pose), [...eye, hx, 1.0, hz]);
      await shoot(page, map, "hero");
    }
    if (take("vista")) {
      const eye = [hx - fx * 22 + fz * 10, 7, hz - fz * 22 - fx * 10];
      await page.evaluate(
        (pose) => window.engine.draw(pose),
        [...eye, hx + fx * 20, 0, hz + fz * 20],
      );
      await shoot(page, map, "vista");
    }
    if (take("overview")) {
      await page.evaluate(() => {
        window.engine.draw([], true);
        window.engine.draw([], true);
      });
      await shoot(page, map, "overview");
    }
    if (take("action")) {
      // Blasts ahead of the player, through the shared explosion path, caught mid-bloom.
      await page.evaluate(
        ({ x, z }) => {
          window.sloppy.game.debug_explode(x, z, 6, 40);
          window.sloppy.game.debug_explode(x + 4, z - 3, 5, 30);
        },
        { x: hx + fx * 9, z: hz + fz * 9 },
      );
      await play(page, 9);
      await page.evaluate(() => window.engine.draw());
      await shoot(page, map, "action", true);
    }
    if (take("strip")) {
      // Motion over half a second of play: recoil, suspension, dust, camera.
      for (let frame = 0; frame < 8; frame++) {
        await play(page, 4);
        await page.evaluate(() => window.engine.draw());
        await shoot(page, map, `strip-${frame}`);
      }
    }
    if (take("first-person")) {
      await page.evaluate(() => window.sloppy.firstPerson());
      await animate(page, 90);
      await shoot(page, map, "first-person", true);
    }
    if (map === "village" && take("recap")) {
      await page.evaluate(() => {
        for (const element of document.querySelectorAll("#overlay")) element.style.display = "";
      });
      await page.keyboard.press("Escape");
      await page.locator("#end-battle").waitFor();
      await page.waitForTimeout(400);
      await shoot(page, map, "pause", true);
      await page.locator("#end-battle").click();
      await page.locator(".recap-stats").waitFor();
      await page.waitForTimeout(800);
      await shoot(page, map, "recap", true);
    }
    await page.close();
  }
} finally {
  fs.writeFileSync(path.join(out, "report.json"), JSON.stringify(report, null, 2));
  await browser.close();
  await server.close();
}
if (errors.length) {
  console.error("Page errors:", errors.join("\n"));
  process.exitCode = 1;
}
console.log(`Wrote ${report.shots.length} stills to ${out}`);
