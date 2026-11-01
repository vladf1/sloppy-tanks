// The page's side of the shader tables (crates/render/src/shader/tables.rs): the parsed
// file `index.html` keeps as `globalThis.sloppyShaders`, and its texts rebuilt one at a
// time for the renderer, so they never sit in the Wasm memory.

/** The page's shader tables, or null on a page without them. */
export function shaderTables() {
  return globalThis.sloppyShaders ?? null;
}

/** Text `index` of `tables`: its runs of lines (a line's index or `[first, count]`)
 * joined by newlines. */
export function shaderText(tables, index) {
  const { lines } = tables;
  const out = [];
  for (const run of tables.texts[index]) {
    if (typeof run === "number") out.push(lines[run]);
    else for (let line = run[0]; line < run[0] + run[1]; line++) out.push(lines[line]);
  }
  return out.join("\n");
}
