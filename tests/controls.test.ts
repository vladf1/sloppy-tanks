import { test } from "node:test";
import assert from "node:assert/strict";
import { Controls } from "../src/game/controls";
import { INPUT } from "../src/game/engine-api";
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
  let inputActive = true;
  const zooms: number[] = [];
  const controls = new Controls(
    canvas as unknown as HTMLCanvasElement,
    () => pauses++,
    (n) => zooms.push(n),
    () => inputActive,
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
    /** False while the player is destroyed or a menu is open. */
    set inputActive(value: boolean) {
      inputActive = value;
    },
    dispose() {
      Reflect.deleteProperty(globalThis, "window");
      Reflect.deleteProperty(globalThis, "document");
    },
  };
}
/** One animation frame's packed input, as the engine receives it. */
function frame(controls: Controls) {
  const input = controls.takeInput(new Float32Array(INPUT.length));
  return {
    moveX: input[INPUT.right] - input[INPUT.left] || input[INPUT.touchMoveX],
    fire: input[INPUT.fire],
    mine: input[INPUT.mine],
    ammoSlot: input[INPUT.ammoSlot],
    ammoStep: input[INPUT.ammoStep],
    wheel: input[INPUT.wheelAmmo],
    input,
  };
}
const SLOTS = ["standard", "spread", "rocket", "ricochet", "piercing"];
test("quick right click is queued until exactly one frame takes the mine", () => {
  const f = fixture();
  f.emit(f.canvas, "pointerdown", { button: 2 });
  f.emit(f.win, "pointerup", { button: 2 });
  assert.equal(frame(f.controls).mine, 1);
  assert.equal(frame(f.controls).mine, 0);
  f.dispose();
});
test("the wheel sends one ammo step per frame (the engine throttles it); Shift-wheel only zooms", () => {
  const f = fixture();
  f.emit(f.canvas, "wheel", { deltaY: 100, shiftKey: false });
  assert.equal(frame(f.controls).wheel, 1);
  assert.equal(frame(f.controls).wheel, 0);
  f.emit(f.canvas, "wheel", { deltaY: -100, shiftKey: false });
  assert.equal(frame(f.controls).wheel, -1);
  f.emit(f.canvas, "wheel", { deltaY: 100, shiftKey: true });
  f.emit(f.canvas, "wheel", { deltaY: -100, shiftKey: true });
  f.emit(f.canvas, "wheel", { deltaY: 0, deltaX: 100, shiftKey: true });
  f.emit(f.canvas, "wheel", { deltaY: 0, deltaX: -100, shiftKey: true });
  f.emit(f.canvas, "wheel", { deltaY: 0, deltaX: 0, shiftKey: true });
  f.emit(f.canvas, "wheel", { deltaY: 0, deltaX: 100, shiftKey: false });
  assert.deepEqual(f.zooms, [2, -2, 2, -2]);
  assert.equal(frame(f.controls).wheel, 0);
  f.dispose();
});
test("inactive play rejects wheel ammo selection", () => {
  const f = fixture();
  f.controls.active = () => false;
  f.emit(f.canvas, "wheel", { deltaY: 1 });
  assert.equal(frame(f.controls).wheel, 0);
  f.dispose();
});
test("blur, hidden, Escape and clear discard held and queued keyboard, mouse and touch input", () => {
  for (const [interruption, pauseWhenHidden, pauses] of [
    ["blur", true, 0],
    ["hidden", true, 1],
    ["Escape", true, 1],
    ["clear", true, 0],
    // A multiplayer room keeps running in the background, but Escape still pauses.
    ["blur", false, 0],
    ["hidden", false, 0],
    ["Escape", false, 1],
  ] as const) {
    const label = `${interruption}${pauseWhenHidden ? "" : " (multiplayer)"}`;
    const f = fixture(pauseWhenHidden);
    f.emit(f.win, "keydown", { code: "KeyD" });
    f.emit(f.canvas, "pointerdown", { button: 0 });
    f.emit(f.canvas, "pointerdown", { button: 2 });
    f.emit(f.canvas, "wheel", { deltaY: 1 });
    f.controls.touch.begin("drive", 1);
    f.controls.touch.begin("arena", 2);
    f.controls.touch.moveStick(1, 1, 0);
    assert.equal(f.controls.fire, true);
    assert.equal(f.controls.wheelAmmo, 1);
    if (interruption === "clear") f.controls.clear();
    else if (interruption === "Escape") f.emit(f.win, "keydown", { code: "Escape" });
    else if (interruption === "hidden") {
      Object.assign(f.doc, { hidden: true });
      f.emit(f.doc, "visibilitychange", {});
    } else f.emit(f.win, "blur", {});
    const input = frame(f.controls);
    assert.deepEqual(
      [input.moveX, input.fire, input.mine, input.wheel, f.pauses],
      [0, 0, 0, 0, pauses],
      label,
    );
    f.dispose();
  }
});

