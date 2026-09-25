import { build } from "esbuild";
import { fileURLToPath } from "node:url";
import { contentVersion } from "../../scripts/content-version.mjs";
const repo = (path) => fileURLToPath(new URL(`../../${path}`, import.meta.url));
// One self-contained file: the compat Rapier package inlines its WASM, so the host
// needs only Node, not node_modules.
await build({
  entryPoints: [repo("server/node/main.ts")],
  outfile: repo("server/dist-node/server.mjs"),
  bundle: true,
  format: "esm",
  platform: "node",
  target: "node24",
  sourcemap: true,
  // Bundled CommonJS dependencies (ws) still call require for Node built-ins.
  banner: {
    js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);",
  },
  // ws optionally loads native accelerators; the pure-JS fallbacks are enough here.
  external: ["bufferutil", "utf-8-validate"],
  define: { __MULTIPLAYER_CONTENT_VERSION__: JSON.stringify(await contentVersion()) },
});
