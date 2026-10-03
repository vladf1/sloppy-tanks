// The WebGL2 fallback engine. `?webgl` asks for it on any browser: on every standard
// map the page must download only the WebGL build, start a round through Battle
// Setup, drive and fire, and draw the arena (shadows included) without page, console,
// GL or engine errors. Without `?webgl` the page must pick the build this browser
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
const MAPS = ["village", "harbor", "quarry"];
/** The still frame both engines draw: the quarry's floor and cliffs from above. */
const STILL_POSE = [-40, 70, 40, 0, 0, 0];
/** WebGL may look simpler, but not different: mean channel difference out of 255,
 * and the share of pixels whose largest channel difference exceeds 48. Loose on
 * purpose; a broken cached shadow (the whole floor in shadow) measured 53 and 0.73. */
const STILL_MEAN_TOLERANCE = 16;
const STILL_LARGE_TOLERANCE = 0.15;
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
  };
}

function assertDrew(result, name) {
  assert.equal(result.engineError, null, name);
  assert.ok(result.human, `${name}: the player's tank is in the round`);
  assert.ok(result.drawCalls > 20, `${name}: the arena draws (${result.drawCalls} draw calls)`);
  assert.ok(result.shadowDrawCalls > 0, `${name}: the sun casts shadows`);
  assert.ok(result.detail > 0.2, `${name}: the frame shows the arena (${result.detail})`);
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
    await open(page, "?webgl");
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
  assert.deepEqual(errors, []);
  console.log("PASS");
} finally {
  await browser.close();
}
