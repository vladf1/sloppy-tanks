// Showcase stills for judging the game's look build against build: every standard map
// after a seeded stretch of autoplay, through the follow camera (with HUD), a hero
// close-up of the player's tank, a low vista, the overview, first person, a blast and
// a tank and a Humvee dying, each kind of destructible cover breaking up, every kind of
// pickup up close and being collected, plus the interface: Battle Setup (both tabs),
// the settings dialog, respawn, pause and the battle report, a tablet's touch HUD and
// the phone edition. The same seed and frame
// count give the same battle on a build that leaves gameplay alone, so two builds'
// stills line up shot for shot.
//
//   node scripts/showcase-capture.mjs --out artifacts/showcase/candidate [--maps village,quarry]
//     [--shots follow,hero,vista,overview,action,strip,first-person,death,destruction,pickups,
//      menu,respawn,settings,recap,tablet,phone] [--frames 900] [--seed 4242] [--webgl]
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

/**
 * A page of `context` showing the seeded autoplay battle on `map` advanced `ticks` fixed
 * steps by `debug_step` alone. The game's loop never steps it: the loop's first frame
 * lands at a different point of the engine's clock on every page load, so a battle
 * played through it drifts a few ticks between runs and then apart. Render-only frames
 * settle the camera; one loop frame at time zero (no step) fills the HUD.
 */
