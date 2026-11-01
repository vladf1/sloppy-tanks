import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { createServer } from "node:net";
import { setTimeout as wait } from "node:timers/promises";
import { chromium } from "playwright";

/**
 * Browser checks run in headless Chrome so they never pop a window over the desktop;
 * WebGPU and Playwright's coordinate mouse/touch input work the same there. Set
 * SLOPPY_HEADED=1 to watch a check in a visible window.
 */
export const headless = !process.env.SLOPPY_HEADED;

/** The Vite dev server under test; `bun run dev` prints it. */
export const gameUrl = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";

/** SLOPPY_WEBGL=1 opens the game on its WebGL2 build (`?webgl`, `openShell`). */
const webglOnly = Boolean(process.env.SLOPPY_WEBGL);

/** Headless Chrome whose pages keep their timers and frames while another page is in front. */
export function launchChrome() {
  return chromium.launch({
    channel: "chrome",
    headless,
    args: [
      "--disable-background-timer-throttling",
      "--disable-renderer-backgrounding",
      "--disable-backgrounding-occluded-windows",
    ],
  });
}

/** Chrome's touch emulation warns of its own touch events, which cannot be cancelled. */
const TOUCH_EMULATION = /Ignored attempt to cancel a touch\w+ event with cancelable=false/;

/**
 * Collect a page's uncaught errors in `errors`, each after `prefix`. `consoleErrors` also
 * collects console.error output (shader compilation failures are only reported there).
 * @param {import("playwright").Page} page
 * @param {string[]} errors
 * @param {{ consoleErrors?: boolean, prefix?: string }} [options]
 */
export function collectErrors(page, errors, { consoleErrors = false, prefix = "" } = {}) {
  page.on("pageerror", (error) => errors.push(prefix + error.message));
  if (!consoleErrors) return;
  page.on("console", (message) => {
    // Fixture pages have no icon; Chrome's automatic favicon request is not an error.
    if (message.type() !== "error" || message.location().url.endsWith("/favicon.ico")) return;
    if (TOUCH_EMULATION.test(message.text())) return;
    errors.push(prefix + message.text());
  });
}

/**
 * Launch installed Chrome (`launchChrome`) with one context and page, a desktop's unless
 * the device options say otherwise (`hasTouch` and `isMobile` make a touch screen with a
 * coarse pointer). Errors from every page in the context are collected in `errors`
 * (`collectErrors`).
 * @param {{ viewport?: { width: number, height: number }, consoleErrors?: boolean,
 *   deviceScaleFactor?: number, hasTouch?: boolean, isMobile?: boolean }} [options]
 */
export async function launchGame({
  viewport = { width: 1600, height: 900 },
  consoleErrors = false,
  deviceScaleFactor = 1,
  hasTouch = false,
  isMobile = false,
} = {}) {
  const browser = await launchChrome();
  const context = await browser.newContext({
    viewport,
    screen: viewport,
    deviceScaleFactor,
    hasTouch,
    isMobile,
  });
  const errors = [];
  context.on("page", (page) => collectErrors(page, errors, { consoleErrors }));
  const page = await context.newPage();
  return { browser, context, page, errors };
}

/**
 * Open the game page with its automation hook (`?automation`, the shell's
 * `crates/desktop/src/automation.rs`), on WebGL2 with SLOPPY_WEBGL=1, and wait for the
 * shell's first frame. `query` adds
 * the page's own parameters (`"debug&map=harbor"`). Throws if the shell failed to start.
 * @param {import("playwright").Page} page
 * @param {string} [query]
 */
export async function openShell(page, query = "") {
  const url = new URL(gameUrl);
  const parameters = ["automation", ...(webglOnly ? ["webgl"] : []), ...(query ? [query] : [])];
  url.search = parameters.join("&");
  return openAddress(page, url.href);
}

/**
 * Open `address` (an invite link, a page's own address) and wait for the shell's first
 * frame, as `openShell` does. A `reload` reopens the page's own address.
 * @param {import("playwright").Page} page
 * @param {string | { reload: true }} address
 */
export async function openAddress(page, address) {
  if (typeof address === "string") await page.goto(address);
  else await page.reload();
  await page.waitForFunction(() => document.documentElement.dataset.shell, null, {
    timeout: 60000,
  });
  const shell = await page.evaluate(() => document.documentElement.dataset.shell);
  if (shell !== "ready") throw new Error(`the shell did not start: ${shell}`);
  // The loading view fades out over the first frames; screenshots should not catch it.
  await page.waitForFunction(() => {
    const boot = document.getElementById("boot");
    return !boot || getComputedStyle(boot).display === "none";
  });
  return nextFrame(page);
}

