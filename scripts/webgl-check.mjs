// The WebGL2 fallback engine. `?webgl` asks for it on any browser: on every standard and
// extra map the page must download only the WebGL build, start a round through Battle
// Setup, drive and fire, and draw the arena (shadows included) without page, console,
// GL or engine errors, and without clearing an index buffer through a vector of
// zeros in the Wasm heap. Without `?webgl` the page must pick the build this browser
// supports by itself (WebGPU when it has an adapter, else WebGL) and download only
// that one. Where WebGPU has an adapter, a WebGPU device that fails must fall back
// to WebGL on the same canvas, and the two engines must draw the same still frame of
// the quarry (whose fixed scenery shadow is cached) closely alike. Through 32 Scrap
// Yard resets and five resizes the context's live GL objects and the Wasm memory must
// stop changing, and a lost context and a GL error must reach `Game.error()`.
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import {
  chooseMap,
  freezeLoop,
  gameUrl,
  launchGame,
  menuReady,
  pixels,
  seedGame,
  startRound,
} from "./browser-helpers.mjs";

const output = "artifacts/performance/webgl";
const WIDTH = 1280;
const HEIGHT = 720;
/** Software WebGL (SwiftShader, on machines without a GPU) takes minutes to compile
 * an arena's programs; hardware takes seconds. */
const LOADING_TIMEOUT_MS = 300_000;
/** The standard maps and the extra levels (offered with `?debug`). */
const MAPS = ["village", "harbor", "quarry", "stress-test", "superstress"];
/** The still frame both engines draw: the quarry's floor and cliffs from above. */
const STILL_POSE = [-40, 70, 40, 0, 0, 0];
/** WebGL may look simpler, but not different: mean channel difference out of 255,
 * and the share of pixels whose largest channel difference exceeds 48. Loose on
 * purpose; a broken cached shadow (the whole floor in shadow) measured 53 and 0.73. */
const STILL_MEAN_TOLERANCE = 16;
const STILL_LARGE_TOLERANCE = 0.15;
/** Resets before the renderer and the allocator settle at their high-water marks;
 * every later reset must leave the same GL objects and Wasm memory. */
const SETTLING_RESETS = 16;
const RESETS = 32;
/** More frames than the backend draws between `getError` checks
 * (`ERROR_CHECK_FRAMES` in `crates/render/src/gpu/webgl/mod.rs`). */
const ERROR_CHECK_DRAWS = 320;
/** A zeroed index buffer must come from the browser (`bufferData` with a size), never
 * from an upload of zeros out of a vector in the engine's Wasm heap, which would keep
 * that size for good (wgpu's GL backend once cleared unwritten buffer tails that way).
 * No index upload this large is all zeros; smaller ones are left out, since a tiny
 * mesh's own indices can be. */
const ZERO_INDEX_UPLOAD_BYTES = 1024;
mkdirSync(output, { recursive: true });

/** The game page with `search` in place of the URL's query. */
const pageUrl = (search) => Object.assign(new URL(gameUrl), { search }).href;

/** Share of sampled pixels that differ clearly from the frame's mean color. */
async function detail(png) {
  const data = await pixels(png);
  const samples = [];
  for (let i = 0; i < data.length; i += 4 * 97) {
    samples.push([data[i], data[i + 1], data[i + 2]]);
  }
  const mean = [0, 1, 2].map((c) => samples.reduce((sum, s) => sum + s[c], 0) / samples.length);
  const varied = samples.filter((s) => s.some((value, c) => Math.abs(value - mean[c]) > 24));
  return varied.length / samples.length;
}

const { browser, context, errors } = await launchGame({
  viewport: { width: WIDTH, height: HEIGHT },
  consoleErrors: true,
});
// Count the bytes of index uploads that are all zeros: wgpu's clears.
await context.addInitScript((minimum) => {
  window.zeroIndexUploadBytes = 0;
  const upload = WebGL2RenderingContext.prototype.bufferSubData;
  WebGL2RenderingContext.prototype.bufferSubData = function (target, offset, data, ...rest) {
    if (
      target === WebGL2RenderingContext.ELEMENT_ARRAY_BUFFER &&
      ArrayBuffer.isView(data) &&
      data.byteLength >= minimum
    ) {
      const bytes = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
      if (bytes.every((byte) => byte === 0)) window.zeroIndexUploadBytes += bytes.length;
    }
    return upload.call(this, target, offset, data, ...rest);
  };
}, ZERO_INDEX_UPLOAD_BYTES);

