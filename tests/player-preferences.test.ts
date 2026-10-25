import assert from "node:assert/strict";
import { test } from "bun:test";
import {
  preferredTank,
  preferredGameMode,
  savedPreference,
  savePreference,
  savedCameraPreferences,
  saveCameraPreferences,
  startingCamera,
} from "../src/game/player-preferences";
import { memoryStorage, withLocalStorage, withStorage } from "./local-storage";

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
  const storage = memoryStorage();
  withStorage(storage, () => {
    assert.deepEqual(savedCameraPreferences(), { firstPerson: false });
    saveCameraPreferences({ camera_preferences: () => new Float64Array([1, 17]) });
    assert.deepEqual(savedCameraPreferences(), { firstPerson: true, zoom: 17 });
    assert.equal(storage.values.get("sloppy-camera"), "first-person");
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
  const denied = {
    get() {
      throw new Error("Storage access denied");
    },
  };
  withLocalStorage(denied, () => {
    assert.equal(savedPreference("game-mode"), null);
    assert.deepEqual(savedCameraPreferences(), { firstPerson: false });
    assert.doesNotThrow(() => savePreference("game-mode", "solo"));
  });
});

/** Run `body` on a device: `phone` sets the phone edition's class, `coarse` the
 * primary pointer, `touchPoints` with a Mac user agent iPadOS Safari. */
function onDevice({ phone = false, coarse = false, touchPoints = 0 }, body: () => void): void {
  const saved = Object.fromEntries(
    ["document", "navigator", "matchMedia"].map((key) => [
      key,
      Object.getOwnPropertyDescriptor(globalThis, key),
    ]),
  );
  const define = (key: string, value: unknown) =>
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  define("document", {
    documentElement: { classList: { contains: (name: string) => phone && name === "phone" } },
  });
  define("navigator", {
    userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
    maxTouchPoints: touchPoints,
  });
  define("matchMedia", (query: string) => ({ matches: coarse && query === "(pointer: coarse)" }));
  try {
    withStorage(memoryStorage(), body);
  } finally {
    for (const [key, descriptor] of Object.entries(saved)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete (globalThis as Record<string, unknown>)[key];
    }
  }
}

test("touch screens start without the overhead reticle; desktops keep it", () => {
  onDevice({}, () => assert.deepEqual(startingCamera(), { firstPerson: false }));
  for (const device of [{ coarse: true }, { touchPoints: 5 }]) {
    onDevice(device, () =>
      assert.deepEqual(startingCamera(), { firstPerson: false, hideReticle: true }),
    );
  }
  onDevice({ phone: true, coarse: true }, () =>
    assert.deepEqual(startingCamera(), { firstPerson: false, zoom: 40, hideReticle: true }),
  );
});
