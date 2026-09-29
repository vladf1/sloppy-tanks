import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { build, type Metafile, type Plugin } from "esbuild";

const CLIENT = "src/net/client.ts";
const ROOM_BROWSER = "src/net/room-browser.ts";
// The Rust engine (`NetGame`) runs the connection, replication, interpolation, input and
// drawing. Anything reaching these makes opening a room download the TypeScript engine,
// its physics or Three.js; scripts/multiplayer-loading-check.mjs checks the real Vite
// chunks in a browser.
const ENGINE_CODE = new RegExp(
  [
    String.raw`^src/game/(simulation|physics-browser|presentation|renderer|render-state)\.ts$`,
    String.raw`/(three|@dimforge)/`,
    String.raw`^src/net/(render-timeline|interpolation|playout-clock|scene-codec|replication|match-host|multiplayer-simulation|player-controls|fixed-step-clock|input-cadence|transport-delay|schema|protocol|connection)\.ts$`,
  ].join("|"),
);
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
  test(`${entry} imports none of the TypeScript engine, physics or Three.js`, async () => {
    const inputs = await staticGraph(entry);
    const modules = Object.keys(inputs);
    assert.ok(modules.includes("src/net/room-protocol.ts"), "The client graph was walked");
    const leaks = modules
      .filter((id) => ENGINE_CODE.test(id))
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
