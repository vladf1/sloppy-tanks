// The HUNTER TOW humvee beside a Big Rig, drawn by the engine in a cleared arena
// with an orbit camera: drag to orbit, scroll to zoom, right-drag to pan.
import { draw, fixtureGame, read } from "./engine-fixture";
const canvas = document.querySelector("canvas");
const game = await fixtureGame(canvas, { seed: 123 });
game.debug_set_autoplay(false);
game.debug_clear_arena(new Uint32Array());
// The player's own tank waits out of sight.
const player = read.state(game).human;
game.debug_place_tank(player.id, 60, 60, NaN);
const shown = [
  [game.debug_add_tank(1, "humvee"), -3.4, 0, 0.35, 0.05],
  [game.debug_add_tank(1, "humvee"), 3.4, 0, -0.35, -0.05],
  [game.debug_add_tank(0, "heavy"), 0, -6, -0.35, Math.PI],
];
for (const [id, x, z, heading, aim] of shown) {
  game.debug_place_tank(id, x, z, heading);
  game.debug_set_tank(id, JSON.stringify({ aim, protection: 0, frozen: true }));
}
game.start();
game.pause();

// Orbit camera about a target, in spherical coordinates.
const TARGET = [0, 0.7, 0.5];
const START = { distance: Math.hypot(8, 8.3, 12.5), yaw: Math.atan2(8, 12.5), pitch: 0.55 };
const LIMITS = { min: 4, max: 45, pitch: Math.PI / 2 - 0.03 };
let orbit = { ...START, target: [...TARGET] };
document.querySelector("#reset-view").addEventListener("click", () => {
  orbit = { ...START, target: [...TARGET] };
});
let drag;
canvas.addEventListener("contextmenu", (event) => event.preventDefault());
canvas.addEventListener("pointerdown", (event) => {
  drag = { x: event.clientX, y: event.clientY, pan: event.button === 2 };
  canvas.setPointerCapture(event.pointerId);
});
canvas.addEventListener("pointerup", () => (drag = undefined));
canvas.addEventListener("pointermove", (event) => {
  if (!drag) return;
  const dx = event.clientX - drag.x;
  const dy = event.clientY - drag.y;
  drag.x = event.clientX;
  drag.y = event.clientY;
  if (drag.pan) {
    const scale = orbit.distance / innerHeight;
    orbit.target[0] -= Math.cos(orbit.yaw) * dx * scale;
    orbit.target[2] += Math.sin(orbit.yaw) * dx * scale;
    orbit.target[1] += dy * scale;
  } else {
    orbit.yaw -= dx * 0.005;
    orbit.pitch = Math.min(LIMITS.pitch, Math.max(0.05, orbit.pitch + dy * 0.005));
  }
});
canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    orbit.distance = Math.min(
      LIMITS.max,
      Math.max(LIMITS.min, orbit.distance * Math.exp(event.deltaY * 0.001)),
    );
  },
  { passive: false },
);
function frame() {
  const { distance, yaw, pitch, target } = orbit;
  const position = [
    target[0] + distance * Math.cos(pitch) * Math.sin(yaw),
    target[1] + distance * Math.sin(pitch),
    target[2] + distance * Math.cos(pitch) * Math.cos(yaw),
  ];
  draw(game, 1 / 60, false, { position, target });
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
