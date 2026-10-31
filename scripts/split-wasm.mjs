// Split the game page's binary in two with Binaryen's wasm-split (`build-wasm.mjs`):
// the menu half, which the page instantiates first and which shows Battle Setup and
// Settings (`crates/desktop/src/menu.rs`), and the engine half (the simulation, bots,
// maps, models and renderer), which `index.html` downloads behind it and instantiates
// over the menu half's exports. Both share one memory and one function table, so the
// app goes on as one program once both are in: there is no boundary to cross per frame.
//
// The menu half keeps the functions a `menu-only` build of the shell links (the linker's
// own reachability from Battle Setup, function pointers and trait objects included),
// matched by name, with wasm-bindgen's glue functions, and whatever those call directly
// in the full build (inlining differs between the builds), stopping at the app's entry
// points in `split.rs` (`split::engine::`). Every other function goes to the engine half.
// A call from the menu half into a function the engine half has not delivered yet runs
// a stub that throws, naming the function by its placeholder number (the build's
// `split/placeholders` maps it); `scripts/split-check.mjs` plays the menu that way.
// Engine code reaching the menu half fails the build (`LEAKS`).
//
// Data stays in the menu half: wasm-split moves functions only, and Rust's read-only
// data is one segment the halves share by address.
import { spawn } from "node:child_process";
import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { binaryen } from "./binaryen.mjs";

/** The WebAssembly features rustc's wasm32 target enables, with `.cargo/config.toml`'s
 * SIMD: wasm-split must read and write the module with exactly these. */
const FEATURES = [
  "--enable-simd",
  "--enable-bulk-memory",
  "--enable-sign-ext",
  "--enable-mutable-globals",
  "--enable-nontrapping-float-to-int",
  "--enable-reference-types",
  "--enable-multivalue",
];

/** The app's entry points, which only the engine half's arrival lets the menu call. */
const BOUNDARY = /::split::engine::/;
/** wasm-bindgen's glue, which the page calls for both halves: always in the menu half. */
const GLUE = /^Closure\(|^__wbindgen|^__externref|^<?(wasm_bindgen|js_sys|wasm_bindgen_futures)\[/;
/** wasm-bindgen's adapter around an export or a closure, named after what it wraps. */
const SHIM = / externref shim(_\d+)?$/;
/** Engine entry points that must never be in the menu half. (Identical functions are
 * folded under one of their names, so a name alone does not say which crate a small
 * generic function serves: `CODE_GROWTH` watches the rest.) */
const LEAKS =
  /::sim::simulation::Simulation>::(new|step)$|::gpu::Renderer>::(on|render)$|::game::Game>::create_on/;
/** How much more code the menu half may hold than the menu-only build, whose functions
 * it keeps under a different build's inlining; engine code reaching it shows as more. */
const CODE_GROWTH = 1.25;
/** The namespace of the stubs the menu half imports in place of the engine's functions. */
const PLACEHOLDERS = "placeholder.deferred";

/** A name as both builds spell it: crate hashes (which features change) and the
 * linker's and Binaryen's duplicate suffixes left out. */
function normal(name) {
  return name
    .replace(/\[[0-9a-f]{12,16}\]/g, "")
    .replace(/\.llvm\.\d+|\[\d+\]/g, "")
    .replace(/(\.\d+|_\d+)$/, "");
}

function run(command, args, { capture = false } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      stdio: ["ignore", capture ? "pipe" : "inherit", "inherit"],
    });
    const chunks = [];
    if (capture) child.stdout.on("data", (chunk) => chunks.push(chunk));
    child.on("error", reject);
    child.on("exit", (status) =>
      status === 0
        ? resolve(Buffer.concat(chunks).toString("utf8"))
        : reject(new Error(`${path.basename(command)} exited with ${status}`)),
    );
  });
}

/** A module's sections, by id: `[start, end)` byte ranges of their contents. */
function sections(bytes) {
  const found = new Map();
  for (let offset = 8; offset < bytes.length;) {
    const id = bytes[offset];
    const [size, start] = leb(bytes, offset + 1);
    found.set(id, [start, start + size]);
    offset = start + size;
  }
  return found;
}

function leb(bytes, offset) {
  let value = 0;
  let shift = 0;
  for (;;) {
    const byte = bytes[offset++];
    value += (byte & 0x7f) * 2 ** shift;
    shift += 7;
    if (byte < 0x80) return [value, offset];
  }
}

