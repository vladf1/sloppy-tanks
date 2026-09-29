import { AMMO_ORDER } from "../game/ammunition";
import type { Controls } from "../game/controls";

/** Slots of the packed raw-input frame the engine reads each animation frame
 * (`sloppy_render::presentation::input::slot`, shared by `Game.frame` and
 * `NetGame.frame`). Booleans are 0 or 1; one-shot slots count presses since the
 * previous frame. The engine builds commands, aims and throttles from these. */
export const INPUT_SLOT = {
  up: 0,
  down: 1,
  left: 2,
  right: 3,
  touchMoveX: 4,
  touchMoveZ: 5,
  fire: 6,
  mine: 7,
  /** 1–5 for a chosen ammo slot, 0 none. */
  ammoSlot: 8,
  /** Q/E (and throttled wheel steps from `Controls`): -1, +1 or 0. */
  ammoStep: 9,
  pointerX: 10,
  pointerY: 11,
  touchAiming: 12,
  touchAimX: 13,
  touchAimY: 14,
  aimStickHeld: 15,
  lookPixels: 16,
  zoom: 17,
  toggleView: 18,
  wheelAmmo: 19,
} as const;
export const INPUT_LENGTH = 20;

const held = (controls: Controls, ...codes: string[]) =>
  Number(codes.some((code) => controls.keys.has(code)));

/** Write this frame's control state into `out` and consume the one-shot presses.
 * `zoom` is the camera zoom change in metres since the last frame. */
export function packInput(controls: Controls, out: Float32Array, zoom: number): Float32Array {
  const { touch } = controls;
  out.fill(0);
  out[INPUT_SLOT.up] = held(controls, "KeyW", "ArrowUp");
  out[INPUT_SLOT.down] = held(controls, "KeyS", "ArrowDown");
  out[INPUT_SLOT.left] = held(controls, "KeyA", "ArrowLeft");
  out[INPUT_SLOT.right] = held(controls, "KeyD", "ArrowRight");
  out[INPUT_SLOT.touchMoveX] = touch.moveX;
  out[INPUT_SLOT.touchMoveZ] = touch.moveZ;
  out[INPUT_SLOT.fire] = Number(controls.fire || touch.fire);
  out[INPUT_SLOT.mine] = Number(controls.mine);
  const selection = controls.ammoSelection;
  if (typeof selection === "string") {
    out[INPUT_SLOT.ammoSlot] = AMMO_ORDER.indexOf(selection as (typeof AMMO_ORDER)[number]) + 1;
  } else if (typeof selection === "number") {
    out[INPUT_SLOT.ammoStep] = Math.sign(selection);
  }
  controls.mine = false;
  controls.ammoSelection = undefined;
  out[INPUT_SLOT.pointerX] = controls.nx;
  out[INPUT_SLOT.pointerY] = controls.ny;
  out[INPUT_SLOT.touchAiming] = Number(touch.aiming);
  out[INPUT_SLOT.touchAimX] = touch.aimX;
  out[INPUT_SLOT.touchAimY] = touch.aimY;
  out[INPUT_SLOT.aimStickHeld] = Number(touch.pointers.aim !== null);
  out[INPUT_SLOT.lookPixels] = controls.takeLook();
  out[INPUT_SLOT.zoom] = zoom;
  return out;
}
