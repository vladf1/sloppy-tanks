import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import path from "node:path";
import { test } from "bun:test";
import { devPageHtml, withBuildValues } from "../scripts/dev-pages-plugin.ts";
import { startDevServer } from "../scripts/site-server.ts";
import { root } from "../scripts/site-build.ts";

test("a second dev process cannot share an occupied port", async () => {
  const server = await startDevServer({ port: 0 });
  const child = Bun.spawn({
    cmd: [process.execPath, "scripts/serve.ts", "--port", server.url.port, "--strictPort"],
    cwd: root,
    stdout: "pipe",
    stderr: "pipe",
    timeout: 2000,
  });
  try {
    const [code, errors] = await Promise.all([child.exited, new Response(child.stderr).text()]);
    assert.notEqual(code, 0);
    assert.match(errors, /EADDRINUSE/);
  } finally {
    child.kill();
    await server.stop();
  }
});

test("dev pages read public files from public/, link under the base and load the startup module", async () => {
  const index = await devPageHtml(
    '<link rel="icon" href="/favicon.svg" /><a href="/">Play</a><!-- startup-script -->',
    path.join(root, "index.html"),
    "/sloppy-tanks/",
    "",
  );
  assert.equal(
    index,
    '<link rel="icon" href="./public/favicon.svg" /><a href="/sloppy-tanks/">Play</a>' +
      '<script type="module" src="./src/main.ts"></script>',
  );
  const fixture = await devPageHtml(
    '<link rel="icon" href="../favicon.svg" /><img src="/icons/a.png">',
    path.join(root, "tests/maps.browser.html"),
    "/sloppy-tanks/",
    "",
  );
  assert.equal(
    fixture,
    '<link rel="icon" href="../favicon.svg" /><img src="../public/icons/a.png">',
  );
});

test("dev modules get the build values they read, and an unset one fails the bundle", () => {
  const values = { PUBLIC_BASE_URL: "/sloppy-tanks/", PUBLIC_DEV: "true" };
  assert.equal(
    withBuildValues(
      'fetch(`${process.env.PUBLIC_BASE_URL}a`, process.env.PUBLIC_DEV === "true")',
      values,
    ),
    'fetch(`${"/sloppy-tanks/"}a`, "true" === "true")',
  );
  assert.equal(withBuildValues("export const plain = 1;", values), undefined);
  assert.throws(() => withBuildValues("process.env.PUBLIC_OTHER", values), /PUBLIC_OTHER/);
});

// The game page bundles the engine glue, which only `bun run wasm` generates.
const engineBuilt = existsSync(path.join(root, "src/generated/engine-webgl/engine-webgl.js"));

test.skipIf(!engineBuilt)(
  "the dev server bundles the game and serves its engine binaries and bake workers",
  async () => {
    const server = await startDevServer({ port: 0 });
    try {
      const page = await fetch(server.url);
      assert.equal(page.status, 200);
      const html = await page.text();
      assert.ok(html.includes('id="startup-overlay"'), "Battle Setup is in the page");
      const script = /<script type="module"[^>]*src="(\/_bun\/client\/[^"]+)"/.exec(html)?.[1];
      assert.ok(script, "the startup module is bundled");
      const bundle = await (await fetch(new URL(script, server.url))).text();
      assert.ok(!bundle.includes("process.env.PUBLIC_"), "build values are written in");
      assert.ok(bundle.includes('"/sloppy-tanks/src/generated/engine/engine_bg.wasm"'));
      const binary = await fetch(new URL("src/generated/engine/engine_bg.wasm", server.url));
      assert.equal(binary.headers.get("content-type"), "application/wasm");
      const worker = await fetch(new URL("bake-workers/webgpu.js", server.url));
      assert.equal(worker.status, 200);
      assert.match(await worker.text(), /bake_texture/);
    } finally {
      await server.stop();
    }
  },
);
