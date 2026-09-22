import { test } from "node:test";
import assert from "node:assert/strict";
import { Controls } from "../src/game/controls";
function fixture(pauseWhenHidden = true) {
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
    (n) => zooms.push(n),
    () => true,
    pauseWhenHidden,
  );
  const emit = (target: EventTarget, name: string, props: Record<string, unknown>) => {
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
  f.emit(f.canvas, "wheel", { deltaY: 0, deltaX: 100, shiftKey: true });
  f.emit(f.canvas, "wheel", { deltaY: 0, deltaX: -100, shiftKey: true });
  f.emit(f.canvas, "wheel", { deltaY: 0, deltaX: 0, shiftKey: true });
  f.emit(f.canvas, "wheel", { deltaY: 0, deltaX: 100, shiftKey: false });
  assert.deepEqual(f.zooms, [2, -2, 2, -2]);
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
      Object.assign(f.doc, { hidden: true });
      f.emit(f.doc, action, {});
    } else f.emit(f.win, action, {});
    assert.equal(f.controls.command(0).ammoSelection, undefined);
  }
  f.dispose();
});
test("focus loss clears held movement, fire, and queued mines without pausing", () => {
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
  assert.equal(f.pauses, 0);
  f.dispose();
});

test("hidden visibility clears input and requests pause", () => {
  const f = fixture();
  f.emit(f.win, "keydown", { code: "KeyD" });
  f.emit(f.canvas, "pointerdown", { button: 0 });
  f.emit(f.canvas, "pointerdown", { button: 2 });
  Object.assign(f.doc, { hidden: true });
  f.emit(f.doc, "visibilitychange", {});
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
    ["KeyQ", -1],
    ["KeyE", 1],
    ...expected.flatMap((weapon, i) => [
      [`Digit${i + 1}`, weapon],
      [`Numpad${i + 1}`, weapon],
    ]),
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

test("stress controls clear input on focus loss without pausing; Escape still pauses", () => {
  const f = fixture(false);
  for (const action of ["blur", "visibilitychange"]) {
    f.emit(f.canvas, "pointerdown", { button: 0 });
    Object.assign(f.doc, { hidden: true });
    f.emit(action === "blur" ? f.win : f.doc, action, {});
    assert.equal(f.controls.command(0).fire, false);
    assert.equal(f.pauses, 0);
  }
  f.emit(f.win, "keydown", { code: "Escape" });
  assert.equal(f.pauses, 1);
  f.dispose();
});

test("touch does not use mouse firing or aiming, and unrelated touch release cannot stop mouse fire", () => {
  const f = fixture();
  f.emit(f.canvas, "pointerdown", { button: 0, pointerType: "touch" });
  f.emit(f.canvas, "pointermove", { clientX: 95, clientY: 95, pointerType: "touch" });
  assert.equal(f.controls.command(0).fire, false);
  assert.equal(f.controls.nx, 0);
  f.emit(f.canvas, "pointerdown", { button: 0, pointerType: "mouse" });
  f.emit(f.win, "pointerup", { button: 0, pointerType: "touch" });
  assert.equal(f.controls.command(0).fire, true);
  f.dispose();
});

test("touch joins the shared command and all held touch input clears on interruption", () => {
  const f = fixture();
  f.controls.touch.begin("drive", 1);
  f.controls.touch.begin("aim", 2);
  f.controls.touch.move("drive", 1, 0.56, 0);
  f.controls.touch.move("aim", 2, 1, 0);
  f.controls.mine = true;
  f.controls.ammoSelection = "rocket";
  const first = f.controls.command(0.7);
  assert.ok(Math.abs(first.moveX - 0.5) < 1e-8);
  assert.equal(first.fire, true);
  assert.equal(first.mine, true);
  assert.equal(first.ammoSelection, "rocket");
  const second = f.controls.command(0.7);
  assert.equal(second.fire, true);
  assert.equal(second.mine, false);
  assert.equal(second.ammoSelection, undefined);
  f.emit(f.win, "blur", {});
  assert.equal(f.controls.command(0).fire, false);
  assert.equal(f.controls.command(0).moveX, 0);
  f.dispose();
});

test("hidden visibility clears both touch sticks and pending touch actions", () => {
  const f = fixture();
  f.controls.touch.begin("drive", 1);
  f.controls.touch.begin("aim", 2);
  f.controls.touch.move("drive", 1, 1, 0);
  f.controls.touch.move("aim", 2, 1, 0);
  f.controls.mine = true;
  Object.assign(f.doc, { hidden: true });
  f.emit(f.doc, "visibilitychange", {});
  const command = f.controls.command(0);
  assert.equal(command.moveX, 0);
  assert.equal(command.fire, false);
  assert.equal(command.mine, false);
  assert.equal(f.pauses, 1);
  f.dispose();
});