/**
 * The shell's state after its next presented frame, with the engine's JSON for each of
 * `extras` (`"hud"`, `"stats"`, `"nerd"`, `"debug"`, `"snapshot"`) in `state.extras`.
 * Throws with the page's reason once its loading view says the shell stopped, which
 * draws no further frame.
 * @param {import("playwright").Page} page
 * @param {string[]} [extras]
 */
export function nextFrame(page, extras = []) {
  return page.evaluate(
    (wanted) =>
      new Promise((resolve, reject) => {
        const root = document.documentElement;
        const stopped = () => {
          if (root.dataset.shell !== "failed") return false;
          reject(
            new Error(`the shell stopped: ${document.getElementById("boot-note")?.dataset.detail}`),
          );
          return true;
        };
        if (stopped()) return;
        const watch = new MutationObserver(() => stopped() && watch.disconnect());
        watch.observe(root, { attributes: true, attributeFilter: ["data-shell"] });
        window.sloppyShell
          .frame(wanted)
          .then(resolve, reject)
          .finally(() => watch.disconnect());
      }),
    extras,
  );
}

/**
 * Wait until `test(state)` holds for a presented frame; returns that state. `test` may
 * be async (to read the page's widgets or storage too). `extras` as in `nextFrame`.
 * @param {import("playwright").Page} page
 * @param {(state: any) => unknown} test
 * @param {{ extras?: string[], timeout?: number, what?: string }} [options]
 */
export async function untilState(page, test, { extras = [], timeout = 30000, what } = {}) {
  const start = Date.now();
  let state;
  while (Date.now() - start < timeout) {
    state = await nextFrame(page, extras);
    if (await test(state)) return state;
  }
  const last = JSON.stringify({ ...state, prefs: undefined, extras: undefined });
  throw new Error(`timed out waiting for ${what ?? test.toString()}: ${last}`);
}

/**
 * The widget labelled `label` on the last frame (a string matches the whole label, a
 * RegExp any part), or undefined.
 * @param {import("playwright").Page} page
 * @param {string | RegExp} label
 */
export async function findWidget(page, label) {
  const widgets = await page.evaluate(() => window.sloppyShell.widgets());
  return widgets.find((widget) =>
    typeof label === "string" ? widget.label === label : label.test(widget.label),
  );
}

/**
 * The widget labelled `label` once a frame lists it (an egui area that has just opened
 * lists its widgets a frame later).
 * @param {import("playwright").Page} page
 * @param {string | RegExp} label
 */
export async function untilWidget(page, label, { timeout = 15000 } = {}) {
  const start = Date.now();
  for (;;) {
    const widget = await findWidget(page, label);
    if (widget) return widget;
    if (Date.now() - start > timeout) throw new Error(`no widget ${label} on screen`);
    await nextFrame(page);
  }
}

/**
 * Click the middle of the widget labelled `label` with the real mouse, at its CSS
 * coordinates, once a frame shows it, and wait for the frame that takes the click.
 * Returns the widget.
 * @param {import("playwright").Page} page
 * @param {string | RegExp} label
 * @param {{ timeout?: number }} [options]
 */
export async function clickWidget(page, label, { timeout = 15000 } = {}) {
  const start = Date.now();
  for (;;) {
    const widget = await findWidget(page, label);
    if (widget?.enabled) {
      await page.mouse.click(widget.x + widget.width / 2, widget.y + widget.height / 2);
      // egui reads one click per widget a frame: let it see this one before the next.
      await nextFrame(page);
      return widget;
    }
    if (Date.now() - start > timeout) {
      const labels = (await page.evaluate(() => window.sloppyShell.widgets())).map(
        (shown) => shown.label,
      );
      throw new Error(`no enabled widget ${label} on screen; shown: ${labels.join(", ")}`);
    }
    await nextFrame(page);
  }
}

/**
 * Run a debug command of the hook (the old `window.sloppy` methods: `start`, `killHuman`,
 * `giveAmmo`, `overview`, `autoplay`, `zoom`, `soak`, a room's `latency`).
 * @param {import("playwright").Page} page
 * @param {string} name
 * @param {...unknown} args
 */
