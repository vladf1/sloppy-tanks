// The page's choice of graphics backend and its fallbacks: a normal load downloads only
// the WebGPU shell (its menu and engine halves); `?webgl` and a browser without WebGPU load only the WebGL2 one, and
// so does an adapter that refuses all, after dropping the WebGPU shell it started on
// while it asked (`dropped`), even when it refuses only once that shell's engine half is
// on its way; a device that fails after an adapter reloads once into
// `?webgl`; and with neither backend the loading view says why, with no reload loop.
// Each case plays a round on the backend it ends on (Stats for nerds' Graphics API).
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import {
  clickWidget,
  gameUrl,
  launchChrome,
  collectErrors,
  nextFrame,
  untilState,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/webgl-check";
mkdirSync(out, { recursive: true });

/** Browser failures each case stages before the page runs. */
const STAGED = {
  normal: () => {},
  noWebgpu: () => {
    delete Navigator.prototype.gpu;
  },
  noAdapter: () => {
    GPU.prototype.requestAdapter = async () => null;
  },
  // No adapter, answered only once the menu half is in: the WebGPU engine half has begun
  // downloading (and maybe compiling) by then, and must not stand in for WebGL's.
  lateNoAdapter: () => {
    GPU.prototype.requestAdapter = () =>
      new Promise((resolve) => {
        const answer = () =>
          performance.getEntriesByName("sloppy:downloaded").length
            ? resolve(null)
            : setTimeout(answer, 5);
        answer();
      });
  },
  deviceFails: () => {
    GPUAdapter.prototype.requestDevice = async () => {
      throw new Error("staged device failure");
    };
  },
  neither: () => {
    GPUAdapter.prototype.requestDevice = async () => {
      throw new Error("staged device failure");
    };
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (kind, ...rest) {
      return kind === "webgl2" ? null : getContext.call(this, kind, ...rest);
    };
  },
};

// Each shell downloads as its menu half and its engine half (`scripts/split-wasm.mjs`).
const WEBGPU = ["sloppy-tanks_bg", "sloppy-tanks_engine"];
const WEBGL = ["webgl_bg", "webgl_engine"];
const CASES = [
  { name: "normal", stage: "normal", query: "", wasm: WEBGPU, api: "WebGPU" },
  { name: "forced WebGL", stage: "normal", query: "webgl", wasm: WEBGL, api: "WebGL" },
  { name: "no WebGPU", stage: "noWebgpu", query: "", wasm: WEBGL, api: "WebGL" },
  {
    name: "no adapter",
    stage: "noAdapter",
    query: "",
    wasm: WEBGL,
    dropped: WEBGPU,
    api: "WebGL",
  },
  {
    name: "late no adapter",
    stage: "lateNoAdapter",
    query: "",
    wasm: WEBGL,
    dropped: WEBGPU,
    api: "WebGL",
  },
  {
    name: "device fails",
    stage: "deviceFails",
    query: "",
    wasm: [...WEBGPU, ...WEBGL],
    api: "WebGL",
  },
  {
    name: "neither backend",
    stage: "neither",
    query: "",
    wasm: [...WEBGPU, ...WEBGL],
    api: null,
  },
];

const browser = await launchChrome();
try {
  for (const { name, stage, query, wasm, dropped = [], api } of CASES) {
    const context = await browser.newContext({
      viewport: { width: 1280, height: 800 },
      deviceScaleFactor: 1,
    });
    await context.addInitScript(STAGED[stage]);
    const page = await context.newPage();
    const errors = [];
    collectErrors(page, errors);
    const downloads = [];
    page.on("request", (request) => {
      const url = new URL(request.url());
      const file = url.pathname.split("/").pop();
      // The dev server answers a `?url` import with a module naming the file.
      if (file.endsWith(".wasm") && !url.searchParams.has("import")) downloads.push(file);
    });
    let navigations = 0;
    page.on("framenavigated", (frame) => {
      if (frame === page.mainFrame()) navigations += 1;
    });
    const url = new URL(gameUrl);
    url.search = query ? `automation&${query}` : "automation";
    await page.goto(url.href);
    await page.waitForFunction(() => document.documentElement.dataset.shell, null, {
      timeout: 60000,
    });
    const shell = await page.evaluate(() => document.documentElement.dataset.shell);
    await page.screenshot({ path: `${out}/${name.replaceAll(" ", "-")}.png` });
    const fetched = wasm.map((part) => downloads.filter((file) => file.includes(part)).length);
    assert.deepEqual(
      fetched,
      wasm.map(() => 1),
      `${name}: downloads ${downloads.join(", ")}`,
    );
    // A dropped build's request may or may not have gone out before it was aborted.
    const started = dropped.map((part) => downloads.filter((file) => file.includes(part)).length);
    assert.ok(
      started.every((count) => count <= 1),
      `${name}: downloads ${downloads.join(", ")}`,
    );
    assert.equal(
      downloads.length,
      wasm.length + started.reduce((sum, count) => sum + count, 0),
      `${name}: downloads ${downloads.join(", ")}`,
    );
    if (api) {
      assert.equal(shell, "ready", `${name}: the shell started`);
      await untilState(page, (state) => state.ready, { what: `${name}: the arena` });
      await clickWidget(page, "GO!");
      const state = await untilState(page, (state) => state.inRound, {
        extras: ["nerd"],
        what: `${name}: the round`,
      });
      const rows = state.extras.nerd.flatMap((section) => section.rows ?? []);
      const graphics = rows.find((row) => row.label === "Graphics API")?.value;
      assert.equal(graphics, api, `${name}: Graphics API`);
      assert.equal((await nextFrame(page)).error, null, `${name}: renderer errors`);
    } else {
      assert.equal(shell, "failed", `${name}: the loading view says it failed`);
      const text = await page.evaluate(() => document.getElementById("boot").innerText);
      assert.match(text, /could not start/i, `${name}: ${text}`);
      // Give a reload loop time to show itself.
      await page.waitForTimeout(3000);
      assert.ok(navigations <= 2, `${name}: ${navigations} navigations`);
    }
    assert.deepEqual(errors, [], `${name}: page errors`);
    console.log(`webgl-check: ${name}: ${api ?? "failed as it should"} (${downloads.join(", ")})`);
    await context.close();
  }
  console.log("webgl-check: backend choice and fallbacks passed");
} finally {
  await browser.close();
}
