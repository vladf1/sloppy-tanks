// Binary room-state frames as the JSON the Node checks read. Frames are deltas against
// the client's own copy of the state, so only the engine's Rust decoder reads them: each
// socket gets its own `WireView`, fed that socket's binary messages in order, which
// returns the former JSON `full` and `snapshot` messages (see `state-mirror.mjs`).
// The engine Wasm (`pnpm run wasm` builds `src/generated/engine/`) loads on the first
// view, synchronously, so socket handlers can create and use views as frames arrive.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

let engine;

function loadEngine() {
  if (!engine) {
    // Node's `require` of an ES module is synchronous, unlike `import()`.
    const glue = createRequire(import.meta.url)("../src/generated/engine/engine.js");
    glue.initSync({
      module: readFileSync(new URL("../src/generated/engine/engine_bg.wasm", import.meta.url)),
    });
    engine = glue;
  }
  return engine;
}

/** A decoder for one socket's messages, in arrival order: `decode(payload)` parses a text
 * message's JSON, and turns a binary one (`Uint8Array`, `Buffer`, `ArrayBuffer`) into the
 * former JSON `full` or `snapshot` object. A snapshot for a round the view holds no
 * baseline for comes back with an empty `snapshots` array. */
export function createWireView() {
  const view = new (loadEngine().WireView)();
  return {
    decode(payload) {
      if (typeof payload === "string") return JSON.parse(payload);
      const bytes = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
      return JSON.parse(view.json(bytes));
    },
  };
}