/** The size of a module's code section, in bytes. */
function codeSize(bytes) {
  const [start, end] = sections(bytes).get(10) ?? [0, 0];
  return end - start;
}

/** A module's exported functions: export name to function index. */
function exportedFunctions(bytes) {
  const exports = new Map();
  const range = sections(bytes).get(7);
  if (!range) return exports;
  let [count, offset] = leb(bytes, range[0]);
  for (; count > 0; count--) {
    const [length, nameStart] = leb(bytes, offset);
    const name = Buffer.from(bytes.subarray(nameStart, nameStart + length)).toString("utf8");
    const kind = bytes[nameStart + length];
    const [index, next] = leb(bytes, nameStart + length + 1);
    if (kind === 0) exports.set(name, index);
    offset = next;
  }
  return exports;
}

/** Binaryen's name of each function of `wasm`, by index. */
async function functionNames(wasmOpt, wasm) {
  const map = await run(wasmOpt, [...FEATURES, wasm, "--print-function-map"], { capture: true });
  const names = new Map();
  for (const line of map.split("\n")) {
    const colon = line.indexOf(":");
    if (colon > 0) names.set(Number(line.slice(0, colon)), line.slice(colon + 1));
  }
  return names;
}

/** `wasm`'s functions (named, with whether each is imported) and direct calls. */
async function callGraph(wasmOpt, wasm) {
  const dot = await run(wasmOpt, [...FEATURES, wasm, "--print-call-graph"], { capture: true });
  const functions = new Map();
  const calls = new Map();
  const node = /^ {2}"(.*)" \[style="[^"]*", fillcolor="([^"]*)"\];$/;
  const edge = /^ {2}"(.*)" -> "(.*)"; \/\/ call$/;
  for (const line of dot.split("\n")) {
    let match = edge.exec(line);
    if (match) {
      if (!calls.has(match[1])) calls.set(match[1], []);
      calls.get(match[1]).push(match[2]);
      continue;
    }
    match = node.exec(line);
    if (match) functions.set(match[1], { imported: match[2] === "turquoise" });
  }
  if (functions.size === 0) throw new Error(`no call graph from wasm-opt for ${wasm}`);
  return { functions, calls };
}

/** The functions of `full` the menu half keeps, from `exports` too (the functions
 * behind the menu-only build's exports, whose own names a build may give another
 * function's: Rust merges identical functions); fails on engine code among them. */
function keptFunctions(full, menu, exports) {
  const menuNames = new Set([...menu.functions.keys()].map(normal));
  // An adapter goes with what it wraps: a closure's or a type conversion's always, an
  // export's when the menu has the export (`bake_texture` is the engine's).
  const glue = (name) => {
    if (GLUE.test(name)) return true;
    if (!SHIM.test(name)) return false;
    const wrapped = name.replace(SHIM, "");
    return (
      wrapped.includes("->") || wrapped.startsWith("Closure(") || menuNames.has(normal(wrapped))
    );
  };
  const kept = new Set();
  const reachedFrom = new Map();
  const pending = [...full.functions.keys()]
    .filter((name) => !BOUNDARY.test(name) && (menuNames.has(normal(name)) || glue(name)))
    .concat(exports);
  while (pending.length) {
    const name = pending.pop();
    if (kept.has(name)) continue;
    kept.add(name);
    for (const callee of full.calls.get(name) ?? []) {
      if (kept.has(callee) || BOUNDARY.test(callee)) continue;
      if (!reachedFrom.has(callee)) reachedFrom.set(callee, name);
      pending.push(callee);
    }
  }
  const leaks = [...kept].filter((name) => LEAKS.test(name));
  if (leaks.length) {
    const chain = (name) => {
      const links = [name];
      while (reachedFrom.has(links.at(-1)) && links.length < 16)
        links.push(reachedFrom.get(links.at(-1)));
      return links.join("\n    called by ");
    };
    throw new Error(
      `Engine code reaches the page's menu half (move the call behind split::engine):\n  ${chain(leaks[0])}`,
    );
  }
  return [...kept].filter((name) => !full.functions.get(name).imported);
}

/** Give the glue the stubs the menu half imports for the engine's functions: each throws
 * if the menu calls it before the engine half has arrived and replaced it. */
async function addPlaceholders(glue, outName) {
  const source = await readFile(glue, "utf8");
  const getImports = "function __wbg_get_imports() {";
  const table = `"./${outName}_bg.js": import0,`;
  if (source.split(getImports).length !== 2 || source.split(table).length !== 2) {
    throw new Error(`${glue}: wasm-bindgen's import object changed; update split-wasm.mjs`);
  }
  const stubs = `// The engine half's functions until it arrives (scripts/split-wasm.mjs).
const ENGINE_PLACEHOLDERS = new Proxy({}, {
    get: (_, index) => () => {
        throw new Error(\`The menu called the engine before it arrived (placeholder \${String(index)})\`);
    },
});

`;
  await writeFile(
    glue,
    source
      .replace(getImports, `${stubs}${getImports}`)
      .replace(table, `"${PLACEHOLDERS}": ENGINE_PLACEHOLDERS,\n        ${table}`),
  );
}

/**
 * Split wasm-bindgen's `${outName}_bg.wasm` in `dir` (built with names, which this
 * strips) into the menu half in its place and `${outName}_engine.wasm`, with `menuOnly`
 * the raw `menu-only` build. The keep list and the symbol and placeholder maps go to
 * `${dir}/split/` for debugging.
 */
export async function splitShell({ dir, outName, menuOnly }) {
  const { wasmSplit, wasmOpt } = await binaryen();
  const full = path.join(dir, `${outName}_bg.wasm`);
  const debug = path.join(dir, "split");
  await mkdir(debug, { recursive: true });
  const [fullGraph, menuGraph, fullNames, fullBytes, menuOnlyBytes] = await Promise.all([
    callGraph(wasmOpt, full),
    callGraph(wasmOpt, menuOnly),
    functionNames(wasmOpt, full),
    readFile(full),
    readFile(menuOnly),
  ]);
  const menuExports = exportedFunctions(menuOnlyBytes);
  const exports = [...exportedFunctions(fullBytes)]
    .filter(([name]) => menuExports.has(name))
    .map(([, index]) => fullNames.get(index));
  const kept = keptFunctions(fullGraph, menuGraph, exports);
  const keepList = path.join(debug, "keep.txt");
  await writeFile(keepList, `${kept.join("\n")}\n`);
  const menuHalf = path.join(debug, "menu.wasm");
  const engineHalf = path.join(dir, `${outName}_engine.wasm`);
  await run(wasmSplit, [
    ...FEATURES,
    "--split",
    full,
    // `index.html` instantiates the engine half with the menu half's exports as `primary`.
    "--import-namespace=primary",
    `--keep-funcs=@${keepList}`,
    "--placeholdermap",
    "--symbolmap",
    "--strip-debug",
    "-o1",
    menuHalf,
    "-o2",
    engineHalf,
  ]);
  // wasm-split writes the maps beside each half: the symbols of each (`wasm-function[n]`
  // in a stack trace), and the placeholders the menu half imports, which name the
  // engine's function by its index in the whole binary; the name goes beside it.
  await rename(`${engineHalf}.symbols`, path.join(debug, "engine.symbols"));
  await rename(`${menuHalf}.symbols`, path.join(debug, "menu.symbols"));
  const placeholders = (await readFile(`${menuHalf}.placeholders`, "utf8"))
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const colon = line.indexOf(":");
      const target = line.slice(colon + 1);
      if (colon < 0 || !/^\d+$/.test(target)) return line;
      return `${line.slice(0, colon)}:${fullNames.get(Number(target)) ?? target}`;
    });
  await writeFile(path.join(debug, "placeholders"), `${placeholders.join("\n")}\n`);
  await rm(`${menuHalf}.placeholders`);
  const menuBytes = await readFile(menuHalf);
  const strange = WebAssembly.Module.imports(new WebAssembly.Module(menuBytes))
    .map((entry) => entry.module)
    .filter((module) => !module.startsWith("./") && module !== PLACEHOLDERS);
  if (strange.length) throw new Error(`the menu half imports from ${strange[0]}`);
  const growth = codeSize(menuBytes) / codeSize(menuOnlyBytes);
  if (growth > CODE_GROWTH) {
    throw new Error(
      `The page's menu half has ${growth.toFixed(2)}x the menu-only build's code: engine code reaches it (${keepList})`,
    );
  }
  await rename(menuHalf, full);
  await addPlaceholders(path.join(dir, `${outName}.js`), outName);
  return {
    kept: kept.length,
    total: [...fullGraph.functions.values()].filter((f) => !f.imported).length,
  };
}
