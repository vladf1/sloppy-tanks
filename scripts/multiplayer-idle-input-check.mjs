import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { headless } from "./browser-helpers.mjs";
import { StateMirror } from "../src/net/replication.ts";

const url = new URL(process.env.SLOPPY_URL ?? "http://127.0.0.1:5175/sloppy-tanks/");
url.searchParams.set("multiplayer", "");
if (process.env.SLOPPY_SERVER) url.searchParams.set("server", process.env.SLOPPY_SERVER);
const output = `artifacts/performance/multiplayer/idle-input-${process.env.SLOPPY_CHECK_LABEL ?? "local"}`;
await mkdir(output, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless });
const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
const mirror = new StateMirror(),
  inputs = [],
  errors = [],
  observations = {};
let lobby,
  control,
  ack = 0,
  pings = 0;
const viewer = () => mirror.render(control.tankId).viewer;
page.on("pageerror", (error) => errors.push(error.message));
page.on("websocket", (socket) => {
  socket.on("framesent", ({ payload }) => {
    const message = JSON.parse(String(payload));
    if (message.type === "input") inputs.push(message);
    if (message.type === "ping") pings++;
  });
  socket.on("framereceived", ({ payload }) => {
    const message = JSON.parse(String(payload));
    if (message.type === "lobby") lobby = message;
    if (message.type === "control") control = message;
    if (message.type === "full") mirror.applyFull(message, lobby);
    if (message.type === "snapshot") {
      ack = message.ack;
      for (const snapshot of message.snapshots) mirror.applySnapshot(snapshot);
    }
    if (message.type === "error") errors.push(message.message);
  });
});
const click = async (selector) => {
  const bounds = await page.locator(selector).boundingBox();
  assert.ok(bounds, selector);
  await page.mouse.click(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
};
try {
  await page.goto(url.href);
  await page.locator("#create-room").waitFor();
  await page.locator("#player-name").fill("Idle input check");
  assert.equal(await page.locator("#create-humans-only").isChecked(), true);
  await click("#create-room");
  await page.waitForFunction(
    () => /^\d+ ms/.test(document.querySelector("#network-status")?.textContent ?? ""),
    null,
    { timeout: 60000 },
  );
  await page.waitForTimeout(2000);
  const start = inputs.length,
    startPings = pings,
    epoch = control.controlEpoch;
  await page.waitForTimeout(6500);
  observations.idle = {
    inputs: inputs.length - start,
    pings: pings - startPings,
    milliseconds: 6500,
    ack,
    lastSeq: inputs.at(-1)?.seq,
  };
  assert.ok(
    observations.idle.inputs >= 5 && observations.idle.inputs <= 8,
    "Unchanged idle input should send about once per second",
  );
  assert.equal(
    control.driver,
    "human",
    "Idle player retains control beyond the server's five-second watchdog",
  );
  assert.equal(control.controlEpoch, epoch, "No hidden takeover/resume cycle");
  assert.ok(ack >= inputs.at(-2).seq, "Idle input is acknowledged");

  const before = { ...viewer().position },
    moveStart = inputs.length;
  await page.keyboard.down("d");
  await page.waitForTimeout(800);
  await page.keyboard.up("d");
  await page.waitForTimeout(150);
  observations.moving = inputs.length - moveStart;
  assert.ok(observations.moving >= 8, "Held movement retains the active input cadence");
  assert.ok(
    Math.hypot(viewer().position.x - before.x, viewer().position.z - before.z) > 0.4,
    "Movement reaches the authoritative tank",
  );
  assert.equal(inputs.at(-1).moveX, 0, "Release sends neutral input promptly");

  const aimStart = inputs.length;
  await page.mouse.move(400, 300);
  await page.waitForTimeout(150);
  assert.ok(inputs.length > aimStart, "Aim change wakes input sending");
  await page.mouse.down();
  await page.waitForTimeout(650);
  await page.mouse.up();
  await page.waitForTimeout(150);
  assert.ok(
    inputs.slice(aimStart).filter((input) => input.fire).length >= 5,
    "Held fire is renewed before its lease expires",
  );
  assert.ok(!inputs.at(-1).fire, "Fire release reaches the server");

  const actionStart = inputs.length;
  await page.mouse.click(400, 300, { button: "right" });
  await page.waitForTimeout(150);
  assert.equal(
    inputs
      .slice(actionStart)
      .flatMap((input) => input.actions ?? [])
      .filter((action) => action.type === "mine").length,
    1,
    "An idle mine click is sent once",
  );
  assert.ok(viewer().mineCooldown > 0, "Server applied the mine");
  await click("#pause");
  await click("#network-resume");
  await page.waitForTimeout(1500);
  assert.equal(control.driver, "human");
  assert.ok(control.controlEpoch > epoch);
  assert.deepEqual(errors, []);
  console.log(JSON.stringify(observations));
  console.log("Idle input, acknowledgements, movement, aim, fire, mine and resume passed.");
} finally {
  await page.screenshot({ path: `${output}/game.png` }).catch(() => {});
  await writeFile(`${output}/result.json`, JSON.stringify({ observations, errors }, null, 2));
  if (
    await page
      .locator("#pause")
      .isVisible()
      .catch(() => false)
  )
    await click("#pause").catch(() => {});
  if (
    await page
      .locator("#leave-room")
      .isVisible()
      .catch(() => false)
  )
    await click("#leave-room").catch(() => {});
  await browser.close();
}