export function shellDebug(page, name, ...args) {
  return page.evaluate(
    ([command, rest]) => window.sloppyShell.debug(command, ...rest),
    [name, args],
  );
}

/** Where `dropSockets` sends new sockets: a port nothing listens on, refused at once. */
export const DEAD_SOCKET = "ws://127.0.0.1:9/";

/**
 * What a context's pages let a check see and stage, as `window.sloppyProbes`, installed
 * before the page runs:
 *
 * - `openSockets()`: the game's WebSockets connecting or open (not the dev server's);
 *   `socketsCreated`: all it made so far; `inputs`: the room `input` messages they sent
 *   (time, seq, drive, fire, mines), the latest few thousand;
 * - `dropSockets(ms)`: close every open socket and send new ones to a closed port
 *   (`DEAD_SOCKET`) for `ms`, as a network drop would;
 * - `setHidden(hidden)`: report the page hidden or visible (`visibilityState`,
 *   `hidden`) and fire `visibilitychange`, as switching tabs does; while hidden it gets
 *   no animation frames, and its timers and delayed tasks wait a second at least;
 * - `wasmBytes()`: each instantiated Wasm memory's size;
 * - `listenerCounts()`: event listeners added and not removed, by target and type (but
 *   not those of one-shot audio sources);
 * - `intervalCount()`: `setInterval` timers not cleared.
 * @param {import("playwright").BrowserContext} context
 */
