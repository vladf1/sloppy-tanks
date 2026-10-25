import assert from "node:assert/strict";
import { test } from "bun:test";
import { buildSite } from "../scripts/site-build.ts";

test("the development directory builds without gameplay workers or engine binaries", async () => {
  const { files } = await buildSite({ pages: ["test-pages.html"] });
  assert.deepEqual([...files.keys()], ["test-pages.html"]);
  const html = await files.get("test-pages.html")!.text();
  assert.ok(html.includes('href="/sloppy-tanks/tools/tank-surface-check.html"'));
});
