import { test } from "node:test";
import assert from "node:assert/strict";
import { preferredPlayerName, rememberPlayerName } from "../src/net/player-name";
import { memoryStorage, withStorage } from "./local-storage";

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
