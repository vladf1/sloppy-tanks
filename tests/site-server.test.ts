import assert from "node:assert/strict";
import { test } from "bun:test";
import { startDevServer } from "../scripts/site-server.ts";
import { root } from "../scripts/site-build.ts";

test("a second dev process cannot share an occupied port", async () => {
  const server = startDevServer({ port: 0 });
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
