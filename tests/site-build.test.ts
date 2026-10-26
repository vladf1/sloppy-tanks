import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "bun:test";
import { buildSite, root } from "../scripts/site-build.ts";

test("the development directory builds without gameplay workers or engine binaries", async () => {
  const { files } = await buildSite({ pages: ["test-pages.html"] });
  assert.deepEqual([...files.keys()], ["test-pages.html"]);
  const html = await files.get("test-pages.html")!.text();
  assert.ok(html.includes('href="/sloppy-tanks/tools/tank-surface-check.html"'));
});

test("entries and module graphs are keyed from the checkout in any working directory", async () => {
  const directory = process.cwd();
  process.chdir(tmpdir());
  try {
    const { entries, metadata } = await buildSite({
      pages: [],
      entries: ["src/game/task-yield.ts"],
    });
    assert.match(entries.get("src/game/task-yield.ts") ?? "", /^assets\/task-yield-\w+\.js$/);
    assert.ok("src/game/task-yield.ts" in metadata.inputs);
    for (const output of Object.values(metadata.outputs))
      assert.deepEqual(Object.keys(output.inputs), ["src/game/task-yield.ts"]);
  } finally {
    process.chdir(directory);
  }
});

// The workers bundle the engine glue, which only `bun run wasm` generates.
const engineBuilt = existsSync(path.join(root, "src/generated/engine-webgl/engine-webgl.js"));

test.skipIf(!engineBuilt)(
  "any bundle that reaches the texture bakes gets their workers",
  async () => {
    const { files, entries } = await buildSite({
      pages: [],
      entries: ["src/game/texture-bake.ts"],
    });
    const workers = [...files.keys()].filter((name) =>
      name.startsWith("assets/texture-bake-worker"),
    );
    assert.equal(workers.length, 2);
    const bake = await files.get(entries.get("src/game/texture-bake.ts")!)!.text();
    assert.ok(!bake.includes("process.env"), "no lookup is left for the browser");
    for (const worker of workers) assert.ok(bake.includes(`/sloppy-tanks/${worker}`));
  },
);

test.skipIf(!engineBuilt)(
  "a lazy module's stylesheet starts loading with its code, once per page",
  async () => {
    const { files, metadata } = await buildSite({});
    const touch = [...files.keys()].find((name) => /^assets\/touch-controls-\w+\.js$/.test(name));
    const sheet = [...files.keys()].find((name) => /^assets\/touch-controls-\w+\.css$/.test(name));
    assert.ok(touch && sheet);
    const importers = Object.entries(metadata.outputs).filter(([, output]) =>
      output.imports.some(
        (dependency) => dependency.kind === "dynamic-import" && dependency.path === `./${touch}`,
      ),
    );
    assert.ok(importers.length, "something loads the touch controls lazily");
    for (const [importer] of importers) {
      const code = await files.get(importer)!.text();
      assert.ok(
        code.includes(
          `Promise.all([import("/sloppy-tanks/${touch}"),((globalThis.sloppyStyles??={})["/sloppy-tanks/${sheet}"]`,
        ),
        `${importer} starts the touch styles with the touch module`,
      );
    }
    // The module's own wait shares that load rather than adding a second link.
    const own = await files.get(touch)!.text();
    assert.ok(own.includes(`(globalThis.sloppyStyles??={})["/sloppy-tanks/${sheet}"]??=`));
  },
);