test("touch joins the packed input and one-shot actions are sent once", () => {
  const f = fixture();
  f.controls.touch.begin("drive", 1);
  f.controls.touch.moveStick(1, 0.56, 0);
  // A finger on the arena fires while it is down.
  f.controls.touch.begin("arena", 3);
  f.controls.mine = true;
  f.controls.ammoSelection = "rocket";
  const first = frame(f.controls);
  assert.ok(Math.abs(first.input[INPUT.touchMoveX] - 0.5) < 1e-6);
  assert.deepEqual([first.fire, first.mine, first.ammoSlot], [1, 1, 3]);
  const second = frame(f.controls);
  assert.deepEqual([second.fire, second.mine, second.ammoSlot], [1, 0, 0]);
  f.dispose();
});

test("phone first person turns with the stick's sideways push instead of strafing", () => {
  const f = fixture();
  f.controls.touch.begin("drive", 1);
  f.controls.touch.moveStick(1, 0.6, -0.8);
  const overhead = frame(f.controls).input;
  assert.ok(overhead[INPUT.touchMoveX] > 0.4, "overhead the stick still moves sideways");
  assert.equal(overhead[INPUT.aimStickHeld], 0);
  f.controls.stickTurns = true;
  const seated = frame(f.controls).input;
  assert.equal(seated[INPUT.touchMoveX], 0, "no strafing");
  assert.ok(seated[INPUT.touchMoveZ] < -0.5, "forward still drives");
  assert.ok(seated[INPUT.touchAimX] > 0.4, "sideways turns the view");
  assert.equal(seated[INPUT.aimStickHeld], 1);
  f.controls.touch.moveStick(1, 0, -1);
  assert.equal(frame(f.controls).input[INPUT.aimStickHeld], 0, "a straight push does not turn");
  f.dispose();
});

test("with touch controls on, first person locks the pointer for a mouse but not a finger", () => {
  const f = fixture();
  let locks = 0,
    exits = 0;
  Object.assign(f.canvas, { requestPointerLock: () => locks++ });
  Object.assign(f.doc, { pointerLockElement: null, exitPointerLock: () => exits++ });
  const press = (pointerType: string) => {
    f.emit(f.win, "pointerdown", { pointerType });
    f.emit(f.canvas, "pointerdown", { pointerType, button: 0 });
  };
  f.controls.touchLook = () => true;
  f.controls.holdPointer(true);
  press("touch");
  f.controls.capturePointer();
  assert.equal(locks, 0, "a finger turns the view without a lock");
  // A touch laptop's mouse: the click that follows the finger takes the lock at once.
  press("mouse");
  assert.equal(locks, 1, "a mouse click locks for mouse look");
  Object.assign(f.doc, { pointerLockElement: f.canvas });
  press("touch");
  assert.equal(exits, 1, "a finger frees a mouse's lock");
  Object.assign(f.doc, { pointerLockElement: null });
  f.controls.touchLook = () => false;
  press("touch");
  f.controls.capturePointer();
  assert.equal(locks, 2, "with touch controls off a tap still locks, as before");
  f.dispose();
});

test("Q/E and number keys queue exactly one selection without consuming held fire", () => {
  const f = fixture();
  f.controls.fire = true;
  const presses: [code: string, step: number, slot: number][] = [
    ["KeyQ", -1, 0],
    ["KeyE", 1, 0],
    ...SLOTS.flatMap((_weapon, i): [string, number, number][] => [
      [`Digit${i + 1}`, 0, i + 1],
      [`Numpad${i + 1}`, 0, i + 1],
    ]),
  ];
  for (const [code, step, slot] of presses) {
    f.emit(f.win, "keydown", { code });
    const input = frame(f.controls);
    assert.deepEqual([input.ammoStep, input.ammoSlot, input.fire], [step, slot, 1], code);
    const consumed = frame(f.controls);
    assert.deepEqual([consumed.ammoStep, consumed.ammoSlot], [0, 0]);
    f.emit(f.win, "keydown", { code, repeat: true });
    const repeated = frame(f.controls);
    assert.deepEqual([repeated.ammoStep, repeated.ammoSlot], [0, 0]);
  }
  f.dispose();
});

