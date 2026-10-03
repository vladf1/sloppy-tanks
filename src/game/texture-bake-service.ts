// A bake-only instance of the engine (`texture-bake.ts`): instantiate the binary the
// page sends, bake one band of a generated texture and hand its pixels back. Each
// engine build has a worker entry that hands its glue here (`texture-bake-worker.ts`,
// `texture-bake-worker-webgl.ts`).

interface BakeRequest {
  engine: WebAssembly.Module | string;
  key: string;
  band: number;
  bands: number;
}

type Init = (options: { module_or_path: WebAssembly.Module | string }) => Promise<unknown>;
type Bake = (key: string, band: number, bands: number) => Uint8Array | undefined;

export function serveTextureBakes(init: Init, bake: Bake): void {
  onmessage = async ({ data }: MessageEvent<BakeRequest>) => {
    try {
      await init({ module_or_path: data.engine });
      const pixels = bake(data.key, data.band, data.bands);
      if (!pixels) {
        throw new Error(`The engine does not bake ${data.key}`);
      }
      postMessage(pixels, { transfer: [pixels.buffer] });
    } catch (error) {
      postMessage(String(error));
    }
  };
}