export function installProbes(context) {
  return context.addInitScript((deadSocket) => {
    const probes = {
      sockets: [],
      socketsCreated: 0,
      blockedUntil: 0,
      hidden: false,
      memories: [],
      listeners: new Map(),
      intervals: new Set(),
    };
    window.sloppyProbes = probes;

    // Only the game's sockets: the dev server's own (Vite's HMR, on the page's host)
    // reloads the page when it drops.
    const NativeSocket = window.WebSocket;
    window.WebSocket = class extends NativeSocket {
      constructor(url, protocols) {
        const game = new URL(url, location.href).host !== location.host;
        const blocked = game && performance.now() < probes.blockedUntil;
        super(blocked ? deadSocket : url, protocols);
        if (!game) return;
        probes.socketsCreated += 1;
        probes.sockets.push(this);
      }

      send(data) {
        if (typeof data === "string" && data.startsWith('{"type":"input"')) {
          const input = JSON.parse(data);
          probes.inputs.push({
            at: performance.now(),
            seq: input.seq,
            moveX: input.moveX,
            moveZ: input.moveZ,
            fire: Boolean(input.fire),
            mines: (input.actions ?? []).filter((action) => action.type === "mine").length,
          });
          if (probes.inputs.length > MAX_INPUTS) probes.inputs.splice(0, MAX_INPUTS / 2);
        }
        return super.send(data);
      }
    };
    const MAX_INPUTS = 4000;
    probes.inputs = [];
    probes.openSockets = () => {
      probes.sockets = probes.sockets.filter((socket) => socket.readyState <= 1);
      return probes.sockets.length;
    };
    probes.dropSockets = (ms) => {
      probes.blockedUntil = performance.now() + ms;
      for (const socket of probes.sockets) socket.close();
    };

    // A hidden tab as Chrome runs one: no animation frames, and timers and delayed tasks
    // wake at most once a second. Headless Chrome keeps every tab visible.
    for (const [name, value] of [
      ["hidden", () => probes.hidden],
      ["visibilityState", () => (probes.hidden ? "hidden" : "visible")],
    ]) {
      Object.defineProperty(document, name, { configurable: true, get: value });
    }
    const HIDDEN_WAKE_MS = 1000;
    const heldFrames = new Map();
    let heldId = 0;
    const { requestAnimationFrame: requestFrame, cancelAnimationFrame: cancelFrame } = window;
    window.requestAnimationFrame = function (callback) {
      if (!probes.hidden) return requestFrame.call(this, callback);
      heldId -= 1;
      heldFrames.set(heldId, callback);
      return heldId;
    };
    window.cancelAnimationFrame = function (id) {
      if (!heldFrames.delete(id)) cancelFrame.call(this, id);
    };
    const { setTimeout: startTimeout } = window;
    window.setTimeout = function (handler, delay, ...rest) {
      const wake = probes.hidden ? Math.max(HIDDEN_WAKE_MS, Number(delay) || 0) : delay;
      return startTimeout.call(this, handler, wake, ...rest);
    };
    const scheduler = globalThis.scheduler;
    if (scheduler?.postTask) {
      const postTask = scheduler.postTask.bind(scheduler);
      scheduler.postTask = (callback, options) =>
        postTask(
          callback,
          probes.hidden && options?.delay != null
            ? { ...options, delay: Math.max(HIDDEN_WAKE_MS, options.delay) }
            : options,
        );
    }
    probes.setHidden = (hidden) => {
      probes.hidden = hidden;
      document.dispatchEvent(new Event("visibilitychange"));
      if (hidden) return;
      const callbacks = [...heldFrames.values()];
      heldFrames.clear();
      for (const callback of callbacks) requestFrame.call(window, callback);
    };

    for (const name of ["instantiate", "instantiateStreaming"]) {
      const original = WebAssembly[name];
      if (typeof original !== "function") continue;
      WebAssembly[name] = async function (...args) {
        const result = await original.apply(this, args);
        const instance = result instanceof WebAssembly.Instance ? result : result.instance;
        const memory = instance?.exports?.memory;
        if (memory instanceof WebAssembly.Memory) probes.memories.push(memory);
        return result;
      };
    }
    probes.wasmBytes = () => probes.memories.map((memory) => memory.buffer.byteLength);

    // Kept per target and listener, so one added twice or removed unadded counts as the
    // browser counts it.
    const added = new WeakMap();
    const targetName = (target) =>
      target === window
        ? "window"
        : target === document
          ? "document"
          : (target?.constructor?.name ?? "unknown");
    const keyOf = (type, options) =>
      `${type}|${typeof options === "boolean" ? options : Boolean(options?.capture)}`;
    // Audio buffer sources play once and are collected with their listeners (the web's
    // audio output schedules one per buffer), so they are left out.
    const counted = (target) =>
      !(globalThis.AudioScheduledSourceNode && target instanceof AudioScheduledSourceNode);
    const { addEventListener, removeEventListener } = EventTarget.prototype;
    EventTarget.prototype.addEventListener = function (type, listener, options) {
      if (listener && counted(this)) {
        let keys = added.get(this);
        if (!keys) added.set(this, (keys = new Map()));
        const key = keyOf(type, options);
        let listeners = keys.get(key);
        if (!listeners) keys.set(key, (listeners = new Set()));
        if (!listeners.has(listener)) {
          listeners.add(listener);
          const name = `${targetName(this)} ${type}`;
          probes.listeners.set(name, (probes.listeners.get(name) ?? 0) + 1);
        }
      }
      return addEventListener.call(this, type, listener, options);
    };
    EventTarget.prototype.removeEventListener = function (type, listener, options) {
      if (added.get(this)?.get(keyOf(type, options))?.delete(listener)) {
        const name = `${targetName(this)} ${type}`;
        probes.listeners.set(name, probes.listeners.get(name) - 1);
      }
      return removeEventListener.call(this, type, listener, options);
    };
    probes.listenerCounts = () =>
      Object.fromEntries([...probes.listeners].filter(([, count]) => count > 0).sort());

    const { setInterval: startInterval, clearInterval: stopInterval } = window;
    window.setInterval = function (...args) {
      const id = startInterval.apply(this, args);
      probes.intervals.add(id);
      return id;
    };
    window.clearInterval = function (id) {
      probes.intervals.delete(id);
      return stopInterval.call(this, id);
    };
    probes.intervalCount = () => probes.intervals.size;
  }, DEAD_SOCKET);
}

/**
 * The game server the multiplayer checks start: `SLOPPY_SERVER_BIN`, else the build of
 * `bun scripts/build-server.mjs`, which stamps the content version the page plays (a
 * plain `cargo build` reports "test-content", and the page refuses its rooms).
 */
export const serverBinary = process.env.SLOPPY_SERVER_BIN ?? "target/server/sloppy-server";

/** Whether `serverBinary` is there to start. */
export function hasServerBinary() {
  return existsSync(serverBinary);
}

/** A local TCP port nothing listens on now, so a check never meets another session's. */
export function freePort() {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const { port } = probe.address();
      probe.close(() => resolve(port));
    });
  });
}

