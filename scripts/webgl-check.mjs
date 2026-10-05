// The WebGL2 fallback engine. `?webgl` asks for it on any browser: on every standard
// map the page must download only the WebGL build, start a round through Battle
// Setup, drive and fire, and draw the arena (shadows included) without page, console,
// GL or engine errors, and without wgpu clearing an index buffer through a vector of
// zeros in the Wasm heap. Without `?webgl` the page must pick the build this browser
// supports by itself (WebGPU when it has an adapter, else WebGL) and download only
// that one. Where WebGPU has an adapter, a WebGPU device that fails must fall back
// to WebGL on the same canvas, and the two engines must draw the same still frame of
// the quarry (whose fixed scenery shadow is cached) closely alike.
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { createCanvas, loadImage } from "@napi-rs/canvas";
import {
  chooseMap,
  freezeLoop,
  gameUrl,
  launchGame,
  seedGame,
  startRound,
} from "./browser-helpers.mjs";

const output = "artifacts/performance/webgl";
const WIDTH = 1280;
const HEIGHT = 720;
/** Software WebGL (SwiftShader, on machines without a GPU) takes minutes to compile
 * an arena's programs; hardware takes seconds. */
const LOADING_TIMEOUT_MS = 300_000;
const MAPS = ["village", "harbor", "quarry", "stress-test", "superstress"];
/** The still frame both engines draw: the quarry's floor and cliffs from above. */
const STILL_POSE = [-40, 70, 40, 0, 0, 0];
/** WebGL may look simpler, but not different: mean channel difference out of 255,
 * and the share of pixels whose largest channel difference exceeds 48. Loose on
 * purpose; a broken cached shadow (the whole floor in shadow) measured 53 and 0.73. */
const STILL_MEAN_TOLERANCE = 16;
const STILL_LARGE_TOLERANCE = 0.15;
/** wgpu clears the never-written part of a bound buffer before a pass, and on WebGL an
 * index buffer by uploading zeros from a vector in the engine's Wasm heap, which keeps
 * that size for good. Mesh pages bind only their written prefix
 * (`MeshStore::index_buffer`), so no index upload this large is all zeros; smaller
 * ones are left out, since a tiny mesh's own indices can be. */
const ZERO_INDEX_UPLOAD_BYTES = 1024;
mkdirSync(output, { recursive: true });

async function pixels(png) {
  const canvas = createCanvas(WIDTH, HEIGHT);
  const context = canvas.getContext("2d");
  context.drawImage(await loadImage(png), 0, 0);
  return context.getImageData(0, 0, WIDTH, HEIGHT).data;
}

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
  const url = new URL(gameUrl);
  url.search = search;
  await page.goto(url.href);
  await page.waitForFunction(
    () => document.querySelector("#startup-overlay")?.dataset.state === "ready",
  );
}

/** Play a moment of a round: drive, fire, and report what the frame drew. */
async function play(page, name) {
  await startRound(page);
  await page.keyboard.down("KeyW");
  await page.waitForTimeout(1500);
  await page.keyboard.up("KeyW");
  await page.mouse.click(WIDTH / 2, HEIGHT / 3);
  await page.waitForTimeout(1500);
  const stats = await page.evaluate(() => JSON.parse(window.sloppy.game.stats_json()));
  const shot = await page.screenshot({ path: `${output}/${name}.png` });
  return {
    graphicsApi: stats.graphicsApi,
    drawCalls: stats.drawCalls,
    shadowDrawCalls: stats.shadowDrawCalls,
    latePipelines: stats.latePipelines,
    detail: await detail(shot),
    engineError: await page.evaluate(() => window.sloppy.game.error() ?? null),
    human: await page.evaluate(() => Boolean(window.engine.state().human)),
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
    `${name}: wgpu cleared index buffer bytes through a vector of zeros`,
  );
}

