import { fetchCompressedAsset } from "./shared.mjs";

export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    if (!path.startsWith("/assets/") || !path.endsWith(".wasm")) {
      return env.ASSETS.fetch(request);
    }

    const response = await fetchCompressedAsset(request, env.ASSETS, path);
    if (response === null) {
      return env.ASSETS.fetch(request);
    }

    response.headers.set("Content-Type", "application/wasm");
    response.headers.set("Cache-Control", "public, max-age=31536000, immutable, no-transform");
    response.headers.set("X-Sloppy-Wasm-Delivery", "precompressed");
    return response;
  },
};