async function steppedBattlePage(context, map, ticks) {
  const page = await context.newPage();
  await seedGame(page, SEED);
  await freezeLoop(page);
  await page.goto(`${base}?autoplay&map=${map}${webglQuery}`);
  await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing", null, {
    timeout: 120_000,
  });
  await page.evaluate((ticks) => {
    for (const element of document.querySelectorAll("#overlay")) element.style.display = "none";
    window.sloppy.game.debug_step(ticks, 0, 0);
    window.runLoop(0);
  }, ticks);
  await animate(page, 90);
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
  if (take("pickups")) {
    // Pickups: on a fresh stepped copy of each map's battle, one of every kind stands in
    // an arc opening toward the player a few metres up-screen, on the nearest clear
    // ground (the player moves there when its own spot has none), seen up close from a
    // fixed elevated three-quarter camera and through the follow camera with the HUD.
    // Then the arc gives way to one power-up beside the player, everyone held, which the
    // player takes on the next tick, and a fixed camera watches eight moments of the
    // collection effect.
    const kinds = [
      "spread",
      "rocket",
      "ricochet",
      "piercing",
      "rapid",
      "shield",
      "speed",
      "repair",
      "laser",
    ];
    // Ten seconds into the battle: under way, its cover still standing.
    const PICKUP_BATTLE_TICKS = 600;
    const ARC_RADIUS = 9;
    const ARC_HALF_ANGLE = Math.PI / 3;
    // Free ground kept around each crate and around the player's spots.
    const PICKUP_CLEARANCE = 1.6;
    const TANK_CLEARANCE = 2.5;
    const TANK_SIZE = 6;
    // Where the arc may go, nearest first: the player's spot, then rings of spots
    // around it, each tried with the arc turned a little away from up-screen.
    const SHIFT_STEP = 4;
    const SHIFT_RINGS = 3;
    const TURN_STEP = Math.PI / 8;
    const TURN_STEPS = 4;
    // Where the player stands for the collection, toward the arc, and its camera.
    const COLLECT_AHEAD = 6;
    // Dead tanks wait this long to respawn while the collection plays.
    const HELD_RESPAWN_SECONDS = 999;
    const moments = [1, 2, 3, 4, 6, 8, 10, 10];
    for (const map of values.maps.split(",").filter(Boolean)) {
      const page = await steppedBattlePage(context, map, PICKUP_BATTLE_TICKS);
      const human = await page.evaluate(() => window.sloppy.sim.human);
      if (!human.alive) {
        console.log(`${map}-pickups: player not alive`);
        await page.close();
        continue;
      }
      const { id } = human;
      // The ground direction that runs up the follow camera's screen, so the arc shows
      // in that view whichever way the camera faces: solve for the world step whose
      // projection moves straight up.
      const [ux, uz] = await page.evaluate(({ x, z }) => {
        const { game } = window.sloppy;
        window.engine.draw();
        const [x0, y0] = game.debug_screen_point(x, 0, z);
        const [xa, ya] = game.debug_screen_point(x + 1, 0, z);
        const [xb, yb] = game.debug_screen_point(x, 0, z + 1);
        const [ax, ay, bx, by] = [xa - x0, ya - y0, xb - x0, yb - y0];
        const det = ax * by - bx * ay;
        const [a, b] = [bx / det, -ax / det];
        const length = Math.hypot(a, b);
        return [a / length, b / length];
      }, human);
      const blockers = await page.evaluate(
        ({ id, size }) => {
          const covers = window.engine
            .covers()
            .filter((cover) => cover.alive)
            .map(({ x, z, w, d }) => ({ x, z, w, d }));
          const tanks = window.sloppy.sim.tanks
            .filter((tank) => tank.alive && tank.id !== id)
            .map(({ x, z }) => ({ x, z, w: size, d: size }));
          return [...covers, ...tanks];
        },
        { id, size: TANK_SIZE },
      );
      const blocked = (x, z, margin) =>
        blockers.some(
          (box) =>
            Math.abs(x - box.x) < box.w / 2 + margin && Math.abs(z - box.z) < box.d / 2 + margin,
        );
      const layout = (hx, hz, turn) => {
        const dx = ux * Math.cos(turn) - uz * Math.sin(turn);
        const dz = ux * Math.sin(turn) + uz * Math.cos(turn);
        const [sx, sz] = [-dz, dx];
        const arc = kinds.map((kind, index) => {
          const angle = ARC_HALF_ANGLE * ((2 * index) / (kinds.length - 1) - 1);
          const ahead = Math.cos(angle) * ARC_RADIUS;
          const across = Math.sin(angle) * ARC_RADIUS;
          return {
            kind,
            x: hx + dx * ahead + sx * across,
            z: hz + dz * ahead + sz * across,
            available: true,
          };
        });
        const clear =
          arc.every((pickup) => !blocked(pickup.x, pickup.z, PICKUP_CLEARANCE)) &&
          !blocked(hx, hz, TANK_CLEARANCE) &&
          !blocked(hx + dx * COLLECT_AHEAD, hz + dz * COLLECT_AHEAD, TANK_CLEARANCE);
        return { hx, hz, dx, dz, sx, sz, arc, clear };
      };
      const candidates = [];
      for (let ring = 0; ring <= SHIFT_RINGS; ring++) {
        const spots = ring === 0 ? 1 : 8;
        for (let turn = 0; turn <= TURN_STEPS; turn++) {
          for (let spot = 0; spot < spots; spot++) {
            const bearing = (spot * Math.PI) / 4;
            const x = human.x + Math.cos(bearing) * ring * SHIFT_STEP;
            const z = human.z + Math.sin(bearing) * ring * SHIFT_STEP;
            for (const sign of turn === 0 ? [1] : [1, -1]) {
              candidates.push(layout(x, z, sign * turn * TURN_STEP));
            }
          }
        }
      }
      const chosen = candidates.find((candidate) => candidate.clear) ?? candidates[0];
      if (!chosen.clear) console.log(`${map}-pickups: no clear ground for the arc`);
      const { hx, hz, dx, dz, sx, sz, arc } = chosen;
      await page.evaluate(
        ({ id, hx, hz, moved, json }) => {
          const { game } = window.sloppy;
          if (moved) game.debug_place_tank(id, hx, hz, NaN);
          game.debug_set_pickups(json);
        },
        { id, hx, hz, moved: hx !== human.x || hz !== human.z, json: JSON.stringify(arc) },
      );
      // Settle the follow camera on the player and let the pickups bob and spin.
      await animate(page, 60);
      const eye = [hx - dx * 5 + sx * 4, 10, hz - dz * 5 + sz * 4];
      const center = [hx + dx * 6.5, 0.8, hz + dz * 6.5];
      await page.evaluate((pose) => window.engine.draw(pose), [...eye, ...center]);
      await shoot(page, map, "pickups-close");
      // A loop frame at time zero steps nothing, redraws the follow view and refreshes
      // the HUD.
      await page.evaluate(() => window.runLoop(0));
      await shoot(page, map, "pickups-follow", true);

      // A power-up with no precondition, a metre from the player; every tank holds
      // still and the dead stay down, so the effect plays out undisturbed.
      const [px, pz] = [hx + dx * COLLECT_AHEAD, hz + dz * COLLECT_AHEAD];
      await page.evaluate(
        ({ id, px, pz, json, HELD_RESPAWN_SECONDS }) => {
          const { game, sim } = window.sloppy;
          game.debug_place_tank(id, px, pz, NaN);
          for (const tank of sim.tanks) {
            const hold = tank.alive ? { frozen: true } : { respawn: HELD_RESPAWN_SECONDS };
            window.engine.setTank(tank.id, hold);
          }
          game.debug_set_pickups(json);
        },
        {
          id,
          px,
          pz,
          json: JSON.stringify([{ kind: "speed", x: px + sx, z: pz + sz, available: true }]),
          HELD_RESPAWN_SECONDS,
        },
      );
      const pose = [px - dx * 7 + sx * 7, 7.5, pz - dz * 7 + sz * 7, px, 0.8, pz];
      for (const [frame, count] of moments.entries()) {
        await page.evaluate(
          ({ count, pose }) => {
            for (let i = 0; i < count; i++) {
              window.sloppy.game.debug_step(1, 0, 0);
              window.sloppy.game.debug_render(1, 1 / 60, false, new Float32Array(pose));
            }
          },
          { count, pose },
        );
        await shoot(page, map, `pickups-collect-strip-${frame}`);
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
