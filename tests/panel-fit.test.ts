import assert from "node:assert/strict";
import { test } from "node:test";
import { fitPanels } from "../src/game/panel-fit";

/** A Battle Setup panel whose laid-out height is `height` CSS pixels. */
function panel(height: number) {
  const style = new Map<string, string>();
  return {
    offsetHeight: height,
    style: {
      setProperty: (name: string, value: string) => style.set(name, value),
      removeProperty: (name: string) => style.delete(name),
    },
    zoom: () => style.get("zoom"),
  };
}

function withWindow(innerHeight: number, phone: boolean, run: () => void) {
  const globals = globalThis as Record<string, unknown>;
  const saved = { window: globals.window, document: globals.document };
  globals.window = { innerHeight };
  globals.document = {
    documentElement: { classList: { contains: (name: string) => phone && name === "phone" } },
  };
  try {
    run();
  } finally {
    Object.assign(globals, saved);
  }
}

test("a short laptop screen scales Battle Setup down so GO stays on screen", () => {
  const setup = panel(864);
  withWindow(800, false, () => fitPanels({ querySelectorAll: () => [setup] } as never));
  const zoom = Number(setup.zoom());
  assert.ok(zoom < 1, `zoom ${zoom}`);
  assert.ok(864 * zoom <= 800 - 24, "the scaled panel fits under the overlay's padding");
});

test("a tall screen, a phone and a tiny window keep sane sizes", () => {
  const tall = panel(864);
  withWindow(1080, false, () => fitPanels({ querySelectorAll: () => [tall] } as never));
  assert.equal(tall.zoom(), undefined);
  const phone = panel(1600);
  withWindow(700, true, () => fitPanels({ querySelectorAll: () => [phone] } as never));
  assert.equal(phone.zoom(), undefined, "phones scroll their own layout");
  const tiny = panel(864);
  withWindow(300, false, () => fitPanels({ querySelectorAll: () => [tiny] } as never));
  assert.ok(Number(tiny.zoom()) >= 0.6, "text never shrinks past legibility");
});
