import assert from "node:assert/strict";
import { test } from "bun:test";
import { requestPath } from "../scripts/site-server";

/** Where a request for `url` is redirected: a path, so the browser keeps its scheme and host. */
function visit(base: string, url: string): string | undefined {
  const result = requestPath(new Request(`http://localhost${url}`), base);
  return (result instanceof Response && result.headers.get("Location")) || undefined;
}

test("the bare base redirects to the page and keeps its query", () => {
  assert.equal(visit("/sloppy-tanks/", "/sloppy-tanks"), "/sloppy-tanks/");
  assert.equal(visit("/sloppy-tanks/", "/sloppy-tanks?debug"), "/sloppy-tanks/?debug");
  assert.equal(
    visit("/sloppy-tanks/", "/sloppy-tanks?debug&map=harbor"),
    "/sloppy-tanks/?debug&map=harbor",
  );
});

test("the site root leads to the page under a base", () => {
  assert.equal(visit("/sloppy-tanks/", "/"), "/sloppy-tanks/");
  assert.equal(visit("/sloppy-tanks/", "/?map=quarry"), "/sloppy-tanks/?map=quarry");
});

test("other paths and a root base pass through", () => {
  assert.equal(visit("/sloppy-tanks/", "/sloppy-tanks/?debug"), undefined);
  assert.equal(visit("/sloppy-tanks/", "/sloppy-tanks-other"), undefined);
  assert.equal(visit("/", "/?debug"), undefined);
});

test("the server refuses malformed and escaping paths", () => {
  for (const pathname of ["/%", "/%2e%2e%2fsecret", "/%5csecret", "/.git/config", "/%00secret"]) {
    const result = requestPath(
      new Request(`http://localhost/sloppy-tanks${pathname}`),
      "/sloppy-tanks/",
    );
    assert.ok(result instanceof Response);
    assert.ok(result.status === 400 || result.status === 404);
  }
  assert.equal(
    requestPath(new Request("http://localhost/sloppy-tanks/assets/game.js"), "/sloppy-tanks/"),
    "assets/game.js",
  );
});
