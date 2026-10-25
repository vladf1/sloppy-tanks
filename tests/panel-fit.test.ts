import assert from "node:assert/strict";
import { test } from "node:test";
import { fitPanels } from "../src/game/panel-fit";

/** The build label's box: where its top-left corner sits in the window. */
interface LabelBox {
  left: number;
  top: number;
}

/** The dialog's right and bottom edges in the window. */
const DIALOG_EDGES = { right: 1000, bottom: 760 };

/** The build label, which fitPanels marks `data-crowded` where it has no room. */
function buildLabel(box: LabelBox) {
  const attributes = new Set<string>();
  return {
    getBoundingClientRect: () => box,
    removeAttribute: (name: string) => attributes.delete(name),
    toggleAttribute: (name: string, force: boolean) =>
      force ? attributes.add(name) : attributes.delete(name),
    crowded: () => attributes.has("data-crowded"),
  };
}

/** A Battle Setup panel whose laid-out height is `height` CSS pixels, optionally with a
 * build label in the window's corner. */
function panel(height: number, label?: ReturnType<typeof buildLabel>) {
  const style = new Map<string, string>();
  return {
    offsetHeight: height,
    style: {
      setProperty: (name: string, value: string) => style.set(name, value),
      removeProperty: (name: string) => style.delete(name),
    },
    getBoundingClientRect: () => DIALOG_EDGES,
    querySelector: (selector: string) => (selector === ".menu-build" ? (label ?? null) : null),
    zoom: () => style.get("zoom"),
    panelZoom: () => style.get("--panel-zoom"),
  };
}

/** `oneColumn` is a window narrow enough for Battle Setup's one-column layout. */
function withWindow(innerHeight: number, phone: boolean, run: () => void, oneColumn = false) {
  const globals = globalThis as Record<string, unknown>;
  const saved = { window: globals.window, document: globals.document };
  globals.window = { innerHeight, matchMedia: () => ({ matches: oneColumn }) };
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
  assert.ok(864 * zoom <= 800 - 50, "the scaled panel fits between the overlay's paddings");
  assert.equal(setup.panelZoom(), setup.zoom(), "the build label counters the same zoom");
});

test("a tall screen, a phone and a tiny window keep sane sizes", () => {
  const tall = panel(864);
  withWindow(1080, false, () => fitPanels({ querySelectorAll: () => [tall] } as never));
  assert.equal(tall.zoom(), undefined);
  assert.equal(tall.panelZoom(), undefined);
  const phone = panel(1600);
  withWindow(700, true, () => fitPanels({ querySelectorAll: () => [phone] } as never));
  assert.equal(phone.zoom(), undefined, "phones scroll their own layout");
  const tiny = panel(864);
  withWindow(300, false, () => fitPanels({ querySelectorAll: () => [tiny] } as never));
  assert.ok(Number(tiny.zoom()) >= 0.6, "text never shrinks past legibility");
});

test("a narrow window's one column scrolls at full size, so switching tabs never rescales it", () => {
  const column = panel(1300);
  withWindow(900, false, () => fitPanels({ querySelectorAll: () => [column] } as never), true);
  assert.equal(column.zoom(), undefined);
  assert.equal(column.panelZoom(), undefined);
});

test("the build label shows beside or below the dialog and hides where it would crowd it", () => {
  const fit = (box: LabelBox) => {
    const label = buildLabel(box);
    const setup = panel(700, label);
    withWindow(900, false, () => fitPanels({ querySelectorAll: () => [setup] } as never));
    return label;
  };
  assert.equal(fit({ left: 1100, top: 880 }).crowded(), false, "clear to the dialog's right");
  assert.equal(fit({ left: 900, top: 800 }).crowded(), false, "clear under the dialog");
  assert.equal(fit({ left: 1007, top: 740 }).crowded(), true, "closer than the gap on the right");
  assert.equal(fit({ left: 950, top: 740 }).crowded(), true, "over the dialog's corner");

  // A window that grows gives the label its room back: the mark is cleared and re-measured.
  const label = buildLabel({ left: 950, top: 740 });
  const setup = panel(700, label);
  const root = { querySelectorAll: () => [setup] } as never;
  withWindow(900, false, () => fitPanels(root));
  assert.equal(label.crowded(), true);
  label.getBoundingClientRect = () => ({ left: 1100, top: 880 });
  withWindow(900, false, () => fitPanels(root));
  assert.equal(label.crowded(), false);
});
