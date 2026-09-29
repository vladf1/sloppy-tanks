/** What a prepared engine game (`Game`, `NetGame`) offers for baking textures. */
export interface TextureBaker {
  claim_texture_bake(): string | undefined;
  supply_texture(key: string, rgba: Uint8Array): boolean;
  release_texture_bake(key: string): void;
}

/** The engine binary a bake worker instantiates: the page's compiled module, or its
 * URL for the worker to download and compile itself. */
export type EngineSource = WebAssembly.Module | string;

/** Most workers one bake splits into; each bakes a band of rows. */
const MAX_BAKE_WORKERS = 4;

/** Workers for one bake: leave a core to the page and one to the GPU process. */
export function bakeWorkers(cores = navigator.hardwareConcurrency || 2): number {
  return Math.max(1, Math.min(MAX_BAKE_WORKERS, cores - 2));
}

/** Rows `band` of `bands` of a generated texture, from a worker running its own
 * instance of the engine (`bake_texture`). */
function bakeBand(engine: EngineSource, key: string, band: number, bands: number) {
  return new Promise<Uint8Array>((resolve, reject) => {
    const worker = new Worker(new URL("./texture-bake-worker.ts", import.meta.url), {
      type: "module",
    });
    worker.onmessage = ({ data }: MessageEvent<Uint8Array | string>) => {
      worker.terminate();
      if (data instanceof Uint8Array) {
        resolve(data);
      } else {
        reject(new Error(data));
      }
    };
    worker.onerror = (event) => {
      worker.terminate();
      reject(new Error(event.message));
    };
    worker.postMessage({ engine, key, band, bands });
  });
}

/** Bake a generated texture in bands across workers; the bands stack top to bottom. */
export async function bakeTexture(
  engine: EngineSource,
  key: string,
  bands = bakeWorkers(),
): Promise<Uint8Array> {
  const parts = await Promise.all(
    Array.from({ length: bands }, (_, band) => bakeBand(engine, key, band, bands)),
  );
  if (parts.length === 1) {
    return parts[0];
  }
  const pixels = new Uint8Array(parts.reduce((total, part) => total + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    pixels.set(part, offset);
    offset += part.length;
  }
  return pixels;
}

/** Start the bake an arena waits for, if it has one to hand out. The quarry soil
 * takes the engine about 0.7 s on one thread; in workers it overlaps building the
 * arena and its pipelines instead of adding to them. A failed worker hands the bake
 * back, and the engine bakes it between preparation steps. */
export function startTextureBake(game: TextureBaker, engine: EngineSource): void {
  const key = game.claim_texture_bake();
  if (!key) {
    return;
  }
  bakeTexture(engine, key).then(
    (pixels) => {
      try {
        game.supply_texture(key, pixels);
      } catch (error) {
        // The page has let this game go.
        console.warn(`Baked ${key} too late:`, error);
      }
    },
    (error: unknown) => {
      console.warn(`Baking ${key} in workers failed; baking it on the main thread.`, error);
      game.release_texture_bake(key);
    },
  );
}