test("ammo shortcuts ignore inactive play, browser modifiers and editable controls", () => {
  const f = fixture();
  const none = () => {
    const input = frame(f.controls);
    return input.ammoStep === 0 && input.ammoSlot === 0;
  };
  f.controls.active = () => false;
  f.emit(f.win, "keydown", { code: "KeyE" });
  assert.ok(none());
  f.controls.active = () => true;
  for (const modifier of ["metaKey", "ctrlKey", "altKey"]) {
    f.emit(f.win, "keydown", { code: "Digit3", [modifier]: true });
    assert.ok(none());
  }
  for (const tagName of ["INPUT", "TEXTAREA", "SELECT"]) {
    Object.assign(f.win, { tagName });
    f.emit(f.win, "keydown", { code: "KeyE" });
    assert.ok(none());
  }
  Object.assign(f.win, { tagName: "DIV", isContentEditable: true });
  f.emit(f.win, "keydown", { code: "Digit5" });
  assert.ok(none());
  Object.assign(f.win, { isContentEditable: false });
  f.emit(f.win, "keydown", { code: "KeyE" });
  f.emit(f.win, "blur", {});
  assert.ok(none());
  f.dispose();
});

test("touch does not use mouse firing or aiming, and unrelated touch release cannot stop mouse fire", () => {
  const f = fixture();
  f.emit(f.canvas, "pointerdown", { button: 0, pointerType: "touch" });
  f.emit(f.canvas, "pointermove", { clientX: 95, clientY: 95, pointerType: "touch" });
  assert.equal(frame(f.controls).fire, 0);
  assert.equal(f.controls.nx, 0);
  f.emit(f.canvas, "pointermove", { clientX: 75, clientY: 25, pointerType: "mouse" });
  const aimed = frame(f.controls).input;
  assert.deepEqual([aimed[INPUT.pointerX], aimed[INPUT.pointerY]], [0.5, 0.5]);
  f.emit(f.canvas, "pointerdown", { button: 0, pointerType: "mouse" });
  f.emit(f.win, "pointerup", { button: 0, pointerType: "touch" });
  assert.equal(frame(f.controls).fire, 1);
  f.dispose();
});

test("V toggles the view once per press, and mouse travel is drained per frame", () => {
  const f = fixture();
  let toggles = 0;
  f.controls.toggleView = () => toggles++;
  f.emit(f.win, "keydown", { code: "KeyV" });
  f.emit(f.win, "keydown", { code: "KeyV", repeat: true });
  assert.equal(toggles, 1);
  f.emit(f.canvas, "pointermove", { clientX: 10, clientY: 10, movementX: 12 });
  f.emit(f.canvas, "pointermove", { clientX: 5, clientY: 10, movementX: -5 });
  f.emit(f.canvas, "pointermove", { pointerType: "touch", movementX: 40 });
  assert.equal(f.controls.takeLook(), 7);
  assert.equal(f.controls.takeLook(), 0);
  f.emit(f.canvas, "pointermove", { clientX: 5, clientY: 10, movementX: 9 });
  f.controls.clear();
  assert.equal(f.controls.takeLook(), 0);
  f.dispose();
});

/** Browser-like pointer lock: requests and releases both announce the change. */
function mockPointerLock(f: ReturnType<typeof fixture>) {
  const doc = f.doc as unknown as { pointerLockElement: unknown };
  const lock = {
    requests: 0,
    exits: 0,
    held: () => doc.pointerLockElement === f.canvas,
    /** The browser's own Esc handling, which may release without delivering the key. */
    releaseByBrowser() {
      doc.pointerLockElement = null;
      f.emit(f.doc, "pointerlockchange", {});
    },
  };
  Object.assign(f.canvas, {
    requestPointerLock() {
      lock.requests++;
      doc.pointerLockElement = f.canvas;
      f.emit(f.doc, "pointerlockchange", {});
      return Promise.resolve();
    },
  });
  Object.assign(f.doc, {
    pointerLockElement: null,
    exitPointerLock() {
      lock.exits++;
      lock.releaseByBrowser();
    },
  });
  return lock;
}