/** A page whose engine downloads land in `binaries` (file names, in order). */
async function newPage() {
  const page = await context.newPage();
  page.setDefaultTimeout(LOADING_TIMEOUT_MS);
  const binaries = [];
  // Engine binaries are fetched; Vite's dev server also serves `?url` imports of
  // them as scripts.
  page.on("request", (request) => {
    const path = new URL(request.url()).pathname;
    if (path.endsWith(".wasm") && request.resourceType() === "fetch") {
      binaries.push(path.slice(path.lastIndexOf("/") + 1));
    }
  });
  // Chrome reports GL errors as console warnings, not errors (and performance
  // notes, such as screenshots' pixel reads, which are not failures).
  page.on("console", (message) => {
    if (message.type() === "warning" && /WebGL: |GL_INVALID|GL ERROR/.test(message.text())) {
      errors.push(message.text());
    }
  });
  return { page, binaries };
}

/** Open the game at `search` and wait for Battle Setup to be ready. */
async function open(page, search) {
  await page.goto(pageUrl(search));
  await menuReady(page);
}

/** Play a moment of a round: drive, fire, and report what the frame drew. */
async function play(page, name) {
  await startRound(page);
  await page.keyboard.down("KeyW");
  await page.waitForTimeout(1500);
  await page.keyboard.up("KeyW");
  await page.mouse.click(WIDTH / 2, HEIGHT / 3);
  await page.waitForTimeout(1500);
  const stats = await page.evaluate(() => window.sloppy.stats());
  const shot = await page.screenshot({ path: `${output}/${name}.png` });
  return {
    graphicsApi: stats.graphicsApi,
    drawCalls: stats.drawCalls,
    shadowDrawCalls: stats.shadowDrawCalls,
    latePipelines: stats.latePipelines,
    detail: await detail(shot),
    engineError: await page.evaluate(() => window.sloppy.error()),
    human: await page.evaluate(() => Boolean(window.sloppy.sim.human)),
    zeroIndexUploadBytes: await page.evaluate(() => window.zeroIndexUploadBytes),
  };
}

function assertDrew(result, name) {
  assert.equal(result.engineError, null, name);
  assert.ok(result.human, `${name}: the player's tank is in the round`);
  assert.ok(result.drawCalls > 20, `${name}: the arena draws (${result.drawCalls} draw calls)`);
  assert.ok(result.shadowDrawCalls > 0, `${name}: the sun casts shadows`);
  assert.ok(result.detail > 0.2, `${name}: the frame shows the arena (${result.detail})`);
  assert.equal(
    result.zeroIndexUploadBytes,
    0,
    `${name}: index buffer bytes were cleared through a vector of zeros`,
  );
}

/** The quarry drawn once, still, from `STILL_POSE` by the engine `search` picks. */
async function stillQuarry(search, name) {
  const { page } = await newPage();
  await seedGame(page, 424242);
  await freezeLoop(page);
  await page.goto(pageUrl(`?autoplay&map=quarry${search}`));
  await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing");
  const api = await page.evaluate((pose) => {
    for (const element of document.querySelectorAll("#overlay, #hud")) {
      element.style.display = "none";
    }
    window.engine.draw(pose);
    window.engine.draw(pose);
    return window.sloppy.stats().graphicsApi;
  }, STILL_POSE);
  const png = await page.screenshot({ path: `${output}/${name}.png` });
  await page.close();
  return { api, image: await pixels(png), detail: await detail(png) };
}

/** Mean channel difference, and the share of pixels differing by more than 48. */
function difference(a, b) {
  let total = 0;
  let large = 0;
  for (let i = 0; i < a.length; i += 4) {
    let peak = 0;
    for (let c = 0; c < 3; c++) {
      const delta = Math.abs(a[i + c] - b[i + c]);
      total += delta;
      peak = Math.max(peak, delta);
    }
    if (peak > 48) large++;
  }
  const count = a.length / 4;
  return { mean: total / (count * 3), large: large / count };
}

