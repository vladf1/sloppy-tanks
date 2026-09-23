import * as RAPIER from "@dimforge/rapier3d";
import * as bindings from "@dimforge/rapier3d/rapier_wasm3d_bg.js";
import binaryUrl from "@dimforge/rapier3d/rapier_wasm3d_bg.wasm?url";

export * from "@dimforge/rapier3d";

let ready: Promise<void> | undefined;

/** Instantiate the binary that index.html preloads. Game code, the renderer and
 * scenery load meanwhile; nothing may call into physics before this resolves. */
export function init(): Promise<void> {
  ready ??= (async () => {
    const response = await fetch(binaryUrl);
    if (!response.ok) {
      throw new Error(`Physics download failed: HTTP ${response.status}`);
    }
    const imports = { "./rapier_wasm3d_bg.js": bindings };
    const { instance } = response.headers.get("Content-Type")?.startsWith("application/wasm")
      ? await WebAssembly.instantiateStreaming(response, imports)
      : await WebAssembly.instantiate(await response.arrayBuffer(), imports);
    bindings.__wbg_set_wasm(instance.exports);
  })();
  ready.catch(() => {
    ready = undefined;
  });
  return ready;
}

// The matching compat package remains the Node test/CLI entry; game code uses one API.
export default { ...RAPIER, init };
