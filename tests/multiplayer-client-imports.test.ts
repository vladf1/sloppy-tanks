import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { build, type Metafile } from "esbuild";

const CLIENT = "src/net/client.ts";
// Multiplayer clients render server state. Anything reaching these makes opening a
// room download client simulation or physics code; scripts/multiplayer-loading-check.mjs
// checks the real Vite chunks in a browser.
const SIMULATION_CODE = /^src\/game\/(simulation|physics-browser)\.ts$|\/@dimforge\//;

/** The runtime import path from the client entry, for a readable failure. */
function importChain(inputs: Metafile["inputs"], target: string): string[] {
  const parents = new Map<string, string>([[CLIENT, ""]]);
  const queue = [CLIENT];
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

test("the multiplayer client imports neither the simulation nor physics", async () => {
  const { metafile } = await build({
    absWorkingDir: fileURLToPath(new URL("..", import.meta.url)),
    entryPoints: [CLIENT],
    bundle: true,
    write: false,
    metafile: true,
    format: "esm",
    outdir: "unused",
    loader: { ".css": "empty" },
    logLevel: "silent",
  });
  const modules = Object.keys(metafile.inputs);
  assert.ok(modules.includes("src/game/presentation.ts"), "The client graph was walked");
  const leaks = modules
    .filter((id) => SIMULATION_CODE.test(id))
    .map((id) => importChain(metafile.inputs, id).join(" -> "));
  assert.deepEqual(leaks, []);
});