const onlyWebgl = (binaries) => binaries.every((name) => /^engine-webgl_bg[-.]/.test(name));

try {
  for (const map of MAPS) {
    const { page, binaries } = await newPage();
    await open(page, "?webgl&debug");
    await chooseMap(page, map);
    const result = { map, ...(await play(page, `webgl-${map}`)), binaries };
    console.log(JSON.stringify(result));
    assert.equal(result.graphicsApi, "WebGL", map);
    assert.ok(onlyWebgl(binaries), `?webgl downloads only the WebGL engine: ${binaries}`);
    assertDrew(result, map);
    await page.close();
  }

  // Ask for an adapter on a page of its own: once the game holds one, Chrome may
  // not hand out another.
  const { page: probe } = await newPage();
  await probe.goto(new URL("test-pages.html", gameUrl).href);
  const webgpu = await probe.evaluate(async () =>
    Boolean(await navigator.gpu?.requestAdapter().catch(() => null)),
  );
  await probe.close();
  const { page, binaries } = await newPage();
  await open(page, "");
  const { graphicsApi } = await page.evaluate(() => window.sloppy.stats());
  console.log(JSON.stringify({ webgpuAdapter: webgpu, graphicsApi, binaries }));
  assert.equal(graphicsApi, webgpu ? "WebGPU" : "WebGL");
  const build = webgpu ? /^engine_bg[-.]/ : /^engine-webgl_bg[-.]/;
  assert.ok(
    binaries.length > 0 && binaries.every((name) => build.test(name)),
    `the page downloads only the ${graphicsApi} engine: ${binaries}`,
  );
  await page.close();

  if (webgpu) {
    // An adapter whose device fails: the WebGPU engine must not have claimed the
    // canvas, so the WebGL one can draw on it.
    const { page, binaries } = await newPage();
    await page.addInitScript(() => {
      GPUAdapter.prototype.requestDevice = () =>
        Promise.reject(new Error("simulated device failure"));
    });
    await open(page, "");
    const result = { ...(await play(page, "webgpu-device-fallback")), binaries };
    console.log(JSON.stringify({ deviceFallback: result }));
    assert.equal(result.graphicsApi, "WebGL");
    assertDrew(result, "device fallback");
    await page.close();
    const webgl = await stillQuarry("&webgl", "still-quarry-webgl");
    const native = await stillQuarry("", "still-quarry-webgpu");
    assert.deepEqual([webgl.api, native.api], ["WebGL", "WebGPU"]);
    assert.ok(webgl.detail > 0.2, `the WebGL still frame shows the arena (${webgl.detail})`);
    if (native.detail > 0.2) {
      const diff = difference(webgl.image, native.image);
      console.log(JSON.stringify({ stillQuarry: diff }));
      assert.ok(diff.mean < STILL_MEAN_TOLERANCE, `WebGL frame differs: ${JSON.stringify(diff)}`);
      assert.ok(diff.large < STILL_LARGE_TOLERANCE, `WebGL frame differs: ${JSON.stringify(diff)}`);
    } else {
      // Software WebGPU in headless Chrome (SwiftShader) presents nothing to capture.
      console.log("This browser's WebGPU frames cannot be captured; frames not compared.");
    }
  } else {
    console.log("No WebGPU adapter: the WebGL and WebGPU frames are not compared.");
  }
  // Count the context's live GL objects outside the renderer: its own counters
  // cannot show a GL object it forgot to delete.
  {
    const { page } = await newPage();
    await freezeLoop(page);
    await seedGame(page, 424242);
    await page.addInitScript(() => {
      const prototype = WebGL2RenderingContext.prototype;
      const live = {};
      window.glObjects = () =>
        Object.fromEntries(Object.entries(live).map(([kind, objects]) => [kind, objects.size]));
      for (const kind of [
        "Buffer",
        "Texture",
        "Sampler",
        "Framebuffer",
        "Renderbuffer",
        "Program",
        "Shader",
        "VertexArray",
      ]) {
        const objects = (live[kind] = new Set());
        const create = prototype[`create${kind}`];
        const remove = prototype[`delete${kind}`];
        prototype[`create${kind}`] = function (...args) {
          const object = create.apply(this, args);
          if (object) objects.add(object);
          return object;
        };
        prototype[`delete${kind}`] = function (object) {
          objects.delete(object);
          return remove.call(this, object);
        };
      }
      const instantiate = WebAssembly.instantiate;
      WebAssembly.instantiate = async (...args) => {
        const result = await instantiate(...args);
        window.engineMemory ??= (result.instance ?? result).exports.memory;
        return result;
      };
    });
    await page.goto(pageUrl("?webgl&debug&autoplay&map=superstress"));
    await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing");
    const resets = await page.evaluate(
      ({ resets, pose }) => {
        const samples = [];
        for (let i = 0; i < resets; i++) {
          window.sloppy.start();
          window.engine.draw(pose);
          samples.push({
            objects: window.glObjects(),
            memory: window.engineMemory?.buffer.byteLength,
            error: window.sloppy.error(),
            draws: window.sloppy.stats().drawCalls,
          });
        }
        return samples;
      },
      { resets: RESETS, pose: STILL_POSE },
    );
    console.log(JSON.stringify({ scrapYardResets: resets }));
    assert.ok(resets[0].memory > 0, "the probe sees the engine's Wasm memory");
    const settled = resets[SETTLING_RESETS];
    for (const sample of resets.slice(SETTLING_RESETS)) {
      assert.deepEqual(sample.objects, settled.objects, "a reset frees its round's GL objects");
      assert.equal(sample.memory, settled.memory, "resets stop growing the Wasm memory");
      assert.equal(sample.error, null);
      assert.equal(sample.draws, settled.draws);
    }
    const sizes = [];
    for (const [width, height] of [
      [1100, 700],
      [800, 600],
      [WIDTH, HEIGHT],
      [800, 600],
      [WIDTH, HEIGHT],
    ]) {
      await page.setViewportSize({ width, height });
      sizes.push(
        await page.evaluate((pose) => {
          window.sloppy.exactResolution();
          window.engine.draw(pose);
          return { objects: window.glObjects(), error: window.sloppy.error() };
        }, STILL_POSE),
      );
    }
    console.log(JSON.stringify({ sizes }));
    for (const size of sizes) {
      assert.deepEqual(size.objects, settled.objects, "a resize deletes the targets it replaces");
      assert.equal(size.error, null);
    }
    const lost = await page.evaluate(async () => {
      const gl = document.querySelector("canvas").getContext("webgl2");
      const extension = gl.getExtension("WEBGL_lose_context");
      if (!extension) return "unavailable";
      extension.loseContext();
      await new Promise((resolve) => setTimeout(resolve, 50));
      return window.sloppy.error();
    });
    console.log(JSON.stringify({ contextLoss: lost }));
    if (lost !== "unavailable") assert.match(lost, /WebGL context was lost/);
    await page.close();
  }
  // A GL error while drawing reaches `Game.error()` by the next periodic check.
  {
    const { page } = await newPage();
    await freezeLoop(page);
    await page.goto(pageUrl("?webgl&autoplay"));
    await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing");
    const error = await page.evaluate(
      ({ draws, pose }) => {
        const gl = document.querySelector("canvas").getContext("webgl2");
        // INVALID_ENUM, and no state the backend's cache tracks changes.
        gl.enable(0);
        for (let i = 0; i < draws && !window.sloppy.error(); i++) window.engine.draw(pose);
        return window.sloppy.error();
      },
      { draws: ERROR_CHECK_DRAWS, pose: STILL_POSE },
    );
    console.log(JSON.stringify({ drawingError: error }));
    assert.match(error ?? "", /WebGL error 0x0500 while drawing/);
    await page.close();
  }
  // Only the context loss and the GL error caused on purpose are reported.
  const expected = [/WebGL context was lost/, /INVALID_ENUM: enable/, /WebGL error 0x0500/];
  const unexpected = errors.filter((error) => !expected.some((pattern) => pattern.test(error)));
  assert.deepEqual(unexpected, []);
  console.log("PASS");
} finally {
  await browser.close();
}
