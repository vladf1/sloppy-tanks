import { test } from "node:test";
import assert from "node:assert/strict";
import { serverSources } from "../scripts/content-version.mjs";

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
