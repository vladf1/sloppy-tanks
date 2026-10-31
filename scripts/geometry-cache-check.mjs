// The geometry cache (`src/geometry-cache.ts`, `crates/render/src/presentation/geometry_cache.rs`):
// on each standard map, a first visit builds its vehicles and scenery and stores them;
// later visits in the same profile, alternately with the cache off and on, either build
// or read both back, and every one draws the same frame with the same GPU resources
// (draws, meshes, materials, models, instances and bytes), the cached ones with no
// larger Wasm heap. Each map gets a fresh profile. SLOPPY_MAPS picks maps
// (`village,harbor`) and SLOPPY_ROUNDS the visits of each kind (3).
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { chromium } from "playwright";
import { collectErrors, headless, nextFrame, openShell, untilState } from "./browser-helpers.mjs";

const MAPS = (process.env.SLOPPY_MAPS ?? "village,harbor,quarry").split(",");

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

/** One visit; `cached` keeps the geometry cache on (`?geometry-cache=off` otherwise). */
async function visit(context, map, errors, cached) {
  const page = await context.newPage();
  await page.addInitScript(() => {
    const keep = (result) => {
      const memory = (result.instance ?? result).exports?.memory;
      if (memory instanceof WebAssembly.Memory) window.shellMemory = memory;
      return result;
    };
    const streaming = WebAssembly.instantiateStreaming;
    WebAssembly.instantiateStreaming = (...args) => streaming(...args).then(keep);
    const instantiate = WebAssembly.instantiate;
    WebAssembly.instantiate = (...args) => instantiate(...args).then(keep);
  });
  collectErrors(page, errors, { consoleErrors: true, prefix: `${map}: ` });
  await openShell(page, `seed=1&map=${map}${cached ? "" : "&geometry-cache=off"}`);
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
  if (cached) {
    await page.waitForFunction(
      () => {
        const cache = globalThis.sloppyGeometry;
        const vehicles = cache?.stats.vehicles;
        return (
          cache?.pending() === 0 && vehicles && ("storeMs" in vehicles || "readMs" in vehicles)
        );
      },
      null,
      { timeout: 30000 },
    );
  }
  const frame = await nextFrame(page, ["stats"]);
  const result = {
    resources: resources(frame.extras.stats ?? state.extras.stats),
    files: await page.evaluate(() => structuredClone(globalThis.sloppyGeometry?.stats ?? {})),
    memory: await page.evaluate(() => window.shellMemory?.buffer.byteLength ?? 0),
  };
  await page.close();
  return result;
}

/** Warm visits of each kind per map, alternating which goes first. */
const ROUNDS = Number(process.env.SLOPPY_ROUNDS ?? 3);
/** How much more Wasm heap a cached visit may peak at than a built one (above). */
const READ_HEAP_SLACK = 2 ** 20;
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = sorted.length >> 1;
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};
const mib = (bytes) => bytes / 2 ** 20;

const errors = [];
for (const map of MAPS) {
  const profile = mkdtempSync(path.join(tmpdir(), "sloppy-geometry-check-"));
  const context = await chromium.launchPersistentContext(profile, {
    channel: "chrome",
    headless,
    viewport: { width: 1280, height: 800 },
  });
  try {
    const first = await visit(context, map, errors, true);
    const scenery = `scenery-${map}`;
    for (const file of ["vehicles", scenery]) {
      assert.ok(first.files[file]?.writeMs >= 0, `${map}: the first visit writes ${file}`);
      assert.ok(first.files[file]?.storeMs >= 0, `${map}: the first visit stores ${file}`);
    }
    // Then warm visits (the page and its textures in the HTTP cache) that build with the
    // cache off and that read it back, compared with each other: a first visit also
    // downloads, which moves where its allocations land.
    const built = [];
    const read = [];
    for (let round = 0; round < ROUNDS; round++) {
      for (const cached of round % 2 ? [true, false] : [false, true]) {
        (cached ? read : built).push(await visit(context, map, errors, cached));
      }
    }
    for (const visited of read) {
      for (const file of ["vehicles", scenery]) {
        assert.ok(visited.files[file]?.readMs >= 0, `${map}: a cached visit reads ${file}`);
        assert.equal(visited.files[file]?.rejected, undefined, `${map}: ${file} decodes`);
      }
      assert.equal(visited.files[scenery].buildMs, undefined, `${map}: no scenery built`);
    }
    for (const visited of [...built, ...read]) {
      assert.deepEqual(visited.resources, first.resources, `${map}: the same frame and resources`);
    }
    // The heap's peak (it never shrinks) moves by a few MiB from visit to visit with
    // when textures and bakes land beside the scenery's batches, so the medians of
    // interleaved visits compare: reading back must not need more of it, give or take
    // READ_HEAP_SLACK (the harbor, whose scenery is the smallest, reads back about 0.6 MiB
    // above its built median; the regression this guards against was 6.7 MiB).
    const builtHeap = median(built.map((visited) => visited.memory));
    const readHeap = median(read.map((visited) => visited.memory));
    const heaps = (visits) => visits.map((visited) => mib(visited.memory)).join(", ");
    assert.ok(
      readHeap <= builtHeap + READ_HEAP_SLACK,
      `${map}: a larger Wasm heap read back (${heaps(read)} MiB) than built (${heaps(built)})`,
    );
    const ms = (file) => median(read.map((visited) => visited.files[file].readMs)).toFixed(1);
    console.log(
      `${map}: read back the vehicles in ${ms("vehicles")} ms and the scenery in ${ms(scenery)} ms` +
        ` (built in ${first.files[scenery].buildMs.toFixed(1)}); ${first.resources.drawCalls} draws;` +
        ` heap built ${heaps(built)}, read back ${heaps(read)} MiB`,
    );
  } finally {
    await context.close();
    rmSync(profile, { recursive: true, force: true });
  }
}
assert.deepEqual(errors, [], "page errors");
console.log("geometry-cache-check: every map's vehicles and scenery read back alike");
