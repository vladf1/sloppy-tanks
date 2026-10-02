import assert from "node:assert/strict";
import { setImmediate } from "node:timers/promises";
import { test } from "node:test";
import { initialGameOptions } from "../src/game/game-options";
import { StartMenu, type PreparedGame } from "../src/game/start-menu";

class MenuNode extends EventTarget {
  disabled = false;
  textContent = "";
  dataset: Record<string, string> = {};
  attributes = new Map<string, string>();
  removed = false;
  setAttribute(name: string, value: string) {
    this.attributes.set(name, value);
  }
  removeAttribute(name: string) {
    this.attributes.delete(name);
  }
  remove() {
    this.removed = true;
  }
}

function fixture(load: () => Promise<PreparedGame>, started: () => void = () => {}) {
  const button = new MenuNode();
  const status = new MenuNode();
  const tabs = [new MenuNode(), new MenuNode()];
  const overlay = Object.assign(new MenuNode(), {
    querySelector: (selector: string) =>
      selector === "#start" ? button : selector === "#startup-status" ? status : null,
    querySelectorAll: (selector: string) => (selector === '[role="tab"]' ? tabs : []),
  });
  const options = initialGameOptions(1, "", null);
  new StartMenu({ querySelector: () => overlay } as unknown as HTMLElement, options, load, started);
  return { button, status, tabs, overlay, options };
}

test("queued GO holds the mode but keeps late arena choices, then closes the setup", async () => {
  let release!: (game: PreparedGame) => void;
  const loading = new Promise<PreparedGame>((resolve) => (release = resolve));
  const order: string[] = [];
  const f = fixture(
    () => loading,
    () => {
      assert.equal(f.overlay.removed, false, "cleanup runs before discarding the setup");
      order.push("close");
    },
  );
  f.button.dispatchEvent(new Event("click"));
  assert.equal(f.button.disabled, true);
  assert.ok(
    f.tabs.every((tab) => tab.disabled),
    "mode tabs cannot race the queued start",
  );
  f.options.mapMode = "harbor";
  release({
    prepare: async () => {},
    async start(options) {
      assert.equal(options.mapMode, "harbor");
      order.push("start");
    },
  });
  await setImmediate();
  assert.deepEqual(order, ["start", "close"]);
  assert.equal(f.overlay.removed, true);
});

test("a failed queued start restores the mode tabs", async (context) => {
  context.mock.method(console, "error", () => {});
  const f = fixture(async () => {
    throw new Error("Download unavailable");
  });
  f.button.dispatchEvent(new Event("click"));
  await setImmediate();
  assert.equal(f.button.disabled, false);
  assert.ok(f.tabs.every((tab) => !tab.disabled));
  assert.equal(f.overlay.removed, false);
  assert.equal(f.overlay.dataset.state, "error");
});