/**
 * Start `serverBinary` on `port` (a free one by default) and wait for its `/health`.
 * `directory` is its HTTP address, which a page lists rooms from (`?directory=`; a lone
 * server is its own directory), and `url` the `ws:` address its rooms carry, as seat keys
 * name it. `stop(signal)` waits for it to exit: SIGKILL crashes it, SIGTERM is a deploy's
 * graceful stop, which tells players their room closed. `start()` brings it back on the
 * same port; `logs()` is everything it printed.
 * @param {{ port?: number }} [options]
 */
export async function startGameServer({ port } = {}) {
  port ??= await freePort();
  const directory = `http://127.0.0.1:${port}`;
  let child;
  let logs = "";
  const healthy = () =>
    fetch(`${directory}/health`).then(
      (answer) => answer.ok,
      () => false,
    );
  const server = {
    port,
    directory,
    url: `ws://127.0.0.1:${port}`,
    logs: () => logs,
    async start() {
      if (await healthy()) throw new Error(`something already answers on port ${port}`);
      child = spawn(serverBinary, [], {
        env: { ...process.env, PORT: String(port) },
        stdio: ["ignore", "pipe", "pipe"],
      });
      child.stdout.on("data", (data) => (logs += data));
      child.stderr.on("data", (data) => (logs += data));
      const deadline = Date.now() + 20000;
      while (!(await healthy())) {
        if (child.exitCode !== null) throw new Error(`the server exited: ${logs}`);
        if (Date.now() > deadline) throw new Error(`the server did not start: ${logs}`);
        await wait(100);
      }
    },
    async stop(signal = "SIGTERM") {
      if (!child || child.exitCode !== null || child.signalCode !== null) return;
      const exited = new Promise((resolve) => child.once("exit", resolve));
      child.kill(signal);
      await exited;
    },
  };
  await server.start();
  return server;
}

/**
 * What `directory`'s server lists for `room` (`/rooms`: players connected, seats
 * reserved, phase), or undefined when it lists none.
 * @param {string} directory
 * @param {string} room
 */
export async function roomListing(directory, room) {
  const { rooms } = await (await fetch(`${directory}/rooms?debug`)).json();
  return rooms.find((listed) => listed.room === room);
}

/**
 * Fingers on a touch page (`hasTouch`) through CDP, as a touch screen sends them: a start
 * or move carries every finger down, an end the one that lifts. Once one has lifted the
 * others hold still and lift last. Each waits for the shell's next frame.
 * @param {import("playwright").Page} page
 */
export async function fingers(page) {
  const session = await page.context().newCDPSession(page);
  const down = new Map();
  // CDP ends its touch sequence at a touchEnd although fingers stay down: they can still
  // lift together (an empty touchEnd) but no longer move or lift one by one.
  let lifted = false;
  const send = async (type, touchPoints) => {
    await session.send("Input.dispatchTouchEvent", { type, touchPoints });
    await nextFrame(page);
  };
  const point = (id, x, y) => ({ id, x, y, radiusX: 4, radiusY: 4, force: 1 });
  return {
    start(id, x, y) {
      down.set(id, point(id, x, y));
      return send("touchStart", [...down.values()]);
    },
    move(id, x, y) {
      if (lifted) throw new Error("CDP moves no finger after another lifted");
      down.set(id, point(id, x, y));
      return send("touchMove", [...down.values()]);
    },
    end(id) {
      const finger = down.get(id);
      down.delete(id);
      if (!lifted) {
        lifted = down.size > 0;
        return send("touchEnd", [finger]);
      }
      if (down.size > 0) throw new Error("CDP lifts the rest only together");
      lifted = false;
      return send("touchEnd", []);
    },
    async tap(id, x, y) {
      await this.start(id, x, y);
      await this.end(id);
    },
  };
}

/**
 * Tap the middle of the widget labelled `label` with a finger (`fingers`) once a frame
 * shows it enabled. Returns the widget.
 * @param {import("playwright").Page} page
 * @param {Awaited<ReturnType<typeof fingers>>} touch
 * @param {string | RegExp} label
 */
export async function tapWidget(page, touch, label, { timeout = 15000 } = {}) {
  const start = Date.now();
  for (;;) {
    const widget = await findWidget(page, label);
    if (widget?.enabled) {
      await touch.tap(90, widget.x + widget.width / 2, widget.y + widget.height / 2);
      return widget;
    }
    if (Date.now() - start > timeout) throw new Error(`no enabled widget ${label} to tap`);
    await nextFrame(page);
  }
}
