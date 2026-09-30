import { test } from "node:test";
import assert from "node:assert/strict";
import { preferredPlayerName, rememberPlayerName } from "../src/net/player-name";

function withStorage(storage: Pick<Storage, "getItem" | "setItem">, run: () => void) {
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", { value: storage, configurable: true });
  try {
    run();
  } finally {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else delete (globalThis as { localStorage?: unknown }).localStorage;
  }
}

function memoryStorage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => void values.set(key, value),
  };
}

test("the remembered name is trimmed to 24 characters", () => {
  withStorage(memoryStorage(), () => {
    rememberPlayerName("  Ace  ");
    assert.equal(preferredPlayerName(), "Ace");
    rememberPlayerName("A".repeat(30));
    assert.equal(preferredPlayerName(), "A".repeat(24));
  });
});

test("a blank or unreadable preference falls back to a roster name", () => {
  withStorage(memoryStorage(), () => {
    rememberPlayerName("   ");
    const name = preferredPlayerName();
    assert.match(name, /^[A-Z][A-Z ]+$/);
  });
  const blocked = {
    getItem: () => {
      throw new Error("storage blocked");
    },
    setItem: () => {
      throw new Error("storage blocked");
    },
  };
  withStorage(blocked, () => {
    assert.doesNotThrow(() => rememberPlayerName("Ace"));
    assert.match(preferredPlayerName(), /^[A-Z][A-Z ]+$/);
  });
});
