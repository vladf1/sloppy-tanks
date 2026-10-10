import assert from "node:assert/strict";
import { test } from "node:test";
import { importChain, moduleGraph } from "./import-graph";

const ENTRY = "src/main.ts";
// Single player runs the Rust engine. These DOM shell modules are all it may load: game
// rules, simulation and rendering belong in the engine, not back in TypeScript.
const SHELL = new Set([
  "src/main.ts",
  "src/game.ts",
  "src/engine.ts",
  "src/diagnostics.ts",
  "src/style.css",
  "src/touch-controls.css",
  ...[
    "ammo-options",
    "audio",
    "button-input",
    "cockpit",
    "controls",
    "debug-console",
    "engine-api",
    "game-options",
    "hud-feedback",
    "join-screen",
    "map-options",
    "map-picker",
    "nerd-stats",
    "phone-mode",
    "play-modes",
    "player-preferences",
    "round-recap",
    "settings-dialog",
    "start-menu",
    "startup-error",
    "task-yield",
    "texture-bake",
    "touch-controls",
    "touch-input",
    "touch-mode",
    "ui",
    "ui-markup",
  ].map((name) => `src/game/${name}.ts`),
]);
/** Audio, and the development-only tuning panel, are the libraries single player loads. */
const LIBRARIES = /[\\/]node_modules[\\/](.pnpm[\\/])?(howler|tweakpane|@tweakpane)[@\\/]/;

test("single player reaches only the shell and the Rust engine", async () => {
  // The generated engine glue is checked by building it; multiplayer code has its own
  // boundary in multiplayer-client-imports.test.ts.
  const inputs = await moduleGraph(ENTRY, /generated\/engine(-webgl)?\/|(^|\/)net\/|\?url$/, true);
  const modules = Object.keys(inputs);
  assert.ok(modules.includes("src/game.ts"), "The game entry was walked");
  const leaks = modules
    .filter((id) => !SHELL.has(id) && !LIBRARIES.test(id))
    .map((id) => importChain(inputs, ENTRY, id).join(" -> "));
  assert.deepEqual(leaks, []);
});
