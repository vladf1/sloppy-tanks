import { test } from "node:test";
import assert from "node:assert/strict";
import { bindPress } from "../src/game/button-input";

test("non-primary fingers activate once, while keyboard clicks still work", () => {
  const button = new EventTarget();
  let disabled = false;
  Object.assign(button, { matches: () => disabled });
  let presses = 0;
  bindPress(button as unknown as HTMLElement, () => presses++);
  const emit = (name: string, props: Record<string, unknown>) => {
    const event = new Event(name, { cancelable: true });
    Object.assign(event, props);
    button.dispatchEvent(event);
  };
  emit("pointerdown", { button: 0, pointerType: "touch", isPrimary: false });
  assert.equal(presses, 1);
  emit("click", { detail: 1 });
  assert.equal(presses, 1);
  emit("click", { detail: 0 });
  assert.equal(presses, 2);
  emit("pointerdown", { button: 2 });
  assert.equal(presses, 2);
  disabled = true;
  emit("pointerdown", { button: 0, pointerType: "touch" });
  emit("click", { detail: 0 });
  assert.equal(presses, 2);
});
