// A bake-only instance of the engine (`texture-bake.ts`): instantiate the binary the
// page sends, bake one band of a generated texture and hand its pixels back.
import init, { bake_texture } from "../generated/engine/engine.js";

interface BakeRequest {
  engine: WebAssembly.Module | string;
  key: string;
  band: number;
  bands: number;
}

onmessage = async ({ data }: MessageEvent<BakeRequest>) => {
  try {
    await init({ module_or_path: data.engine });
    const pixels = bake_texture(data.key, data.band, data.bands);
    if (!pixels) {
      throw new Error(`The engine does not bake ${data.key}`);
    }
    postMessage(pixels, { transfer: [pixels.buffer] });
  } catch (error) {
    postMessage(String(error));
  }
};
