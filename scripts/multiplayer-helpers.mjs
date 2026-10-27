import assert from "node:assert/strict";
import { click } from "./browser-helpers.mjs";
import { createWireView } from "./wire-view.mjs";

/** Poll `condition` every 50 ms for up to a minute, then assert it. */
export async function until(condition, message) {
  const deadline = Date.now() + 60000;
  while (!condition() && Date.now() < deadline)
    await new Promise((resolve) => setTimeout(resolve, 50));
  assert.ok(condition(), message);
}

/**
 * Records a page's room traffic from its real WebSocket frames, across reloads and
 * reconnects. Binary state frames are read as JSON through one `WireView` per socket.
 * Pass `mirror: new StateMirror()` (from `state-mirror.mjs`)
 * to rebuild the replicated scene and assert snapshot continuity.
 * Server `error`/`room-reset` messages and malformed frames go to `errors`.
 */
export function recordRoomFrames(page, errors, { mirror } = {}) {
  const room = { mirror, updates: 0, snapshots: 0, ack: 0, inputs: [], pings: 0 };
  page.on("websocket", (socket) => {
    const wire = createWireView();
    socket.on("framereceived", ({ payload }) => {
      try {
        const message = wire.decode(payload);
        if (message.type === "lobby") room.lobby = message;
        if (message.type === "control") room.control = message;
        if (message.type === "full") {
          room.mirror?.applyFull(message, room.lobby);
          room.fullEpoch = room.control?.controlEpoch;
          room.updates++;
        }
        if (message.type === "snapshot") {
          room.updates++;
          room.snapshots++;
          room.ack = message.ack;
          for (const snapshot of message.snapshots)
            if (room.mirror) assert.ok(room.mirror.applySnapshot(snapshot), "Contiguous snapshots");
        }
        if (message.type === "error" || message.type === "room-reset") errors.push(message);
      } catch (error) {
        errors.push(error.message);
      }
    });
    socket.on("framesent", ({ payload }) => {
      const message = JSON.parse(String(payload));
      if (message.type === "input") room.inputs.push(message);
      if (message.type === "ping") room.pings++;
    });
  });
  return room;
}

/** Check the shipped CSS and reachable controls, at supported desktop sizes. Works for
 * Battle Setup's multiplayer tab and for the in-room network menu. */
export async function checkMultiplayerMenu(page) {
  const viewport = page.viewportSize();
  try {
    for (const [width, height] of [
      [1200, 800],
      [1440, 900],
    ]) {
      await page.setViewportSize({ width, height });
      const layout = await page.evaluate(() => {
        const menu = document.querySelector(".network-menu, .menu.start");
        const bounds = menu.getBoundingClientRect();
        const shown = (node) => {
          const style = getComputedStyle(node);
          return style.display !== "none" && style.visibility !== "hidden";
        };
        return {
          rosterDisplay: getComputedStyle(document.querySelector("#network-roster, #room-list"))
            .display,
          hiddenControlsVisible: [...menu.querySelectorAll("[hidden]")].some(shown),
          left: bounds.left,
          right: bounds.right,
          clientWidth: menu.clientWidth,
          scrollWidth: menu.scrollWidth,
        };
      });
      assert.equal(layout.rosterDisplay, "grid", "Multiplayer stylesheet must be applied");
      assert.equal(layout.hiddenControlsVisible, false, "Inactive controls must stay hidden");
      assert.ok(layout.left >= 0 && layout.right <= width, `Menu fits ${width}px viewport`);
      assert.ok(layout.scrollWidth <= layout.clientWidth + 1, `No horizontal scroll at ${width}px`);
    }
  } finally {
    if (viewport) await page.setViewportSize(viewport);
  }
}

/** Sample what every rendered frame shows, from the page's first frame until the arena
 * has shown for two frames, so a check can prove a join from Battle Setup never flashes
 * the room menu or a blank page. Install before navigating; each load samples anew. */
export async function recordJoinFrames(page) {
  await page.addInitScript(() => {
    const frames = (window.sloppyJoinFrames = []);
    const shown = (node) =>
      !!node && !node.closest("[hidden]") && getComputedStyle(node).display !== "none";
    const sample = () => {
      const setup = document.querySelector(".menu.start");
      frames.push({
        setup: shown(setup),
        joining: !!setup && "joining" in setup.dataset,
        play: setup?.dataset.play,
        kind: setup?.querySelector("[data-kind].selected")?.dataset.kind,
        roomMenu: shown(document.querySelector(".network-menu")?.closest("#overlay")),
        game: shown(document.querySelector(".multiplayer canvas#game")),
      });
      if (frames.filter((frame) => frame.game).length < 2) {
        requestAnimationFrame(sample);
      }
    };
    requestAnimationFrame(sample);
  });
}

/** This page load's frames, once the arena has shown. */
export async function joinFrames(page) {
  await page.waitForFunction(
    () => window.sloppyJoinFrames.filter((frame) => frame.game).length >= 2,
  );
  return page.evaluate(() => window.sloppyJoinFrames);
}

/** A room joined from Battle Setup loads behind the setup and replaces it only when
 * the arena is ready: no room menu, no blank frame, no setup left over the arena. */
export function assertJoinedBehindSetup(frames, label) {
  assert.ok(
    frames.every((frame) => !frame.roomMenu),
    `${label}: the room menu never shows while joining`,
  );
  assert.ok(
    frames.every((frame) => frame.setup !== frame.game),
    `${label}: every frame shows either Battle Setup or the arena`,
  );
}

/** Battle Setup's multiplayer tab, opened with a real pointer click. */
export async function openMultiplayerTab(page) {
  await click(page, "#tab-multiplayer");
  await page.locator("#multiplayer-panel:not([hidden])").waitFor();
  await waitForRoomBrowser(page);
}

/** The lazily loaded room list fills in a saved or random name when it is ready. */
export async function waitForRoomBrowser(page) {
  await page.waitForFunction(() => document.querySelector("#player-name")?.value);
}
