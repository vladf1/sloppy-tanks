import assert from "node:assert/strict";
import { build, preview } from "vite";
import { chromium } from "playwright";
import { headless } from "./browser-helpers.mjs";
import { mkdir, writeFile } from "node:fs/promises";
import { checkMultiplayerMenu, waitForRoomBrowser } from "./multiplayer-ui-assertions.mjs";

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
const browser = await chromium.launch({ channel: "chrome", headless });
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
  await page.locator("#nerd-stats button").click();
  assert.match(await page.locator("#nerd-stats-details").innerText(), /Physics/);
  await page.keyboard.press("n");
  assert.equal(await page.locator("#nerd-stats-details").isVisible(), false);
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
  await page.screenshot({ path: `${directory}/single-player-loading.png` });
  console.log("Single-player: zero multiplayer chunk requests, zero sockets, no browser errors.");
  const networkRequests = [];
  const networkPage = await browser.newPage();
  networkPage.on("request", (request) => networkRequests.push(request.url()));
  // The multiplayer tab loads only the room list; its styles are inline with the menu.
  await networkPage.goto(server.resolvedUrls.local[0] + "?multiplayer");
  await waitForRoomBrowser(networkPage);
  await checkMultiplayerMenu(networkPage);
  const listingRequests = networkRequests.length;
  for (const file of networkStyles) {
    assert.ok(
      !networkRequests.some((url) => new URL(url).pathname.endsWith(`/${file}`)),
      `The room list does not load in-room styles: ${file}`,
    );
  }
  // A room page loads the multiplayer client and its extracted stylesheet.
  await networkPage.goto(server.resolvedUrls.local[0] + "?room=ABCD2345");
  await networkPage.locator("#join-room").waitFor();
  await checkMultiplayerMenu(networkPage);
  assert.ok(networkStyles.size > 0, "Build exposes multiplayer's extracted CSS");
  for (const file of networkStyles) {
    assert.ok(
      networkRequests
        .slice(listingRequests)
        .some((url) => new URL(url).pathname.endsWith(`/${file}`)),
      `Multiplayer loads ${file}`,
    );
  }
  for (const url of networkRequests) {
    assert.ok(!url.endsWith(".wasm"), "Multiplayer must not download client physics");
    const path = new URL(url).pathname;
    const modules = [...chunks].find(([file]) => path.endsWith(`/${file}`))?.[1].modules ?? [];
    assert.ok(
      !modules.some((id) => /\/src\/game\/simulation\.ts$/.test(id)),
      "Multiplayer must not download a client simulation",
    );
    assert.ok(
      !modules.some((id) => /\/@dimforge\//.test(id)),
      `Multiplayer must not download Rapier JS: ${path}`,
    );
  }
  result.networkRequests = networkRequests;
  console.log("Multiplayer: no client simulation, Rapier JS or Rapier WASM requests.");
} finally {
  await browser.close();
  await new Promise((resolve) => server.httpServer.close(resolve));
  await mkdir(directory, { recursive: true });
  await writeFile(`${directory}/single-player-loading.json`, JSON.stringify(result, null, 2));
}
