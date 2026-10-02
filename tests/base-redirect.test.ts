import assert from "node:assert/strict";
import { test } from "node:test";
import { baseRedirectMiddleware } from "../scripts/base-redirect";

function visit(base: string, url: string): string | undefined {
  let location: string | undefined;
  const response = {
    writeHead: (_status: number, headers: { Location: string }) => (location = headers.Location),
    end: () => {},
  };
  baseRedirectMiddleware(base)({ url } as never, response as never, () => {});
  return location;
}

test("the bare base redirects to the page and keeps its query", () => {
  assert.equal(visit("/sloppy-tanks/", "/sloppy-tanks"), "/sloppy-tanks/");
  assert.equal(visit("/sloppy-tanks/", "/sloppy-tanks?debug"), "/sloppy-tanks/?debug");
  assert.equal(
    visit("/sloppy-tanks/", "/sloppy-tanks?debug&map=harbor"),
    "/sloppy-tanks/?debug&map=harbor",
  );
});

test("other paths and a root base pass through", () => {
  assert.equal(visit("/sloppy-tanks/", "/sloppy-tanks/?debug"), undefined);
  assert.equal(visit("/sloppy-tanks/", "/sloppy-tanks-other"), undefined);
  assert.equal(visit("/", "/?debug"), undefined);
});