/** Stand-in for `performance.now`, so key timing needs no real waiting. */
function mockClock() {
  let now = 1000;
  const own = Object.getOwnPropertyDescriptor(performance, "now");
  Object.defineProperty(performance, "now", { value: () => now, configurable: true });
  return {
    advance(ms: number) {
      now += ms;
    },
    restore() {
      if (own) {
        Object.defineProperty(performance, "now", own);
      } else {
        Reflect.deleteProperty(performance, "now");
      }
    },
  };
}

test("first-person Esc frees the cursor, and Esc with a free cursor pauses", () => {
  const f = fixture();
  const lock = mockPointerLock(f);
  const clock = mockClock();
  try {
    // Overhead play never captures the pointer, and Esc pauses at once.
    f.emit(f.canvas, "pointerdown", { button: 0 });
    f.emit(f.win, "pointerup", { button: 0 });
    f.emit(f.win, "keydown", { code: "Escape" });
    assert.deepEqual([lock.requests, f.pauses], [0, 1]);
    // First person: a click captures the pointer and fires.
    f.controls.holdPointer(true);
    f.emit(f.canvas, "pointerdown", { button: 0 });
    assert.deepEqual([lock.requests, f.controls.fire], [1, true]);
    // Esc only frees the cursor: the round keeps running and the view holds still.
    f.emit(f.win, "keydown", { code: "Escape" });
    assert.deepEqual([lock.held(), f.pauses, f.controls.fire], [false, 1, false]);
    assert.equal(f.controls.aimWaitsForClick, true);
    f.emit(f.canvas, "pointermove", { clientX: 5, clientY: 5, movementX: 30 });
    assert.equal(f.controls.takeLook(), 0);
    // Esc with the cursor already free opens the menu.
    clock.advance(1000);
    f.emit(f.win, "keydown", { code: "Escape" });
    assert.equal(f.pauses, 2);
    // A click takes the pointer back without firing.
    f.emit(f.canvas, "pointerdown", { button: 0 });
    assert.deepEqual([lock.requests, lock.held(), f.controls.fire], [2, true, false]);
    // A browser that releases first and then delivers the key still only frees the cursor.
    lock.releaseByBrowser();
    clock.advance(100);
    f.emit(f.win, "keydown", { code: "Escape" });
    assert.equal(f.pauses, 2);
    // Leaving first person releases a captured pointer.
    f.emit(f.canvas, "pointerdown", { button: 0 });
    f.controls.holdPointer(false);
    assert.deepEqual([lock.requests, lock.exits, lock.held(), f.pauses], [3, 2, false, 2]);
    // An Esc just after the game's own release is a new press: overhead pauses at once.
    clock.advance(100);
    f.emit(f.win, "keydown", { code: "Escape" });
    assert.equal(f.pauses, 3);
  } finally {
    clock.restore();
    f.dispose();
  }
});

test("a first-person pointer stays captured through a death; menus release it", () => {
  const f = fixture();
  const lock = mockPointerLock(f);
  const clock = mockClock();
  try {
    f.controls.holdPointer(true);
    f.emit(f.canvas, "pointerdown", { button: 0 });
    // Destroyed and respawned: the pointer never leaves, so no click or new lock notice.
    f.inputActive = false;
    f.controls.holdPointer(true);
    f.inputActive = true;
    f.controls.holdPointer(true);
    assert.deepEqual([lock.requests, lock.exits, lock.held(), f.pauses], [1, 0, true, 0]);
    // A menu frees it for its buttons.
    f.controls.holdPointer(true, true);
    assert.deepEqual([lock.held(), f.pauses], [false, 0]);
    // Esc while destroyed frees it for the respawn choices without pausing.
    f.controls.holdPointer(true);
    f.emit(f.canvas, "pointerdown", { button: 0 });
    f.inputActive = false;
    f.controls.holdPointer(true);
    clock.advance(1000);
    f.emit(f.win, "keydown", { code: "Escape" });
    assert.deepEqual([lock.requests, lock.held(), f.pauses], [2, false, 0]);
    // Clicks while destroyed pick a tank without capturing; the first after respawning aims.
    f.emit(f.canvas, "pointerdown", { button: 0 });
    assert.equal(lock.requests, 2);
    f.inputActive = true;
    f.controls.holdPointer(true);
    assert.equal(f.controls.aimWaitsForClick, true);
    f.emit(f.canvas, "pointerdown", { button: 0 });
    assert.deepEqual([lock.requests, f.controls.fire], [3, false]);
  } finally {
    clock.restore();
    f.dispose();
  }
});