/** The quarry drawn once, still, from `STILL_POSE` by the engine `search` picks. */
async function stillQuarry(search, name) {
  const { page } = await newPage();
  await seedGame(page, 424242);
  await freezeLoop(page);
  const url = new URL(gameUrl);
  url.search = `?autoplay&map=quarry${search}`;
  await page.goto(url.href);
  await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing");
  const api = await page.evaluate((pose) => {
    for (const element of document.querySelectorAll("#overlay, #hud, #fps, #loading")) {
      element.style.display = "none";
    }
    window.engine.draw(pose);
    window.engine.draw(pose);
    return window.engine.stats().graphicsApi;
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
  const { graphicsApi } = await page.evaluate(() => JSON.parse(window.sloppy.game.stats_json()));
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
  }
  if (webgpu) {
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
  // Independently count real GL objects through resets and attachment resizes.
  // Renderer counters alone cannot detect a forgotten delete in a new backend.
  {
    const { page } = await newPage();
    await freezeLoop(page);
    await seedGame(page, 424242);
    await page.addInitScript(() => {
      const prototype = WebGL2RenderingContext.prototype;
      const live = new Map();
      window.glResources = () =>
        Object.fromEntries([...live].map(([kind, objects]) => [kind, objects.size]));
      for (const kind of [
        "Buffer",
        "Texture",
        "Sampler",
        "Framebuffer",
        "Renderbuffer",
        "Program",
        "VertexArray",
        "Sync",
      ]) {
        const objects = new Set();
        live.set(kind, objects);
        const createName = kind === "Sync" ? "fenceSync" : `create${kind}`;
        const create = prototype[createName],
          remove = prototype[`delete${kind}`];
        prototype[createName] = function (...args) {
          const object = create.apply(this, args);
          if (object) objects.add(object);
          return object;
        };
        prototype[`delete${kind}`] = function (object) {
          objects.delete(object);
          return remove.call(this, object);
        };
      }
      const getContext = HTMLCanvasElement.prototype.getContext;
      HTMLCanvasElement.prototype.getContext = function (type, ...args) {
        const context = getContext.call(this, type, ...args);
        if (type === "webgl2" && context) window.testGl = context;
        return context;
      };
      const instantiate = WebAssembly.instantiate;
      WebAssembly.instantiate = async function (...args) {
        const result = await instantiate(...args);
        window.testMemory = (result.instance ?? result).exports.memory;
        return result;
      };
      const streaming = WebAssembly.instantiateStreaming;
      WebAssembly.instantiateStreaming = async function (...args) {
        const result = await streaming(...args);
        window.testMemory = result.instance.exports.memory;
        return result;
      };
    });
    const url = new URL(gameUrl);
    url.search = "?webgl&debug&autoplay&map=superstress";
    await page.goto(url.href);
    await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing");
    const resets = await page.evaluate(() => {
      const samples = [];
      for (let i = 0; i < 32; i++) {
        window.sloppy.start();
        window.engine.draw([-40, 70, 40, 0, 0, 0]);
        samples.push({
          objects: window.glResources(),
          memory: window.testMemory?.buffer.byteLength,
          error: window.sloppy.game.error() ?? null,
          draws: window.engine.stats().drawCalls,
        });
      }
      return samples;
    });
    console.log(JSON.stringify({ scrapYardResets: resets }));
    assert.ok(
      Number.isInteger(resets[0].memory) && resets[0].memory > 0,
      "the probe observes actual Wasm memory",
    );
    // The allocator can reach a new high-water mark over the first few worlds.
    // Keep the entire trace, then require a flat second half of a longer run.
    for (const sample of resets.slice(16)) {
      assert.deepEqual(sample.objects, resets[16].objects, "Scrap Yard resets release GL objects");
      assert.equal(
        sample.memory,
        resets[16].memory,
        "Scrap Yard resets stop growing Wasm memory after warm-up",
      );
      assert.equal(sample.error, null);
      assert.equal(sample.draws, resets[16].draws);
    }
    const sizes = [];
    for (const [width, height] of [
      [1100, 700],
      [800, 600],
      [1280, 720],
      [800, 600],
      [1280, 720],
    ]) {
      await page.setViewportSize({ width, height });
      sizes.push(
        await page.evaluate(() => {
          window.sloppy.exactResolution();
          window.engine.draw([-40, 70, 40, 0, 0, 0]);
          return { objects: window.glResources(), error: window.sloppy.game.error() ?? null };
        }),
      );
    }
    for (const size of sizes) {
      assert.deepEqual(
        size.objects,
        resets.at(-1).objects,
        "resizing releases superseded attachments",
      );
      assert.equal(size.error, null);
    }
    assert.equal(await page.evaluate(() => window.testGl.getError()), 0);
    console.log(JSON.stringify({ sizes }));
    assert.deepEqual(errors, []);
    const contextLoss = await page.evaluate(async () => {
      const extension = window.testGl.getExtension("WEBGL_lose_context");
      if (!extension) return "unavailable";
      extension.loseContext();
      await new Promise((resolve) => setTimeout(resolve, 50));
      return window.sloppy.game.error();
    });
    if (contextLoss !== "unavailable") assert.match(contextLoss, /WebGL context lost/);
    console.log(JSON.stringify({ contextLoss }));
    await page.close();
  }
  // A GL command error during gameplay must reach Game.error() within the
  // bounded runtime polling window, even when the simulation is paused.
  {
    const { page } = await newPage();
    await freezeLoop(page);
    const url = new URL(gameUrl);
    url.search = "?webgl&autoplay";
    await page.goto(url.href);
    await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing");
    const error = await page.evaluate(() => {
      const gl = document.querySelector("canvas#game").getContext("webgl2");
      gl.enable(0); // INVALID_ENUM, without changing any tracked state.
      for (let i = 0; i < 32; i++) {
        try {
          window.engine.draw([-40, 70, 40, 0, 0, 0]);
        } catch {
          break;
        }
      }
      return window.sloppy.game.error();
    });
    assert.match(error, /WebGL frame: GL error 0x0500/);
    console.log(JSON.stringify({ runtimeError: error }));
    await page.close();
  }
  assert.ok(
    errors.every(
      (error) =>
        error.includes("WebGL context lost") ||
        error.includes("WebGL frame: GL error 0x0500") ||
        error.includes("WebGL: INVALID_ENUM: enable"),
    ),
    errors.join("\n"),
  );
  console.log("PASS");
} finally {
  await browser.close();
}
