import assert from "node:assert/strict";
import { build, preview } from "vite";
import { chromium } from "playwright";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
import { gzipSync } from "node:zlib";
import { checkMultiplayerMenu } from "./multiplayer-ui-assertions.mjs";

const directory = "artifacts/performance/multiplayer";
const outDir = `${directory}/loading-build`;
const chunks = new Map();
const networkStyles = new Set();
await build({
  logLevel: "warn",
  build: { outDir },
  plugins: [
    {
      name: "audit-multiplayer-loading",
      generateBundle(_options, bundle) {
        for (const item of Object.values(bundle)) {
          if (item.type !== "chunk") continue;
          const modules = Object.keys(item.modules);
          if (modules.some((id) => /\/src\/net\//.test(id))) {
            item.viteMetadata?.importedCss.forEach((file) => networkStyles.add(file));
          }
          assert.ok(
            !modules.some((id) => /\/(server|node_modules\/(wrangler|workerd|esbuild))\//.test(id)),
            `Server code leaked into ${item.fileName}`,
          );
          chunks.set(item.fileName, {
            bytes: Buffer.byteLength(item.code),
            network: modules.some((id) => /\/src\/net\//.test(id)),
            modules,
          });
        }
      },
    },
  ],
});
const server = await preview({ build: { outDir }, preview: { host: "127.0.0.1", port: 4179 } });
const browser = await chromium.launch({ channel: "chrome", headless: false });
const result = { requests: [], sockets: [], errors: [], chunks: Object.fromEntries(chunks) };
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.on("request", (request) => result.requests.push(request.url()));
  page.on("websocket", (socket) => result.sockets.push(socket.url()));
  page.on("pageerror", (error) => result.errors.push(error.message));
  await page.goto(server.resolvedUrls.local[0]);
  await page.locator("#start").click({ timeout: 60000 });
  await page.locator("#startup-overlay").waitFor({ state: "detached" });
  await page.waitForFunction(() => document.querySelector("#hud")?.style.opacity === "1");
  await page.keyboard.down("w");
  await page.waitForTimeout(500);
  await page.keyboard.up("w");
  await page.mouse.move(700, 400);
  await page.mouse.down();
  await page.waitForTimeout(1000);
  await page.mouse.up();
  await page.locator("#pause").click();
  await page.locator("#resume").click();
  for (const url of result.requests) {
    const path = new URL(url).pathname;
    const chunk = [...chunks].find(([file]) => path.endsWith(`/${file}`));
    assert.ok(!chunk?.[1].network, `Single-player loaded multiplayer code: ${path}`);
    assert.ok(
      ![...networkStyles].some((file) => path.endsWith(`/${file}`)),
      `Single-player loaded multiplayer styles: ${path}`,
    );
  }
  assert.deepEqual(result.sockets, [], "Single-player must open no sockets");
  assert.deepEqual(result.errors, []);
  assert.equal(await page.locator("#network-status, .network-menu").count(), 0);
  const hashes = async (path) => {
    const files = (await readdir(resolve(path, "assets"))).filter((file) =>
      /\.(js|css|wasm)$/.test(file),
    );
    const entries = await Promise.all(
      files.map(async (file) => {
        const bytes = await readFile(resolve(path, "assets", file));
        return [
          file,
          {
            bytes: bytes.length,
            gzipBytes: gzipSync(bytes).length,
            sha256: createHash("sha256").update(bytes).digest("hex"),
          },
        ];
      }),
    );
    return Object.fromEntries(entries);
  };
  result.assets = await hashes(outDir);
  const requestedAssets = [
    ...new Set(
      result.requests
        .map((url) => new URL(url).pathname.split("/assets/")[1])
        .filter((file) => file && result.assets[file]),
    ),
  ];
  result.requestedAssets = requestedAssets;
  if (process.env.SLOPPY_BASELINE_BUILD) {
    result.baselineAssets = await hashes(process.env.SLOPPY_BASELINE_BUILD);
    result.identicalToBaseline =
      JSON.stringify(result.assets) === JSON.stringify(result.baselineAssets);
    console.log("Browser assets identical to baseline:", result.identicalToBaseline);
    const total = (assets, key) =>
      Object.values(assets).reduce((sum, asset) => sum + asset[key], 0);
    result.assetChange = {
      bytes: total(result.assets, "bytes") - total(result.baselineAssets, "bytes"),
      gzipBytes: total(result.assets, "gzipBytes") - total(result.baselineAssets, "gzipBytes"),
    };
    console.log("All emitted JS/CSS/WASM asset change:", result.assetChange);
    const baselineHtml = await readFile(
      resolve(process.env.SLOPPY_BASELINE_BUILD, "index.html"),
      "utf8",
    );
    const baselineRequests = [
      ...new Set(
        [...baselineHtml.matchAll(/assets\/([^\s"'<>`]+\.(?:js|wasm|css))/g)].map(
          (match) => match[1],
        ),
      ),
    ];
    const sum = (assets, files, key) => files.reduce((value, file) => value + assets[file][key], 0);
    result.singlePlayerRequestedChange = {
      bytes:
        sum(result.assets, requestedAssets, "bytes") -
        sum(result.baselineAssets, baselineRequests, "bytes"),
      gzipBytes:
        sum(result.assets, requestedAssets, "gzipBytes") -
        sum(result.baselineAssets, baselineRequests, "gzipBytes"),
      htmlBytes:
        Buffer.byteLength(await readFile(resolve(outDir, "index.html"))) -
        Buffer.byteLength(baselineHtml),
    };
    console.log("Single-player requested JS/CSS/WASM change:", result.singlePlayerRequestedChange);
  }
  await page.screenshot({ path: `${directory}/single-player-loading.png` });
  console.log("Single-player: zero multiplayer chunk requests, zero sockets, no browser errors.");
  const networkRequests = [];
  const networkPage = await browser.newPage();
  networkPage.on("request", (request) => networkRequests.push(request.url()));
  await networkPage.goto(server.resolvedUrls.local[0] + "?multiplayer");
  await networkPage.locator("#join-room").waitFor();
  await checkMultiplayerMenu(networkPage);
  assert.ok(networkStyles.size > 0, "Build exposes multiplayer's extracted CSS");
  for (const file of networkStyles) {
    assert.ok(
      networkRequests.some((url) => new URL(url).pathname.endsWith(`/${file}`)),
      `Multiplayer loads ${file}`,
    );
  }
  for (const url of networkRequests) {
    assert.ok(!url.endsWith(".wasm"), "Multiplayer must not download client physics");
    const path = new URL(url).pathname;
    const chunk = [...chunks].find(([file]) => path.endsWith(`/${file}`));
    assert.ok(
      !chunk?.[1].modules.some((id) => /\/src\/game\/simulation\.ts$/.test(id)),
      "Multiplayer must not download a client simulation",
    );
  }
  result.networkRequests = networkRequests;
  console.log("Multiplayer: no client simulation or Rapier WASM requests.");
} finally {
  await browser.close();
  await new Promise((resolve) => server.httpServer.close(resolve));
  await mkdir(directory, { recursive: true });
  await writeFile(`${directory}/single-player-loading.json`, JSON.stringify(result, null, 2));
}
