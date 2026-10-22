// Showcase stills for judging the game's look build against build: every standard map
// after a seeded stretch of autoplay, through the follow camera (with HUD), a hero
// close-up of the player's tank, a low vista, the overview, first person, a blast and
// a tank and a Humvee dying, each kind of destructible cover breaking up, plus the
// interface: Battle Setup (both tabs), the settings dialog, respawn, pause and
// the battle report, a tablet's touch HUD and the phone edition. The same seed and frame
// count give the same battle on a build that leaves gameplay alone, so two builds'
// stills line up shot for shot.
//
//   node scripts/showcase-capture.mjs --out artifacts/showcase/candidate [--maps village,quarry]
//     [--shots follow,hero,vista,overview,action,strip,first-person,death,destruction,menu,respawn,
//      settings,recap,tablet,phone] [--frames 900] [--seed 4242] [--webgl]
//
// It serves this checkout's `src/generated/engine/` through its own Vite dev server on a
// free port (run `pnpm run wasm` first), so parallel checkouts never share a port.
import fs from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";
import { createServer } from "vite";
import {
  freezeLoop,
  installEngineHelpers,
  launchGame,
  menuReady,
  seedGame,
} from "./browser-helpers.mjs";

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

/** A page of `context` showing a seeded autoplay battle on `map` after `frames` frames. */
async function battlePage(context, map, frames) {
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
  await play(page, frames);
  return page;
}

