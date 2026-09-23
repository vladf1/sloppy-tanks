import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve, extname } from "node:path";
import { gzipSync } from "node:zlib";
import { seedGame } from "./browser-helpers.mjs";

// Production files over gzip; every sample has an empty HTTP cache and storage.
const label = process.argv[2] ?? "combined";
const root = resolve(process.argv[3] ?? `artifacts/performance/loading/${label}`);
const repeats = Number(process.env.LOADING_RUNS ?? 5);
const output = "artifacts/performance/loading";
mkdirSync(output, { recursive: true });
const types = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".wasm": "application/wasm",
  ".webp": "image/webp",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".mp3": "audio/mpeg",
  ".woff2": "font/woff2",
};
const files = new Map();
const server = createServer((req, res) => {
  const relative =
    decodeURIComponent(new URL(req.url, "http://localhost").pathname).replace(
      /^\/sloppy-tanks\//,
      "",
    ) || "index.html";
  const path = resolve(root, relative);
  if (!path.startsWith(root + "/") || !existsSync(path)) {
    res.writeHead(404).end();
    return;
  }
  if (!files.has(path)) {
    const raw = readFileSync(path);
    const compressed = /\.(html|css|js|svg|wasm)$/.test(path);
    files.set(path, { body: compressed ? gzipSync(raw) : raw, compressed });
  }
  const { body, compressed } = files.get(path);
  res
    .writeHead(200, {
      "Content-Type": types[extname(path)] ?? "application/octet-stream",
      "Content-Length": body.length,
      "Cache-Control": "public, max-age=600",
      ...(compressed ? { "Content-Encoding": "gzip" } : {}),
    })
    .end(body);
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const url = `http://127.0.0.1:${server.address().port}/sloppy-tanks/`;
const browser = await chromium.launch({ channel: "chrome", headless: true });
const runs = [];
try {
  for (let i = 0; i < repeats; i++) {
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 1,
    });
    const page = await context.newPage();
    await seedGame(page, 424242);
    const cdp = await context.newCDPSession(page);
    await cdp.send("Network.enable");
    await cdp.send("Network.setCacheDisabled", { cacheDisabled: true });
    await cdp.send("Network.emulateNetworkConditions", {
      offline: false,
      latency: 50,
      downloadThroughput: 10_000_000 / 8,
      uploadThroughput: 1_000_000 / 8,
      connectionType: "cellular4g",
    });
    // Use the same fallback fonts to exclude external latency from A/B results.
    await context.route(/https:\/\/fonts\.(googleapis|gstatic)\.com\//, (route) =>
      route.fulfill({ status: 200, contentType: "text/css", body: "" }),
    );
    const errors = [];
    const pending = new Set();
    page.on("request", (r) => pending.add(r));
    page.on("requestfinished", (r) => pending.delete(r));
    page.on("requestfailed", (r) => {
      pending.delete(r);
      errors.push(`Download failed: ${r.url()}`);
    });
    page.on("response", (r) => {
      if (r.status() >= 400) errors.push(`HTTP ${r.status()}: ${r.url()}`);
    });
    page.on("pageerror", (e) => errors.push(e.message));
    await page.addInitScript(() => {
      performance.setResourceTimingBufferSize(10000);
      window.loadingAudit = { tasks: [], menu: 0, firstFrame: 0, menuClear: 0, readyAt: 0 };
      const checkMenu = () => {
        const loading = document.querySelector("#loading");
        if (
          document.querySelector("#start") &&
          (!loading || Number(getComputedStyle(loading).opacity) <= 0.05)
        ) {
          window.loadingAudit.menuClear = performance.now();
        } else {
          requestAnimationFrame(checkMenu);
        }
      };
      requestAnimationFrame(checkMenu);
      new PerformanceObserver((list) => {
        window.loadingAudit.tasks.push(
          ...list.getEntries().map((e) => ({ start: e.startTime, duration: e.duration })),
        );
      }).observe({ type: "longtask", buffered: true });
      new MutationObserver(() => {
        if (
          !window.loadingAudit.readyAt &&
          document.querySelector("#startup-overlay")?.dataset.state === "ready"
        ) {
          window.loadingAudit.readyAt = performance.now();
        }
        if (!window.loadingAudit.menu && document.querySelector("#start")) {
          window.loadingAudit.menu = performance.now();
          requestAnimationFrame(() =>
            requestAnimationFrame(() => {
              window.loadingAudit.firstFrame = performance.now();
            }),
          );
        }
      }).observe(document, {
        childList: true,
        subtree: true,
        attributes: true,
        attributeFilter: ["data-state"],
      });
    });
    await page.goto(url, { waitUntil: "load" });
    await page.waitForFunction(() => window.loadingAudit.firstFrame > 0);
    await page.waitForLoadState("networkidle");
    await page.waitForFunction(() => {
      const menu = document.querySelector("#startup-overlay");
      return window.loadingAudit.menuClear > 0 && (!menu || menu.dataset.state === "ready");
    });
    await page
      .locator(".tank-preview")
      .evaluateAll((images) =>
        Promise.all(
          images
            .filter((image) => image instanceof HTMLImageElement)
            .map((image) => image.decode()),
        ),
      );
    const deadline = Date.now() + 20000;
    while (pending.size && Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 50));
    }
    if (pending.size)
      throw new Error(`Unfinished downloads: ${[...pending].map((r) => r.url()).join(", ")}`);
    const result = await page.evaluate(() => {
      const entries = [
        ...performance.getEntriesByType("navigation"),
        ...performance.getEntriesByType("resource"),
      ].filter((e) => e.name.startsWith(location.origin));
      const resources = entries.map((e) => ({
        path: new URL(e.name).pathname.replace("/sloppy-tanks/", ""),
        bytes: e.encodedBodySize,
        decoded: e.decodedBodySize,
        end: e.responseEnd,
      }));
      const tasks = window.loadingAudit.tasks.filter(
        (e) => e.start < window.loadingAudit.firstFrame,
      );
      return {
        ...window.loadingAudit,
        firstPaint: performance.getEntriesByName("first-contentful-paint")[0]?.startTime ?? null,
        download: resources.reduce((n, e) => n + e.bytes, 0),
        decoded: resources.reduce((n, e) => n + e.decoded, 0),
        lastResource: Math.max(...resources.map((e) => e.end)),
        blocking: tasks.reduce((n, e) => n + Math.max(0, e.duration - 50), 0),
        resources,
      };
    });
    result.errors = errors;
    if (errors.length) throw new Error(errors.join("\n"));
    if (i === 0) await page.screenshot({ path: `${output}/${label}.png` });
    result.startDelay = await page.evaluate(
      () =>
        new Promise((resolve) => {
          const started = performance.now();
          document.querySelector("#start").click();
          const check = () => {
            if (
              document.querySelector("#overlay")?.style.display === "none" &&
              getComputedStyle(document.querySelector("#hud")).opacity === "1"
            ) {
              requestAnimationFrame(() => resolve(performance.now() - started));
            } else {
              requestAnimationFrame(check);
            }
          };
          requestAnimationFrame(check);
        }),
    );
    runs.push(result);
    console.log(
      `${label} ${i + 1}/${repeats}: ${(result.download / 1e6).toFixed(3)} MB; content ${Math.round(result.firstPaint)} ms; menu ${Math.round(result.firstFrame)} ms; ready ${Math.round(result.readyAt)} ms; GO ${Math.round(result.startDelay)} ms; blocking ${result.blocking} ms`,
    );
    await context.close();
  }
} finally {
  await browser.close();
  await new Promise((r) => server.close(r));
}
const median = (key) => runs.map((r) => r[key]).sort((a, b) => a - b)[Math.floor(runs.length / 2)];
const report = {
  label,
  date: new Date().toISOString(),
  browser: browser.version(),
  conditions: {
    repeats,
    downloadMbps: 10,
    latencyMs: 50,
    viewport: "1440x900",
    fonts: "excluded identically in every run",
    cpu: "unthrottled",
    compression: "gzip",
  },
  median: Object.fromEntries(
    [
      "download",
      "decoded",
      "firstPaint",
      "menu",
      "firstFrame",
      "menuClear",
      "readyAt",
      "startDelay",
      "lastResource",
      "blocking",
    ].map((key) => [key, median(key)]),
  ),
  runs,
};
writeFileSync(`${output}/${label}.json`, JSON.stringify(report, null, 2));
console.log(JSON.stringify(report.median));
