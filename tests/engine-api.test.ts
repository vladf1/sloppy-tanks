import { test } from "node:test";
import assert from "node:assert/strict";
import { framesInFlight } from "../src/game/engine-api";

test("only WebKit browsers bound the frames a WebGPU canvas queues", () => {
  const webkit = [
    // Safari on an iPad (the desktop-class user agent), a Mac and an iPhone.
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 26_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Mobile/15E148 Safari/604.1",
    // Chrome on iOS runs WebKit too.
    "Mozilla/5.0 (iPad; CPU OS 26_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/141.0.0.0 Mobile/15E148 Safari/604.1",
  ];
  const others = [
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0",
    "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 15.0; rv:143.0) Gecko/20100101 Firefox/143.0",
  ];
  for (const userAgent of webkit) {
    assert.equal(framesInFlight(userAgent), 2, userAgent);
  }
  for (const userAgent of others) {
    assert.equal(framesInFlight(userAgent), 0, userAgent);
  }
});
