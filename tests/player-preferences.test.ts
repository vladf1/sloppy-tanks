import assert from "node:assert/strict";
import { test } from "node:test";
import {
  preferredTank,
  preferredGameMode,
  savedPreference,
  savePreference,
  savedCameraPreferences,
  saveCameraPreferences,
} from "../src/game/player-preferences";

function withStorage(storage: object, check: () => void): void {
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", { value: storage, configurable: true });
  try {
    check();
  } finally {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else delete (globalThis as { localStorage?: unknown }).localStorage;
  }
}

test("only playable tanks and known battle formats restore from storage", () => {
  assert.equal(preferredTank("scout"), "scout");
  assert.equal(preferredTank("heavy"), "heavy");
  for (const invalid of [null, "", "humvee", "SCOUT", "bad"]) {
    assert.equal(preferredTank(invalid), "balanced");
    assert.equal(preferredGameMode(invalid), "team");
  }
  assert.equal(preferredGameMode("solo"), "solo");
});

test("camera settings use the engine's preferred view and clamped zoom", () => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  };
  withStorage(storage, () => {
    assert.deepEqual(savedCameraPreferences(), { firstPerson: false });
    saveCameraPreferences({ camera_preferences: () => new Float64Array([1, 17]) });
    assert.deepEqual(savedCameraPreferences(), { firstPerson: true, zoom: 17 });
    assert.equal(values.get("sloppy-camera"), "first-person");
    saveCameraPreferences({ camera_preferences: () => new Float64Array([0, 52]) });
    assert.deepEqual(savedCameraPreferences(), { firstPerson: false, zoom: 52 });
  });
});

test("malformed camera storage falls back without copying renderer bounds into the shell", () => {
  for (const zoom of [null, "", "  ", "NaN", "Infinity", "{}", "34px"]) {
    withStorage({ getItem: (key: string) => (key === "sloppy-zoom" ? zoom : "bad") }, () => {
      assert.deepEqual(savedCameraPreferences(), { firstPerson: false });
    });
  }
  // Finite values, including old out-of-range values, go to the renderer to clamp.
  withStorage({ getItem: (key: string) => (key === "sloppy-zoom" ? "999" : null) }, () => {
    assert.deepEqual(savedCameraPreferences(), { firstPerson: false, zoom: 999 });
  });
});

test("denied storage reads and full-storage writes do not interrupt play", () => {
  const unavailable = () => {
    throw new Error("Storage unavailable");
  };
  withStorage({ getItem: unavailable, setItem: unavailable }, () => {
    assert.equal(savedPreference("tank"), null);
    assert.deepEqual(savedCameraPreferences(), { firstPerson: false });
    assert.doesNotThrow(() => savePreference("tank", "heavy"));
    assert.doesNotThrow(() =>
      saveCameraPreferences({ camera_preferences: () => new Float64Array([1, 34]) }),
    );
  });
});

test("a browser denying access to localStorage itself still opens the setup", () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    get() {
      throw new Error("Storage access denied");
    },
  });
  try {
    assert.equal(savedPreference("game-mode"), null);
    assert.deepEqual(savedCameraPreferences(), { firstPerson: false });
    assert.doesNotThrow(() => savePreference("game-mode", "solo"));
  } finally {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else delete (globalThis as { localStorage?: unknown }).localStorage;
  }
});
