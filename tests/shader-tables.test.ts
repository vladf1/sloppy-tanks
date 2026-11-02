import { test } from "bun:test";
import assert from "node:assert/strict";
import { shaderTables, shaderText } from "../crates/render/src/shader/tables.js";

// The tables' texts as `crates/render/src/shader/tables/generate.rs` writes them and its
// tests read them back: runs of line indices, a lone line or `[first, count]`.
const tables = {
  lines: ["fn main() {", "    return;", "}", "", "@vertex"],
  texts: [[4, [0, 3], 3], [[1, 2]], [3]],
};

test("a text is its runs of lines joined by newlines", () => {
  assert.equal(shaderText(tables, 0), "@vertex\nfn main() {\n    return;\n}\n");
  assert.equal(shaderText(tables, 1), "    return;\n}");
  assert.equal(shaderText(tables, 2), "");
});

test("the renderer finds the page's tables on the global object", () => {
  assert.equal(shaderTables(), null);
  Object.assign(globalThis, { sloppyShaders: tables });
  try {
    assert.equal(shaderTables(), tables);
  } finally {
    delete (globalThis as { sloppyShaders?: unknown }).sloppyShaders;
  }
});
