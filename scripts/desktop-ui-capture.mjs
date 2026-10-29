// Manual visual evidence for the native shell against a pinned production web build.
// Native fixtures and pictures come from ui::snapshots; production's own UI class
// renders the identical fixtures. Only local response copies expose the class.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { launchChrome, menuReady } from "./browser-helpers.mjs";

const arguments_ = process.argv.slice(2);
const option = (name, fallback) => {
  const index = arguments_.indexOf(name);
  return index < 0 ? fallback : arguments_[index + 1];
};
const native = resolve(option("--native", "artifacts/performance/desktop-ui/native"));
const output = resolve(option("--out", "artifacts/performance/desktop-ui/reference"));
const reference = option("--reference", "https://sloppy-tanks.fridman.me/");
await mkdir(output, { recursive: true });
const fixtures = await Promise.all(
  (await readdir(native))
    .filter((name) => name.endsWith(".json"))
    .sort()
    .map(async (name) => JSON.parse(await readFile(resolve(native, name), "utf8"))),
);
assert.ok(fixtures.length, "Capture native snapshots first; see scripts/README.md");
const [width, height] = fixtures[0].size;
const browser = await launchChrome();
const errors = [];
const sources = [];
const screens = [];
try {
  const context = await browser.newContext({
    viewport: { width, height },
    deviceScaleFactor: fixtures[0].scale,
    reducedMotion: "reduce",
  });
  // Stop gameplay after preparation, while allowing layout/respawn animation callbacks.
  await context.addInitScript(() => {
    const raf = window.requestAnimationFrame.bind(window);
    window.__gauntletRaf = raf;
    window.requestAnimationFrame = (callback) =>
      raf((time) => {
        if (!window.__gauntletFrozen) callback(time);
      });
  });
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  const modules = new Set();
  await page.route("**/*.js", async (route) => {
    const response = await route.fetch();
    const source = await response.text();
    let body = source;
    const ui = source.match(/([\w$]+)=class\{choices;actions;overlay;respawnLayer;hud;toast;feed;/);
    if (ui) body += `\n;globalThis.__gauntletUI = ${ui[1]};`;
    const networkUI = source.match(/([\w$]+)=class\{root;actions;canvas;panel;lastLobby;/);
    if (networkUI) body += `\n;globalThis.__gauntletNetworkUI = ${networkUI[1]};`;
    if (source.includes("`nerd-stats`")) modules.add(route.request().url());
    sources.push({
      url: route.request().url(),
      sha256: createHash("sha256").update(source).digest("hex"),
      exposedUI: Boolean(ui),
    });
    await route.fulfill({ response, body });
  });
  await page.goto(reference, { waitUntil: "domcontentloaded" });
  await menuReady(page, { timeout: 120_000 });
  const html = await page.content();
  const client = html.match(/import\("([^"\s]*\/client-[^"\s]+\.js)"\)/)?.[1];
  const browserModule = html.match(/import\("([^"\s]*\/room-browser-[^"\s]+\.js)"\)/)?.[1];
  const clientCss = html.match(/"([^"\s]*\/client-[^"\s]+\.css)"/)?.[1];
  assert.ok(client && clientCss && browserModule, "Find production multiplayer assets");
  await page.evaluate(
    async ([client, rooms]) => {
      await import(client);
      window.__gauntletRoomBrowser = (await import(rooms)).RoomBrowser;
    },
    [new URL(client, reference).href, new URL(browserModule, reference).href],
  );
  const cssResponse = await page.request.get(new URL(clientCss, reference).href);
  const css = await cssResponse.text();
  await page.addStyleTag({ content: css });
  sources.push({
    url: new URL(clientCss, reference).href,
    sha256: createHash("sha256").update(css).digest("hex"),
  });
  const healthResponse = await page.request.get(new URL("health", reference).href);
  assert.ok(healthResponse.ok(), "Production build metadata must be available");
  const health = await healthResponse.json();
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all([...document.images].map((image) => image.decode()));
    window.__gauntletFrozen = true;
    window.__gauntletSetup = document.querySelector("#startup-overlay").outerHTML;
  });
  await page.addStyleTag({
    content: `*,*::before,*::after{animation:none!important;transition:none!important}
      canvas#game{visibility:hidden} #app{min-height:100vh;background:#708266}
      .build-label{visibility:hidden}`,
  });
  assert.equal(await page.evaluate(() => typeof window.__gauntletUI), "function");
  assert.equal(await page.evaluate(() => typeof window.__gauntletNetworkUI), "function");
  assert.equal(modules.size, 1, "Find exactly one production HUD/stats module");
  const moduleUrl = [...modules][0];
  await page.evaluate(async (url) => {
    // Stats for nerds lays out the engine's rows, which the fixtures carry.
    const found = Object.values(await import(url)).filter(
      (value) => typeof value === "function" && value.prototype?.frame && value.prototype?.refresh,
    );
    if (found.length !== 1) throw new Error("Production UI exports changed; update capture");
    window.__gauntletNerd = found[0];
  }, moduleUrl);

  for (const fixture of fixtures) {
    // Room network views have a separate renderer, captured by the room scenarios.
    await page.evaluate(
      (fixture) => {
        const root = document.querySelector("#app");
        root.className = "";
        root.innerHTML = window.__gauntletSetup;
        root.style.background = fixture.hud.match.phase === "ready" ? "#0d1627" : "#708266";
        const choices = {
          humanKind: "balanced",
          humanTeam: 0,
          gameMode: fixture.hud.gameMode,
          mapMode: fixture.hud.mapMode,
          difficulty: fixture.hud.difficulty,
        };
        const noop = () => {};
        const state = fixture.hud;
        if (fixture.room && !fixture.room.joining) {
          const room = fixture.room;
          const actions = new Proxy(
            { touchMode: () => "auto", graphics: () => "auto" },
            { get: (target, key) => target[key] ?? noop },
          );
          const ui = new window.__gauntletNetworkUI(root, room.room, actions);
          ui.status({ text: "", connected: true });
          if (room.outcome) ui.result(room.outcome);
          // The engine's words for the room travel in the fixture, as in its notices.
          ui.lobby({ lobby: room.lobby, playerId: room.playerId, menu: room.menu, arrivals: [] });
          ui.setMenu(room.menuOpen);
          ui.editingRules = room.editingRules;
          ui.editingChoice = room.editingChoice;
          ui.renderEditing();
          if (room.link.kind === "connecting") ui.showConnecting(room.link.title, room.link.text);
          if (room.link.kind === "ended") ui.ended(room.link);
          // The respawn card says the engine's sentence from the viewer's death event.
          if (room.deathCause) {
            const deathReport = { sentence: room.deathCause };
            ui.event({ type: "death", own: true, damageAngle: null, deathReport });
          }
          ui.update(state, 0, room.link.kind === "live");
          if (fixture.name === "room-settings") root.querySelector("#settings-open").click();
          root.querySelector(".network-help").open = room.controlsOpen;
          if (fixture.stats) {
            const stats = new window.__gauntletNerd(
              root,
              () => fixture.stats,
              () => true,
              { network: true },
            );
            stats.button.click();
          }
          return;
        }
        const ui = new window.__gauntletUI(root, choices, {
          start: noop,
          resume: noop,
          restart: noop,
          endBattle: noop,
          pause: noop,
          setting: noop,
          touchMode: () => "auto",
          setTouchMode: noop,
          graphics: () => "auto",
          setGraphics: noop,
          selectAmmo: noop,
          pauseReport: () => fixture.pause,
        });
        window.__gauntletCurrent = ui;
        ui.update(state, 0);
        document.querySelector("#startup-overlay")?.remove();
        ui.viewerPoint = () =>
          fixture.viewer && !fixture.cockpit
            ? { x: fixture.viewer[0], y: fixture.viewer[1] }
            : null;
        ui.deathReport = fixture.death;
        ui.respawnTotal = fixture.respawnTotal;
        ui.lastPhase = "";
        ui.feedRows = fixture.feed.map((entry, index) => ({
          names: [entry.killer, entry.victim],
          ownKill: entry.ownKill,
          ownDeath: entry.ownDeath,
          cause: entry.cause,
          icon: entry.icon,
          sides: [entry.killerTeam, entry.victimTeam].map((side) =>
            side === 0 ? "blue" : side === 1 ? "red" : "neutral",
          ),
          seq: index + 1,
          time: 5 - entry.age,
        }));
        if (fixture.ammoNotice) {
          ui.hud.querySelector("#ammo-notice").textContent = fixture.ammoNotice[0];
          ui.ammoNoticeTime = 3 - fixture.ammoNotice[1];
        }
        ui.hurtFlashTime = fixture.hitFlash === null ? 0 : 0.6 - fixture.hitFlash;
        if (fixture.damageRing) {
          ui.event(
            { type: "hurt", id: state.human.id, own: true, damageAngle: fixture.damageRing[0] },
            state,
          );
          ui.damageTime = 1.2 - fixture.damageRing[1];
          ui.hurtFlashTime = fixture.hitFlash === null ? 0 : 0.6 - fixture.hitFlash;
        }
        ui.update(state, 0);
        if (fixture.toast) {
          ui.toast.textContent = fixture.toast.text;
          ui.toast.dataset.tone = fixture.toast.tone;
          ui.toast.classList.add("visible");
        }
        ui.hud.classList.toggle("first-person", fixture.cockpit);
        ui.hud.querySelector("#view-mode").setAttribute("aria-pressed", String(fixture.cockpit));
        const hull = ui.hud.querySelector(".hull-compass .hull");
        if (hull) hull.style.transform = `rotate(${fixture.hullAngle}rad)`;
        const ghost = ui.hud.querySelector(".hull-ghost");
        if (ghost) ghost.style.width = `${fixture.hullGhost * 100}%`;
        if (fixture.name === "pause-controls") {
          document.querySelector('[aria-controls="pause-controls"]')?.click();
        }
        if (fixture.name === "settings") ui.settings.open();
        if (fixture.name.startsWith("multiplayer-") || fixture.room?.joining) {
          const setup = root.querySelector(".menu.start");
          setup.dataset.play = "multiplayer";
          setup.querySelectorAll('[role="tab"][data-play]').forEach((tab) => {
            const selected = tab.dataset.play === "multiplayer";
            tab.setAttribute("aria-selected", String(selected));
            document.getElementById(tab.getAttribute("aria-controls")).hidden = !selected;
          });
          const rooms = new window.__gauntletRoomBrowser(
            setup,
            setup.querySelector(".map-choice"),
            new URL("https://example.test"),
            () => choices,
            noop,
          );
          rooms.rooms = (fixture.rooms ?? []).map((room) => ({
            ...room,
            // Native test binaries use this sentinel; retain the same compatibility state.
            contentVersion:
              room.contentVersion === "test-content"
                ? fixture.referenceContentVersion
                : room.contentVersion,
          }));
          rooms.newRoomServer = "wss://example.test";
          rooms.name.value = fixture.playerName ?? "STEELTOE";
          rooms.shown = true;
          rooms.render();
          rooms.create.disabled = false;
          rooms.refresh.disabled = false;
          rooms.showPrompt();
          if (fixture.room?.joining) {
            setup.querySelector("#rooms-message").textContent = fixture.room.status;
            setup.querySelectorAll("button,input").forEach((control) => {
              control.disabled = true;
            });
          }
        }
        if (state.match.phase === "ready") {
          const status = document.querySelector("#startup-status");
          if (status) status.textContent = fixture.status;
          const start = document.querySelector("#start");
          if (start) {
            start.disabled = !fixture.ready;
            start.textContent = fixture.ready ? "GO!" : "LOADING…";
          }
        }
        document
          .querySelector("#respawn-layer")
          ?.classList.toggle("shown", !state.human.alive && state.match.phase === "playing");
        if (fixture.stats) {
          document.querySelector("#nerd-stats")?.remove();
          const stats = new window.__gauntletNerd(
            root,
            () => fixture.stats,
            () => true,
          );
          stats.button.click();
        }
      },
      { ...fixture, referenceContentVersion: health.contentVersion },
    );
    // These are the browser's real layout frames; gameplay remains held.
    await page.evaluate(
      () => new Promise((done) => window.__gauntletRaf(() => window.__gauntletRaf(done))),
    );
    await page.mouse.move(0, 0);
    const bounds = await page.evaluate(() =>
      Object.fromEntries(
        [
          ".menu.start",
          ".menu.pause-menu",
          "#settings",
          ".menu.results",
          ".menu.respawn",
          ".scoreboard",
          ".combat-status",
          ".hud-actions",
          "#feed",
          "#nerd-stats",
        ].map((selector) => [
          selector,
          [...document.querySelectorAll(selector)].map((element) => ({
            ...element.getBoundingClientRect().toJSON(),
          })),
        ]),
      ),
    );
    const path = resolve(output, `${fixture.name}.png`);
    await page.screenshot({ path });
    screens.push({ name: fixture.name, bounds, path });
    console.log(`Captured ${fixture.name} at ${width}×${height}`);
  }
  await writeFile(
    resolve(output, "capture.json"),
    JSON.stringify(
      {
        reference,
        health,
        viewport: { width, height, scale: fixtures[0].scale },
        instrumentation:
          "Production JavaScript exposes UI constructor; both shells receive native test fixtures. Animation is disabled; arena is a flat stand-in. Production files are unchanged.",
        sources,
        screens,
        errors,
      },
      null,
      2,
    ),
  );
  assert.deepEqual(errors, [], "Production capture reported JavaScript errors");
} finally {
  await browser.close();
}