/** A context for another device; its pages' errors are not the build's errors. */
async function deviceContext(viewport, { touch = false, phone = false } = {}) {
  const device = await browser.newContext({
    viewport,
    screen: viewport,
    hasTouch: touch,
    isMobile: phone,
    deviceScaleFactor: touch ? 2 : 1,
  });
  await installEngineHelpers(device);
  return device;
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
    // The multiplayer tab against a server that is not there: its offline state, the
    // same on every machine. The refused connection is expected, so this page has its
    // own context outside the collected errors.
    const offline = await deviceContext({ width: WIDTH, height: HEIGHT });
    const rooms = await offline.newPage();
    await seedGame(rooms, SEED);
    await rooms.goto(`${base}?map=village&server=ws://127.0.0.1:9${webglQuery}`);
    await menuReady(rooms, { timeout: 120_000 });
    await rooms.locator("#tab-multiplayer").click();
    await rooms.waitForTimeout(2500);
    const roomsFile = path.join(out, "menu-multiplayer.png");
    await rooms.screenshot({ path: roomsFile });
    report.shots.push({ map: "menu", shot: "menu-multiplayer", file: roomsFile });
    await offline.close();
  }

  for (const map of values.maps.split(",").filter(Boolean)) {
    const page = await battlePage(context, map, FRAMES);

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
    const interfaceShots = ["respawn", "settings", "recap"].filter(take);
    if (map === "village" && interfaceShots.length) {
      await page.evaluate(() => {
        for (const element of document.querySelectorAll("#overlay")) element.style.display = "";
      });
    }
    if (map === "village" && take("respawn")) {
      await page.evaluate(() => window.sloppy.killHuman());
      await play(page, 60);
      await page.evaluate(() => window.engine.draw());
      await shoot(page, map, "respawn", true);
    }
    if (map === "village" && take("settings")) {
      // A script click: autoplay's idle overlay covers the HUD's buttons.
      await page.evaluate(() => document.querySelector("#settings-open").click());
      await page.waitForTimeout(500);
      await shoot(page, map, "settings", true);
      await page.evaluate(() => document.querySelector("#settings")?.close());
      await page.waitForTimeout(200);
    }
    if (map === "village" && take("recap")) {
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

  if (take("death")) {
    // Deaths up close: on a fresh copy of each map's battle (so the other shots keep
    // their state), the nearest enemy tank and then the nearest Humvee are destroyed
    // through the shared damage path and watched from a fixed elevated three-quarter
    // camera through eight moments, from the first flash to the burning wreck.
    const moments = [2, 4, 6, 8, 12, 20, 40, 80];
    for (const map of values.maps.split(",").filter(Boolean)) {
      const page = await battlePage(context, map, FRAMES);
      for (const [name, humvee] of [
        ["death", false],
        ["humvee-death", true],
      ]) {
        const victim = await page.evaluate((humvee) => {
          const { human, tanks } = window.sloppy.sim;
          const enemies = tanks.filter(
            (tank) => tank.alive && tank.team !== human.team && (tank.kind === "humvee") === humvee,
          );
          const near = (tank) => Math.hypot(tank.x - human.x, tank.z - human.z);
          return enemies.sort((a, b) => near(a) - near(b))[0] ?? null;
        }, humvee);
        if (!victim) {
          console.log(`${map}-${name}: no victim`);
          continue;
        }
        const pose = [victim.x - 8, 8.5, victim.z - 8, victim.x, 0.8, victim.z];
        await page.evaluate((id) => {
          const { human } = window.sloppy.sim;
          const team = human.team === "red" || human.team === 1 ? 1 : 0;
          window.sloppy.game.debug_damage_tank(id, 100000, human.id, team);
        }, victim.id);
        for (const [frame, count] of moments.entries()) {
          await play(page, count);
          await page.evaluate((pose) => window.engine.draw(pose), pose);
          await shoot(page, map, `${name}-strip-${frame}`);
        }
      }
      await page.close();
    }
  }
  if (take("destruction")) {
    // Destructible cover breaking up: on a fresh copy of each map's battle, the cover of
    // each destructible kind nearest the player is destroyed through the shared damage
    // path and watched from a fixed elevated camera scaled to its size, through eight
    // moments from the hit to the settled debris.
    const moments = [2, 4, 8, 12, 20, 32, 60, 120];
    const kinds = ["tree", "timber", "tower", "cargo", "drum", "house", "container", "concrete"];
    for (const map of values.maps.split(",").filter(Boolean)) {
      const page = await battlePage(context, map, FRAMES);
      for (const kind of kinds) {
        const cover = await page.evaluate((kind) => {
          const { human } = window.sloppy.sim;
          const near = (cover) => Math.hypot(cover.x - human.x, cover.z - human.z);
          const covers = window.engine
            .covers()
            .filter((cover) => cover.kind === kind && cover.alive);
          return covers.sort((a, b) => near(a) - near(b))[0] ?? null;
        }, kind);
        if (!cover) continue;
        const reach = Math.max(7, 1.6 * Math.max(cover.w, cover.d, cover.h));
        const pose = [
          cover.x - reach,
          reach * 0.9,
          cover.z - reach,
          cover.x,
          cover.h * 0.3,
          cover.z,
        ];
        await page.evaluate((id) => window.sloppy.game.debug_damage_cover(id, 100000), cover.id);
        for (const [frame, count] of moments.entries()) {
          await play(page, count);
          await page.evaluate((pose) => window.engine.draw(pose), pose);
          await shoot(page, map, `break-${kind}-strip-${frame}`);
        }
      }
      await page.close();
    }
  }
  if (take("tablet")) {
    // An iPad-sized touchscreen: the touch HUD (drive stick, mine and zoom buttons).
    const tablet = await deviceContext({ width: 1180, height: 820 }, { touch: true });
    const page = await battlePage(tablet, "village", 300);
    await page.evaluate(() => window.engine.draw());
    await shoot(page, "village", "tablet", true);
    await tablet.close();
  }
  if (take("phone")) {
    // The phone edition: Battle Setup in portrait, a battle in landscape.
    const portrait = await deviceContext({ width: 402, height: 874 }, { touch: true, phone: true });
    const menu = await portrait.newPage();
    await seedGame(menu, SEED);
    await menu.goto(`${base}?map=village${webglQuery}`);
    await menuReady(menu, { timeout: 120_000 });
    await menu.waitForTimeout(1500);
    const menuFile = path.join(out, "phone-menu.png");
    await menu.screenshot({ path: menuFile });
    report.shots.push({ map: "phone", shot: "phone-menu", file: menuFile });
    await portrait.close();
    const landscape = await deviceContext(
      { width: 874, height: 402 },
      { touch: true, phone: true },
    );
    const page = await battlePage(landscape, "village", 300);
    await page.evaluate(() => window.engine.draw());
    await shoot(page, "village", "phone", true);
    await landscape.close();
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
