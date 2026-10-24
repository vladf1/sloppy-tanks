import assert from "node:assert/strict";
import { test } from "node:test";
import {
  startupErrorHint,
  startupErrorMessage,
  GraphicsUnavailableError,
} from "../src/game/startup-error";

test("the inline menu recognizes an unavailable-GPU error from the separately built engine", () => {
  const engineError = new Error(
    "Neither WebGPU nor WebGL2 is available. Enable hardware acceleration.",
  );
  engineError.name = "GraphicsUnavailableError";
  assert.equal(startupErrorMessage(engineError, "generic failure"), engineError.message);
  const localError = new GraphicsUnavailableError(new Error("adapter denied"));
  assert.equal(startupErrorMessage(localError, "generic failure"), localError.message);
});

test("unrelated startup failures retain the caller's recovery message", () => {
  for (const error of [
    new Error("network error"),
    null,
    "load failed",
    { message: "not an error" },
  ]) {
    assert.equal(startupErrorMessage(error, "Reload to try again."), "Reload to try again.");
  }
});

test("a missing GPU points at the browser's settings, other failures at a retry", () => {
  const engineError = new Error("Neither WebGPU nor WebGL2 is available.");
  engineError.name = "GraphicsUnavailableError";
  assert.match(startupErrorHint(engineError), /hardware acceleration/);
  assert.match(startupErrorHint(new Error("network error")), /try again/);
});
