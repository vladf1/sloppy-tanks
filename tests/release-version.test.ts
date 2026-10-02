import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { isNumberedRelease, releaseVersion } from "../scripts/release-version.mjs";

const base = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"))
  .version as string;

test("main's builds append the CI build number as the fourth version part", () => {
  assert.match(base, /^\d+\.\d+\.\d+$/);
  assert.equal(releaseVersion("628"), `${base}.628`);
  assert.equal(isNumberedRelease(releaseVersion("628")), true);
});

test("builds without a number report the hand-bumped version alone", () => {
  assert.equal(releaseVersion(), base);
  assert.equal(releaseVersion(""), base);
  assert.equal(isNumberedRelease(base), false);
  assert.equal(isNumberedRelease(undefined), false);
});

test("a malformed build number stops the build", () => {
  assert.throws(() => releaseVersion("pr-12"), /positive integer/);
  assert.throws(() => releaseVersion("0"), /positive integer/);
});
