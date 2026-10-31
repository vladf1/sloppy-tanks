// The geometry cache (`src/geometry-cache.ts`, `crates/render/src/presentation/geometry_cache.rs`):
// on each standard map, a first visit builds its vehicles and scenery and stores them,
// and a second visit in the same profile reads both back instead of building, then
// draws the same frame with the same GPU resources (draws, meshes, materials, models,
// instances and bytes), with no larger Wasm heap. Each map gets a fresh profile.
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { chromium } from "playwright";
import { collectErrors, headless, nextFrame, openShell, untilState } from "./browser-helpers.mjs";

const MAPS = ["village", "harbor", "quarry"];

/** The figures a visit must repeat once its arena and library are built. */
function resources(stats) {
  const { drawCalls, shadowDrawCalls, reflectionDrawCalls, triangles, meshes, materials } = stats;
  const { models, instances, drawClasses, instanceRecords, gpuBytes, textures } = stats;
  return {
    drawCalls,
    shadowDrawCalls,
    reflectionDrawCalls,
    triangles,
    meshes,
    materials,
    models,
    instances,
    drawClasses,
    instanceRecords,
    gpuBytes,
    textures,
  };
}

async function visit(context, map, errors) {
  const page = await context.newPage();
  await page.addInitScript(() => {
    const keep = (result) => {
      const memory = (result.instance ?? result).exports?.memory;
      if (memory instanceof WebAssembly.Memory) window.shellMemory = memory;
      return result;
    };
    const streaming = WebAssembly.instantiateStreaming;
    WebAssembly.instantiateStreaming = (...args) => streaming(...args).then(keep);
  });
  collectErrors(page, errors, { consoleErrors: true, prefix: `${map}: ` });
  await openShell(page, `seed=1&map=${map}`);
  // The arena ready and every library model built: the files are written then.
  const state = await untilState(
    page,
    (state) => {
      const stats = state.extras?.stats;
      return state.ready && stats?.view?.libraryPending === 0 && stats.texturesPending === 0;
    },
    { extras: ["stats"], timeout: 60000, what: `${map}: the arena and its library` },
  );
  // The vehicles are written once the last wreck is built, then everything stored.
  await page.waitForFunction(
    () => {
      const cache = globalThis.sloppyGeometry;
      const vehicles = cache?.stats.vehicles;
      return cache?.pending() === 0 && vehicles && ("storeMs" in vehicles || "readMs" in vehicles);
    },
    null,
    { timeout: 30000 },
  );
  const frame = await nextFrame(page, ["stats"]);
  const result = {
    resources: resources(frame.extras.stats ?? state.extras.stats),
    files: await page.evaluate(() => structuredClone(globalThis.sloppyGeometry?.stats ?? {})),
    memory: await page.evaluate(() => window.shellMemory?.buffer.byteLength ?? 0),
  };
  await page.close();
  return result;
}

const errors = [];
for (const map of MAPS) {
  const profile = mkdtempSync(path.join(tmpdir(), "sloppy-geometry-check-"));
  const context = await chromium.launchPersistentContext(profile, {
    channel: "chrome",
    headless,
    viewport: { width: 1280, height: 800 },
  });
  try {
    const first = await visit(context, map, errors);
    const second = await visit(context, map, errors);
    const scenery = `scenery-${map}`;
    for (const file of ["vehicles", scenery]) {
      assert.ok(first.files[file]?.writeMs >= 0, `${map}: the first visit writes ${file}`);
      assert.ok(first.files[file]?.storeMs >= 0, `${map}: the first visit stores ${file}`);
      assert.ok(second.files[file]?.readMs >= 0, `${map}: the second visit reads ${file}`);
      assert.equal(second.files[file]?.rejected, undefined, `${map}: ${file} decodes`);
    }
    assert.equal(second.files[scenery].buildMs, undefined, `${map}: no scenery built`);
    assert.deepEqual(second.resources, first.resources, `${map}: the same frame and resources`);
    assert.ok(second.memory <= first.memory, `${map}: no larger Wasm heap read back`);
    const ms = (file) => second.files[file].readMs.toFixed(1);
    console.log(
      `${map}: read back the vehicles in ${ms("vehicles")} ms and the scenery in ${ms(scenery)} ms` +
        ` (built in ${first.files[scenery].buildMs.toFixed(1)}); ${first.resources.drawCalls} draws,` +
        ` heap ${first.memory / 2 ** 20} → ${second.memory / 2 ** 20} MiB`,
    );
  } finally {
    await context.close();
    rmSync(profile, { recursive: true, force: true });
  }
}
assert.deepEqual(errors, [], "page errors");
console.log("geometry-cache-check: every map's vehicles and scenery read back alike");
