import { chromium } from "playwright";
import { headless } from "./browser-helpers.mjs";
import { createCanvas, loadImage } from "@napi-rs/canvas";
import fs from "node:fs";
import assert from "node:assert/strict";
const root = process.env.SLOPPY_ARTIFACT_DIR ?? "artifacts/performance/bundle-rendering",
  results = [];
fs.mkdirSync(root, { recursive: true });
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";
const browser = await chromium.launch({ channel: "chrome", headless });
try {
  for (const map of ["village", "harbor", "quarry"]) {
    const page = await browser.newPage({
        viewport: { width: 1440, height: 900 },
        deviceScaleFactor: 1,
      }),
      errors = [];
    page.on("pageerror", (e) => errors.push(e.stack));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(m.text());
    });
    await page.addInitScript(() => {
      let seed = 7654321;
      Math.random = () => {
        seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
        return seed / 4294967296;
      };
      const raf = requestAnimationFrame.bind(window);
      window.nextFrame = () => new Promise(raf);
      window.requestAnimationFrame = (cb) =>
        raf((t) => {
          if (cb.name !== "loop" || !window.sloppy) cb(t);
        });
    });
    await page.goto(`${url}?autoplay&map=${map}`);
    await page.waitForFunction(() => !!window.sloppy);
    const recording = await page.evaluate(async () => {
      const d = window.sloppy,
        b = d.view.renderer.backend;
      window.bundleRecords = [];
      const finish = b.finishBundle.bind(b);
      b.finishBundle = (c, bundle) => {
        finish(c, bundle);
        window.bundleRecords.push({
          camera: bundle.camera?.uuid,
          records: b.get(bundle).renderObjects.length,
          expected: d.view.partBatches.batches.length,
        });
      };
      d.sim.seed = 12345;
      d.sim.roundCount = 12;
      d.sim.humanTeam = 0;
      d.start();
      document.querySelector("#loading")?.remove();
      for (let i = 0; i < 480; i++) {
        if (i % 6 === 0) await window.nextFrame();
        d.sim.step({ moveX: 0, moveZ: 0, aim: 0, fire: false, mine: false, boost: false }, true);
        for (const e of d.sim.events.splice(0)) d.view.event(e, false);
        d.view.render(d.sim, 1, 1 / 60, false);
      }
      return window.bundleRecords;
    });
    for (const overview of [false, true]) {
      const arrays = [];
      for (const cached of [true, false]) {
        await page.evaluate(
          async ({ cached, overview }) => {
            const d = window.sloppy;
            d.view.partBatches.group.isBundleGroup = cached;
            for (let i = 0; i < 3; i++) {
              await window.nextFrame();
              d.view.render(d.sim, 1, 0, overview);
              await d.view.renderer.waitForPipelineCompilation();
            }
            await window.nextFrame();
            d.view.render(d.sim, 1, 0, overview);
          },
          { cached, overview },
        );
        const png = await page.screenshot({
          path: `${root}/moving-${map}-${overview}-${cached ? "cached" : "reference"}.png`,
        });
        const canvas = createCanvas(1440, 900),
          ctx = canvas.getContext("2d");
        ctx.drawImage(await loadImage(png), 0, 0);
        arrays.push(ctx.getImageData(0, 0, 1440, 900).data);
      }
      let changed = 0,
        large = 0,
        sum = 0,
        max = 0;
      for (let i = 0; i < arrays[0].length; i += 4) {
        let peak = 0;
        for (let c = 0; c < 3; c++) {
          const delta = Math.abs(arrays[0][i + c] - arrays[1][i + c]);
          peak = Math.max(peak, delta);
          sum += delta;
        }
        if (peak) changed++;
        if (peak > 20) large++;
        max = Math.max(max, peak);
      }
      const result = {
        map,
        overview,
        changedPercent: (100 * changed) / (1440 * 900),
        largePercent: (100 * large) / (1440 * 900),
        meanError: sum / (1440 * 900 * 3),
        max,
        errors,
        recording,
      };
      results.push(result);
      console.log(result);
      fs.writeFileSync(`${root}/moving-camera.json`, JSON.stringify(results, null, 2));
      assert.equal(errors.length, 0);
      assert(
        recording.every((r) => r.records === r.expected),
        "every cached draw must keep an update record",
      );
      assert(result.largePercent < 0.04, "cached draws must follow camera movement");
    }
    await page.close();
  }
} finally {
  await browser.close();
}
