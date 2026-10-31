// Play the page in the installed Firefox, headless, over WebDriver BiDi (Playwright's
// Firefox does not start on macOS here, so the browser opens through LaunchServices):
// the shell starts, Battle Setup's arena gets ready, GO plays, W drives, Esc pauses,
// and Stats for nerds names the backend. Skips where Firefox is not installed.
import { execSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gameUrl } from "./browser-helpers.mjs";

if (process.platform !== "darwin" || !existsSync("/Applications/Firefox.app")) {
  console.log("firefox-check: skipped, no /Applications/Firefox.app");
  process.exit(0);
}
const pageUrl = gameUrl;
const out = "artifacts/performance/firefox-check";
mkdirSync(out, { recursive: true });
const profile = mkdtempSync(join(tmpdir(), "sloppy-firefox-"));
const PORT = 9341;
spawn("open", [
  "-n",
  "-g",
  "-a",
  "Firefox",
  "--args",
  "--headless",
  "--no-remote",
  "--profile",
  profile,
  "--remote-debugging-port",
  String(PORT),
  "about:blank",
]);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let socket;
for (let attempt = 0; attempt < 60 && !socket; attempt++) {
  await sleep(500);
  // Firefox takes a moment to open its port: a refused connection tries again.
  socket = await new Promise((resolve) => {
    const ws = new WebSocket(`ws://127.0.0.1:${PORT}/session`);
    ws.onopen = () => resolve(ws);
    ws.onerror = () => resolve(undefined);
  });
}
if (!socket) throw new Error("Firefox did not open its BiDi port");
let next = 1;
const waiting = new Map();
const logs = [];
socket.onmessage = ({ data }) => {
  const message = JSON.parse(data);
  if (message.id && waiting.has(message.id)) {
    const { resolve, reject } = waiting.get(message.id);
    waiting.delete(message.id);
    if (message.type === "error") reject(new Error(`${message.error}: ${message.message}`));
    else resolve(message.result);
  } else if (message.method === "log.entryAdded") {
    logs.push(`${message.params.level}: ${message.params.text}`);
  }
};
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = next++;
    waiting.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
const results = [];
const check = (name, ok, detail = "") => {
  results.push({ name, ok: Boolean(ok), detail });
  console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
};
try {
  await send("session.new", { capabilities: {} });
  await send("session.subscribe", { events: ["log.entryAdded"] });
  const { contexts } = await send("browsingContext.getTree", {});
  const context = contexts[0].context;
  await send("browsingContext.setViewport", { context, viewport: { width: 1280, height: 800 } });
  const evaluate = async (expression) => {
    const { result } = await send("script.evaluate", {
      expression: `(async () => JSON.stringify(await (${expression})))()`,
      target: { context },
      awaitPromise: true,
    });
    return result.value === undefined ? undefined : JSON.parse(result.value);
  };
  const until = async (what, expression, timeout = 60000) => {
    const start = Date.now();
    for (;;) {
      const value = await evaluate(expression).catch(() => undefined);
      if (value) return value;
      if (Date.now() - start > timeout) throw new Error(`timed out waiting for ${what}`);
      await sleep(250);
    }
  };
  const shot = async (name) => {
    const { data } = await send("browsingContext.captureScreenshot", { context });
    writeFileSync(join(out, `${name}.png`), Buffer.from(data, "base64"));
  };
  const pointer = (x, y) =>
    send("input.performActions", {
      context,
      actions: [
        {
          type: "pointer",
          id: "mouse",
          parameters: { pointerType: "mouse" },
          actions: [
            { type: "pointerMove", x: Math.round(x), y: Math.round(y) },
            { type: "pointerDown", button: 0 },
            { type: "pointerUp", button: 0 },
          ],
        },
      ],
    });
  const key = (value, holdMs = 0) =>
    send("input.performActions", {
      context,
      actions: [
        {
          type: "key",
          id: "keyboard",
          actions: [
            { type: "keyDown", value },
            ...(holdMs ? [{ type: "pause", duration: holdMs }] : []),
            { type: "keyUp", value },
          ],
        },
      ],
    });
  const url = new URL(pageUrl);
  url.search = "automation";
  await send("browsingContext.navigate", { context, url: url.href, wait: "complete" });
  const shell = await until("the shell", "document.documentElement.dataset.shell");
  check("the shell starts", shell === "ready", shell);
  const gpu = await evaluate("Boolean(navigator.gpu)");
  await until("the arena", "window.sloppyShell.state()?.ready");
  await shot("battle-setup");
  const go = (await evaluate("window.sloppyShell.widgets()")).find((w) => w.label === "GO!");
  await pointer(go.x + go.width / 2, go.y + go.height / 2);
  await until(
    "the round",
    "window.sloppyShell.state()?.inRound && window.sloppyShell.state().phase === 'playing'",
  );
  check("GO plays", true);
  const before = await evaluate("window.sloppyShell.frame(['debug', 'nerd'])");
  await key("w", 800);
  const after = await evaluate("window.sloppyShell.frame(['debug'])");
  const moved = Math.hypot(
    after.extras.debug.human.x - before.extras.debug.human.x,
    after.extras.debug.human.z - before.extras.debug.human.z,
  );
  check("W drives", moved > 1, `${moved.toFixed(1)} m`);
  const rows = before.extras.nerd.flatMap((section) => section.rows ?? []);
  const api = rows.find((row) => row.label === "Graphics API")?.value;
  check("a backend", api, `${api} (navigator.gpu ${gpu})`);
  const frames = [await evaluate("window.sloppyShell.frame()")];
  await sleep(3000);
  frames.push(await evaluate("window.sloppyShell.frame()"));
  const fps = ((frames[1].frame - frames[0].frame) * 1000) / (frames[1].now - frames[0].now);
  check("frames", fps > 20, `${fps.toFixed(1)} fps headless`);
  await shot("round");
  await key("");
  await until("the pause", "window.sloppyShell.state()?.phase === 'paused'");
  await shot("paused");
  check("Esc pauses", true);
  const state = await evaluate("window.sloppyShell.state()");
  check("no renderer error", state.error === null, state.error ?? "");
  const errors = logs.filter((line) => line.startsWith("error"));
  check("no console errors", errors.length === 0, errors.join(" | "));
} catch (error) {
  check("the run", false, error.message);
} finally {
  writeFileSync(join(out, "results.json"), JSON.stringify({ results, logs }, null, 2));
  socket?.close();
  // pkill exits non-zero when Firefox has already gone.
  execSync(`pkill -f ${profile} || true`);
}
const passed = results.every((result) => result.ok);
console.log(`firefox-check: ${passed ? "passed" : "failed"}`);
process.exit(passed ? 0 : 1);
