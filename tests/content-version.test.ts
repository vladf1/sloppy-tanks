import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  geometryKey,
  parseDepInfo,
  serverSources,
  shaderTablesSources,
} from "../scripts/content-version.mjs";

test("the server build hashes the server crate's code and manifest", async () => {
  const sources = await serverSources();
  assert.ok(sources.includes("crates/server/Cargo.toml"));
  assert.ok(sources.includes("crates/server/src/main.rs"));
  // The dashboard page is compiled in with `include_str!`.
  assert.ok(sources.includes("crates/server/src/dashboard.html"));
});

test("the server build ignores the server guide and integration tests", async () => {
  const sources = await serverSources();
  assert.ok(!sources.includes("crates/server/README.md"));
  assert.deepEqual(
    sources.filter((path) => path.endsWith(".md") || path.startsWith("crates/server/tests/")),
    [],
  );
});

test("the shader tables follow the renderer and the core's materials, not the rest of the core", async () => {
  const sources = await shaderTablesSources();
  assert.ok(sources.includes("crates/render/src/shader/tables.rs"));
  assert.ok(sources.includes("crates/render/src/bin/shader-tables.rs"));
  assert.ok(sources.some((path) => path.endsWith(".wgsl")));
  assert.ok(sources.includes("crates/render/build.rs"));
  assert.ok(sources.includes("crates/core/src/scene.rs"));
  assert.ok(sources.includes("Cargo.lock"));
  assert.deepEqual(
    sources.filter(
      (path) =>
        (path.startsWith("crates/core/") && path !== "crates/core/src/scene.rs") ||
        path.startsWith("crates/render/tests/"),
    ),
    [],
  );
});

test("dep-info names the compiled files, relative to the checkout, and the environment read", () => {
  const text = [
    "/repo/target/deps/sloppy_core-0123.d: crates/core/src/lib.rs /repo/crates/core/src/scene.rs crates/core/src/odd\\ name.rs",
    "",
    "crates/core/src/lib.rs:",
    "",
    "# env-dep:SLOPPY_EXAMPLE=on",
  ].join("\n");
  assert.deepEqual(parseDepInfo(text, "/repo/"), {
    files: ["crates/core/src/lib.rs", "crates/core/src/odd name.rs", "crates/core/src/scene.rs"],
    env: ["SLOPPY_EXAMPLE=on"],
  });
});

test("the geometry key follows the files the geometry-only build compiles, and only them", async () => {
  const base = await mkdtemp(path.join(tmpdir(), "geometry-key-"));
  try {
    for (const name of ["models.rs", "scene.rs", "simulation.rs"]) {
      await writeFile(path.join(base, name), `// ${name}\n`);
    }
    // The build compiled the models and the scene, not the simulation.
    const { files } = parseDepInfo(`out.d: ${base}/models.rs ${base}/scene.rs\n`, `${base}/`);
    const key = () => geometryKey(files, ["toolchain"], pathToFileURL(`${base}/`));
    const before = await key();
    await writeFile(path.join(base, "simulation.rs"), "// a gameplay change\n");
    assert.equal(await key(), before, "an excluded file leaves the key");
    await writeFile(path.join(base, "scene.rs"), "// a model change\n");
    const changed = await key();
    assert.notEqual(changed, before, "an included file changes it");
    assert.notEqual(
      await geometryKey(files, ["another toolchain"], pathToFileURL(`${base}/`)),
      changed,
      "so does what it adds (crates, compiler, profiles)",
    );
  } finally {
    await rm(base, { recursive: true, force: true });
  }
});
