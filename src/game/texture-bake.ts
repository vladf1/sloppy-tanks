/** What a prepared engine game (`Game`, `NetGame`) offers for baking textures. */
export interface TextureBaker {
  claim_texture_bake(): string | undefined;
  supply_texture(key: string, rgba: Uint8Array): boolean;
  release_texture_bake(key: string): void;
}

/** The page's compiled engine module for a bake worker, and whether it is the WebGL
 * build, whose glue the worker must instantiate it with. */
export interface EngineSource {
  module: WebAssembly.Module;
  webgl: boolean;
}

/** Most workers one bake splits into; each bakes a band of rows. */
const MAX_BAKE_WORKERS = 4;

/** Workers for one bake: leave a core to the page and one to the GPU process. */
function bakeWorkers(): number {
  const cores = navigator.hardwareConcurrency || 2;
  return Math.max(1, Math.min(MAX_BAKE_WORKERS, cores - 2));
}

/** A bake under way: its pixels, and whether its workers have loaded their scripts. */
interface Bake {
  pixels: Promise<Uint8Array>;
  /** Settles once every worker of the bake has loaded (or failed). */
  loaded: Promise<void>;
}

/** Rows `band` of `bands` of a generated texture, from a worker running its own
 * instance of the engine (`bake_texture`). */
function bakeBand(engine: EngineSource, key: string, band: number, bands: number): Bake {
  let workerLoaded = () => {};
  const loaded = new Promise<void>((resolve) => {
    workerLoaded = resolve;
  });
  const pixels = new Promise<Uint8Array>((resolve, reject) => {
    // Each build's glue has its own worker, so a WebGPU page never loads WebGL code.
    const worker = engine.webgl
      ? new Worker(new URL("./texture-bake-worker-webgl.ts", import.meta.url), {
          type: "module",
        })
      : new Worker(new URL("./texture-bake-worker.ts", import.meta.url), { type: "module" });
    // The worker posts `null` once its script has loaded (`texture-bake-service.ts`).
    worker.onmessage = ({ data }: MessageEvent<Uint8Array | string | null>) => {
      workerLoaded();
      if (data === null) {
        return;
      }
      worker.terminate();
      if (data instanceof Uint8Array) {
        resolve(data);
      } else {
        reject(new Error(data));
      }
    };
    worker.onerror = (event) => {
      workerLoaded();
      worker.terminate();
      reject(new Error(event.message));
    };
    worker.postMessage({ engine: engine.module, key, band, bands });
  });
  return { pixels, loaded };
}

/** Bake a generated texture in bands across workers; the bands stack top to bottom. */
function bakeTexture(engine: EngineSource, key: string): Bake {
  const bands = bakeWorkers();
  const parts = Array.from({ length: bands }, (_, band) => bakeBand(engine, key, band, bands));
  const pixels = Promise.all(parts.map((part) => part.pixels)).then((rows) => {
    if (rows.length === 1) {
      return rows[0];
    }
    const joined = new Uint8Array(rows.reduce((total, row) => total + row.length, 0));
    let offset = 0;
    for (const row of rows) {
      joined.set(row, offset);
      offset += row.length;
    }
    return joined;
  });
  const loaded = Promise.all(parts.map((part) => part.loaded)).then(() => {});
  return { pixels, loaded };
}

/** Bakes started before their arena was built, by key, until an arena claims them. */
const bakesAhead = new Map<string, Bake>();

/** How long `bakeAhead` waits at most for its workers to load, should one never say. */
const BAKE_LOAD_WAIT_MS = 250;

/** Start baking `keys` (`mapTextureBakes`) before the arena that needs them is built;
 * it takes them over when it claims them (`startTextureBake`). Started at the claim,
 * the quarry soil finished long after the arena's pipelines, and its start waited
 * for it. A bake that fails here fails that claim, which bakes on the main thread.
 * Settles once the bakes' workers have loaded their scripts (at most
 * `BAKE_LOAD_WAIT_MS`): WebKit loads them through the page's main thread, so workers
 * started just before the arena build, which holds it for a few hundred
 * milliseconds, only began once it had finished, and the soil came late again. */
export function bakeAhead(engine: EngineSource, keys: readonly string[]): Promise<void> {
  const loading: Promise<void>[] = [];
  for (const key of keys) {
    let bake = bakesAhead.get(key);
    if (!bake) {
      bake = bakeTexture(engine, key);
      bake.pixels.catch(() => {});
      bakesAhead.set(key, bake);
    }
    loading.push(bake.loaded);
  }
  if (loading.length === 0) {
    return Promise.resolve();
  }
  return Promise.race([
    Promise.all(loading).then(() => {}),
    new Promise<void>((resolve) => setTimeout(resolve, BAKE_LOAD_WAIT_MS)),
  ]);
}

/** Start the bake an arena waits for, if it has one to hand out, or take over the one
 * started ahead for it. The quarry soil takes the engine about 1.4 s on one thread; in
 * workers it overlaps building the arena and its pipelines instead of adding to them.
 * A failed worker hands the bake back, and the engine bakes it between preparation
 * steps. */
export function startTextureBake(game: TextureBaker, engine: EngineSource): void {
  const key = game.claim_texture_bake();
  if (!key) {
    return;
  }
  const ahead = bakesAhead.get(key);
  bakesAhead.delete(key);
  (ahead ?? bakeTexture(engine, key)).pixels.then(
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
