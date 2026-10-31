// The engine's packed input and frame slots and its match phases, which the game preview
// reads by index. `tests/ts-copies.test.ts` pins them to the Rust tables
// (`crates/web/src/ts_copies_tests.rs`).

/** Slots of the packed raw input frame (`sloppy_render::presentation::input::slot`).
 * Booleans are 0 or 1; one-shot slots count presses since the previous frame. */
export const INPUT = {
  up: 0,
  down: 1,
  left: 2,
  right: 3,
  touchMoveX: 4,
  touchMoveZ: 5,
  fire: 6,
  mine: 7,
  ammoSlot: 8,
  ammoStep: 9,
  pointerX: 10,
  pointerY: 11,
  stickTurn: 12,
  lookPixels: 13,
  zoom: 14,
  toggleView: 15,
  wheelAmmo: 16,
  length: 17,
} as const;

/** The slots of `Game.frame`'s result the preview reads (`frame_slot` in
 * `crates/web/src/game.rs`). */
export const FRAME = { phase: 0, humanAlive: 1, events: 4, clearInput: 5 } as const;

export const PHASES = ["ready", "playing", "paused", "results"] as const;
