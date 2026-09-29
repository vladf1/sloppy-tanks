import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { build, type Metafile } from "esbuild";

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
    "engine-api",
    "game-options",
    "join-screen",
    "map-options",
    "map-picker",
    "nerd-stats",
    "play-modes",
    "round-recap",
    "start-menu",
    "startup-error",
    "task-yield",
    "touch-controls",
    "touch-input",
    "touch-mode",
    "ui",
    "ui-markup",
  ].map((name) => `src/game/${name}.ts`),
]);
/** Audio, and the development-only tuning panel, are the libraries single player loads. */
const LIBRARIES = /[\\/]node_modules[\\/](.pnpm[\\/])?(howler|tweakpane|@tweakpane)[@\\/]/;

/** The import path from the entry, for a readable failure. */
function importChain(inputs: Metafile["inputs"], target: string): string[] {
  const parents = new Map<string, string>([[ENTRY, ""]]);
  const queue = [ENTRY];
  for (const file of queue) {
    for (const { path } of inputs[file]?.imports ?? []) {
      if (!parents.has(path)) {
        parents.set(path, file);
        queue.push(path);
      }
    }
  }
  const chain = [target];
  while (parents.get(chain[0])) {
    chain.unshift(parents.get(chain[0])!);
  }
  return chain;
}

test("single player reaches only the shell and the Rust engine", async () => {
  const { metafile } = await build({
    absWorkingDir: fileURLToPath(new URL("..", import.meta.url)),
    entryPoints: [ENTRY],
    bundle: true,
    splitting: true,
    write: false,
    metafile: true,
    format: "esm",
    outdir: "unused",
    loader: { ".css": "empty" },
    logLevel: "silent",
    plugins: [
      {
        // The generated engine glue is checked by building it; multiplayer code has
        // its own boundary in multiplayer-client-imports.test.ts.
        name: "external",
        setup(build) {
          build.onResolve({ filter: /generated\/engine\/|(^|\/)net\/|\?url$/ }, (args) => ({
            path: args.path,
            external: true,
          }));
        },
      },
    ],
  });
  const modules = Object.keys(metafile.inputs);
  assert.ok(modules.includes("src/game.ts"), "The game entry was walked");
  const leaks = modules
    .filter((id) => !SHELL.has(id) && !LIBRARIES.test(id))
    .map((id) => importChain(metafile.inputs, id).join(" -> "));
  assert.deepEqual(leaks, []);
});
