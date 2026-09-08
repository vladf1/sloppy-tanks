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
  const zooms: number[] = [];
  const controls = new Controls(
    canvas as unknown as HTMLCanvasElement,
    () => pauses++,
    n => zooms.push(n),
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
    zooms,
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
test("wheel queues one ammo change per 120 ms; Shift-wheel only zooms", () => {
  const f = fixture();
  f.emit(f.canvas, "wheel", { deltaY: 100, shiftKey: false });
  assert.equal(f.controls.command(0).ammoSelection, 1);
  f.emit(f.canvas, "wheel", { deltaY: -100, shiftKey: false });
  assert.equal(f.controls.command(0).ammoSelection, undefined);
  f.controls.lastAmmoScroll -= 120;
  f.emit(f.canvas, "wheel", { deltaY: -100, shiftKey: false });
  assert.equal(f.controls.command(0).ammoSelection, -1);
  assert.equal(f.controls.command(0).ammoSelection, undefined);
  f.emit(f.canvas, "wheel", { deltaY: 100, shiftKey: true });
  f.emit(f.canvas, "wheel", { deltaY: -100, shiftKey: true });
  assert.deepEqual(f.zooms, [2, -2]);
  assert.equal(f.controls.command(0).ammoSelection, undefined);
  f.dispose();
});
test("inactive play rejects wheel selection and pause, blur, visibility and clear discard pending input", () => {
  const f = fixture();
  f.controls.active = () => false;
  f.emit(f.canvas, "wheel", { deltaY: 1 });
  assert.equal(f.controls.ammoSelection, undefined);
  for (const action of ["clear", "blur", "Escape", "visibilitychange"]) {
    f.controls.active = () => true;
    f.emit(f.canvas, "wheel", { deltaY: 1 });
    assert.equal(f.controls.ammoSelection, 1);
    if (action === "clear") f.controls.clear();
    else if (action === "Escape") f.emit(f.win, "keydown", { code: "Escape" });
    else if (action === "visibilitychange") {
      Object.assign(f.doc, { hidden: true }); f.emit(f.doc, action, {});
    } else f.emit(f.win, action, {});
    assert.equal(f.controls.command(0).ammoSelection, undefined);
  }
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

test("Q/E and number keys queue exactly one selection without consuming held fire", () => {
  const f = fixture();
  f.controls.fire = true;
  const expected = ["standard", "spread", "rocket", "ricochet", "piercing"];
  for (const [code, selection] of [
    ["KeyQ", -1], ["KeyE", 1],
    ...expected.flatMap((weapon, i) => [[`Digit${i + 1}`, weapon], [`Numpad${i + 1}`, weapon]]),
  ]) {
    f.emit(f.win, "keydown", { code });
    const command = f.controls.command(0);
    assert.equal(command.ammoSelection, selection);
    assert.equal(command.fire, true);
    assert.equal(f.controls.command(0).ammoSelection, undefined);
    f.emit(f.win, "keydown", { code, repeat: true });
    assert.equal(f.controls.command(0).ammoSelection, undefined);
  }
  f.dispose();
});

test("ammo shortcuts ignore inactive play, browser modifiers and editable controls", () => {
  const f = fixture();
  f.controls.active = () => false;
  f.emit(f.win, "keydown", { code: "KeyE" });
  assert.equal(f.controls.command(0).ammoSelection, undefined);
  f.controls.active = () => true;
  for (const modifier of ["metaKey", "ctrlKey", "altKey"]) {
    f.emit(f.win, "keydown", { code: "Digit3", [modifier]: true });
    assert.equal(f.controls.command(0).ammoSelection, undefined);
  }
  for (const tagName of ["INPUT", "TEXTAREA", "SELECT"]) {
    Object.assign(f.win, { tagName });
    f.emit(f.win, "keydown", { code: "KeyE" });
    assert.equal(f.controls.command(0).ammoSelection, undefined);
  }
  Object.assign(f.win, { tagName: "DIV", isContentEditable: true });
  f.emit(f.win, "keydown", { code: "Digit5" });
  assert.equal(f.controls.command(0).ammoSelection, undefined);
  Object.assign(f.win, { isContentEditable: false });
  f.emit(f.win, "keydown", { code: "KeyE" });
  f.emit(f.win, "blur", {});
  assert.equal(f.controls.command(0).ammoSelection, undefined);
  f.dispose();
});
