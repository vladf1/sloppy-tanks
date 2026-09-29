import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { build, type Metafile, type Plugin } from "esbuild";

const CLIENT = "src/net/client.ts";
const ROOM_BROWSER = "src/net/room-browser.ts";
// The Rust engine (`NetGame`) runs the connection, replication, interpolation, input and
// drawing. The room page is only this DOM shell around it; anything else it reaches would
// be game logic creeping back into TypeScript or a library download on joining a room.
// scripts/multiplayer-loading-check.mjs checks the real Vite chunks in a browser.
const SHELL = new Set([
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
    "task-yield",
    "touch-controls",
    "touch-input",
    "touch-mode",
    "ui-markup",
  ].map((name) => `src/game/${name}.ts`),
  "src/net/multiplayer.css",
  ...[
    "client",
    "network-stats",
    "network-ui",
    "pending-join",
    "player-name",
    "room-browser",
    "room-list",
    "room-protocol",
    "server-address",
  ].map((name) => `src/net/${name}.ts`),
]);
/** Audio is the one library a room page loads. */
const LIBRARIES = /[\\/]node_modules[\\/](.pnpm[\\/])?howler[@\\/]/;
/** The engine build is generated (`pnpm run wasm`); the test only needs its import. */
const generatedEngine: Plugin = {
  name: "generated-engine",
  setup(context) {
    context.onResolve({ filter: /generated\/engine\/|\?url$/ }, ({ path }) => ({
      path,
      external: true,
    }));
  },
};

/** The runtime import path from `entry`, for a readable failure. */
function importChain(inputs: Metafile["inputs"], entry: string, target: string): string[] {
  const parents = new Map<string, string>([[entry, ""]]);
  const queue = [entry];
  for (const file of queue) {
    for (const { path, kind } of inputs[file]?.imports ?? []) {
      if (kind === "import-statement" && !parents.has(path)) {
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

async function staticGraph(entry: string): Promise<Metafile["inputs"]> {
  const { metafile } = await build({
    absWorkingDir: fileURLToPath(new URL("..", import.meta.url)),
    entryPoints: [entry],
    bundle: true,
    write: false,
    metafile: true,
    format: "esm",
    outdir: "unused",
    loader: { ".css": "empty" },
    plugins: [generatedEngine],
    logLevel: "silent",
  });
  return metafile.inputs;
}

for (const entry of [CLIENT, ROOM_BROWSER]) {
  test(`${entry} reaches only the room page shell and the Rust engine`, async () => {
    const inputs = await staticGraph(entry);
    const modules = Object.keys(inputs);
    assert.ok(modules.includes("src/net/room-protocol.ts"), "The client graph was walked");
    const leaks = modules
      .filter((id) => !SHELL.has(id) && !LIBRARIES.test(id))
      .map((id) => importChain(inputs, entry, id).join(" -> "));
    assert.deepEqual(leaks, []);
  });
}

test("the room page runs on the Rust engine build", async () => {
  const inputs = await staticGraph(CLIENT);
  const engine = inputs[CLIENT].imports.filter((item) => item.external).map((item) => item.path);
  assert.deepEqual(engine.sort(), [
    "../generated/engine/engine.js",
    "../generated/engine/engine_bg.wasm?url",
  ]);
});
