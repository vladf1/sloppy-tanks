import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "bun:test";
import { buildErrorPage, startDevServer } from "../scripts/site-server.ts";
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

/** The first token the dev server's live-reload socket sends. */
async function reloadToken(url: URL): Promise<string> {
  const socket = new WebSocket(new URL("__reload", url.href.replace(/^http/, "ws")));
  try {
    return await new Promise<string>((resolve, reject) => {
      socket.onmessage = ({ data }) => resolve(String(data));
      socket.onerror = () => reject(new Error("The reload socket failed"));
    });
  } finally {
    socket.close();
  }
}

test("a restarted dev server makes pages served by the old one reload", async () => {
  const first = startDevServer({ port: 0 });
  const port = Number(first.url.port);
  const before = await reloadToken(first.url);
  await first.stop();
  // Both processes start at revision 0; the token must still differ, or open pages keep
  // chunk URLs the new process never built.
  const second = startDevServer({ port });
  try {
    assert.notEqual(await reloadToken(second.url), before);
  } finally {
    await second.stop();
  }
});

test("a failed page build serves its located errors and the reload script", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "sloppy-build-error-"));
  try {
    const source = path.join(directory, "broken.ts");
    await writeFile(source, "export const broken = <;\n");
    const error = await Bun.build({ entrypoints: [source], target: "browser" }).then(
      () => undefined,
      (failure: unknown) => failure,
    );
    assert.ok(error instanceof AggregateError);
    const response = buildErrorPage(error, "<script>reload()</script>");
    assert.equal(response.status, 500);
    const html = await response.text();
    assert.ok(html.includes(`${source}:1:`), "names the file and line");
    assert.ok(html.includes("broken = &#60;;"), "escapes the source line");
    assert.ok(html.endsWith("<script>reload()</script>"));
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
