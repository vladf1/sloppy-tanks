import { test } from "node:test";
import assert from "node:assert/strict";
import { Controls } from "../src/game/controls";
function fixture() {
  const win = new EventTarget(),
    doc = new EventTarget(),
    canvas = new EventTarget();
  Object.assign(canvas, {
    focus() {},
    getBoundingClientRect() {
      return { left: 0, top: 0, width: 100, height: 100 };
    },
  });
  Object.defineProperty(globalThis, "window", {
    value: win,
    configurable: true,
  });
  Object.defineProperty(globalThis, "document", {
    value: doc,
    configurable: true,
  });
  let pauses = 0;
  const controls = new Controls(
    canvas as unknown as HTMLCanvasElement,
    () => pauses++,
    () => {},
  );
  const emit = (
    target: EventTarget,
    name: string,
    props: Record<string, unknown>,
  ) => {
    const event = new Event(name, { cancelable: true });
    Object.assign(event, props);
    target.dispatchEvent(event);
  };
  return {
    win,
    doc,
    canvas,
    controls,
    emit,
    get pauses() {
      return pauses;
    },
    dispose() {
      Reflect.deleteProperty(globalThis, "window");
      Reflect.deleteProperty(globalThis, "document");
    },
  };
}
test("quick right click is queued until exactly one command consumes the mine", () => {
  const f = fixture();
  f.emit(f.canvas, "pointerdown", { button: 2 });
  f.emit(f.win, "pointerup", { button: 2 });
  assert.equal(f.controls.command(0).mine, true);
  assert.equal(f.controls.command(0).mine, false);
  f.dispose();
});
test("focus loss clears held movement, fire, and queued mines and requests pause", () => {
  const f = fixture();
  f.emit(f.win, "keydown", { code: "KeyD" });
  f.emit(f.canvas, "pointerdown", { button: 0 });
  f.emit(f.canvas, "pointerdown", { button: 2 });
  assert.equal(f.controls.fire, true);
  f.emit(f.win, "blur", {});
  const command = f.controls.command(1);
  assert.equal(command.moveX, 0);
  assert.equal(command.fire, false);
  assert.equal(command.mine, false);
  assert.equal(f.pauses, 1);
  f.dispose();
});
