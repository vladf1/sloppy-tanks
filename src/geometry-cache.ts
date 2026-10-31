// The page's side of the geometry cache (`crates/render/src/presentation/geometry_cache.rs`):
// a returning visitor's shell reads the vehicle models and its theme's scenery from
// IndexedDB (`geometry-store.ts`) instead of building them. The boot script starts it
// before the shell downloads, so the files are read by the time the shell asks.
//
// - Only files this origin stored are read: the vehicles and the last theme's scenery
//   at once, another theme's when the shell asks (`request`). A first visit opens no
//   database until the shell writes.
// - Reads run in a worker that hands the bytes over (`geometry-worker.ts`): a blob the
//   page reads itself arrives only between the shell's long startup tasks. WebKit,
//   though, runs a worker's IndexedDB through the page's main thread, so a reader that
//   was slow on the last visit gives way to the page's own (`chooseReader`).
// - The shell writes once its arena is ready; the store keeps one version's files.
// - `?geometry-cache=off` keeps no cache (for A/B measurements).

import { readFile, writeFile } from "./geometry-store.ts";
import type { WorkerReply, WorkerRequest } from "./geometry-worker.ts";

/** A file the shell asked for: its bytes, its read under way, or null (none, or taken). */
type FileState = ArrayBuffer | Promise<void> | null;

type Reader = "worker" | "page";

/** What this origin remembers between visits (localStorage). */
interface Memory {
  /** Files were stored here. */
  stored?: boolean;
  /** The scenery file the shell asked for last. */
  theme?: string;
  /** How long each reader's last first read took, request to bytes on the page (ms). */
  reads?: Partial<Record<Reader, number>>;
}

export interface GeometryCache {
  /** Measurements for the checks, by file. */
  stats: Record<string, Record<string, number | string>>;
  /** Start reading `name` unless it is read or reading; its state. */
  request(name: string): FileState;
  /** The bytes of `name` if they are here, handed over once. */
  take(name: string): ArrayBuffer | null;
  /** Settles once `name` is read (or known absent), or after `ms`. */
  when(name: string, ms: number): Promise<void>;
  /** Keep `buffer` to store as the file `name` of `version`. */
  write(name: string, version: string, buffer: ArrayBuffer): void;
  /** Store what was written: the arena is ready. */
  flush(): void;
  /** Writes not yet stored (checks wait for none). */
  pending(): number;
}

declare global {
  var sloppyGeometry: GeometryCache | undefined;
}

const MEMORY = "sloppy-geometry";
/** A first read slower than this, request to bytes, tries the other reader next visit. */
const SLOW_READ_MS = 100;

function recall(): Memory {
  try {
    return JSON.parse(localStorage.getItem(MEMORY) ?? "{}") as Memory;
  } catch {
    return {};
  }
}

function remember(memory: Memory): void {
  try {
    localStorage.setItem(MEMORY, JSON.stringify(memory));
  } catch {
    // Without storage the next visit reads from the worker again.
  }
}

/** The faster reader of the last visits: the worker, unless it was slow and the page
 * was not (or is not known yet). */
function chooseReader(reads: Memory["reads"] = {}): Reader {
  const { worker, page } = reads;
  if (worker === undefined || worker <= SLOW_READ_MS) return "worker";
  return page === undefined || page < worker ? "page" : "worker";
}

export function startGeometryCache(): void {
  if (new URLSearchParams(location.search).get("geometry-cache") === "off") return;
  if (typeof indexedDB !== "object" || typeof Worker !== "function") return;
  const memory = recall();
  const reader = chooseReader(memory.reads);
  const files = new Map<string, FileState>();
  const stats: GeometryCache["stats"] = {};
  const stat = (name: string) => (stats[name] ??= {});

  let worker: Worker | undefined;
  const waiting = new Map<string, (reply: WorkerReply) => void>();
  const ask = (request: WorkerRequest, transfer: Transferable[] = []) => {
    worker ??= new Worker(new URL("./geometry-worker.ts", import.meta.url), { type: "module" });
    worker.onmessage = ({ data }: MessageEvent<WorkerReply>) => {
      waiting.get(`${data.op}:${data.name}`)?.(data);
    };
    const reply = new Promise<WorkerReply>((resolve) =>
      waiting.set(`${request.op}:${request.name}`, resolve),
    );
    worker.postMessage(request, transfer);
    return reply;
  };

  let firstRead = true;
  const request = (name: string): FileState => {
    if (files.has(name)) return files.get(name) ?? null;
    if (name.startsWith("scenery-") && memory.theme !== name) {
      memory.theme = name;
      remember(memory);
    }
    if (!memory.stored) {
      files.set(name, null);
      return null;
    }
    const started = performance.now();
    const timed = firstRead;
    firstRead = false;
    const bytes =
      reader === "worker"
        ? ask({ op: "read", name }).then((reply) => (reply.op === "read" ? reply.buffer : null))
        : readFile(name);
    const read = bytes
      .then((buffer) => {
        files.set(name, buffer);
        const ms = performance.now() - started;
        Object.assign(stat(name), { reader, arrivedMs: ms, bytes: buffer?.byteLength ?? 0 });
        if (timed && buffer) {
          memory.reads = { ...memory.reads, [reader]: ms };
          remember(memory);
        }
      })
      .catch(() => {
        files.set(name, null);
      });
    files.set(name, read);
    return read;
  };

  let stores = Promise.resolve();
  let unstored = 0;
  const written: [string, string, ArrayBuffer][] = [];
  const store = (name: string, version: string, buffer: ArrayBuffer) => {
    stores = stores
      .then(async () => {
        const started = performance.now();
        if (reader === "worker") {
          const reply = await ask({ op: "write", name, version, buffer }, [buffer]);
          if (reply.op === "write" && reply.error) throw new Error(reply.error);
        } else {
          await writeFile(name, version, buffer);
        }
        stat(name).storeMs = performance.now() - started;
        if (!memory.stored) {
          memory.stored = true;
          remember(memory);
        }
      })
      .catch((error) => console.info(`The geometry cache did not keep ${name}: ${error}`))
      .finally(() => unstored--);
  };

  globalThis.sloppyGeometry = {
    stats,
    request,
    take(name) {
      const state = files.get(name);
      if (!(state instanceof ArrayBuffer)) return null;
      files.set(name, null);
      return state;
    },
    when(name, ms) {
      const state = files.get(name);
      if (!(state instanceof Promise)) return Promise.resolve();
      return Promise.race([state, new Promise<void>((resolve) => setTimeout(resolve, ms))]);
    },
    write(name, version, buffer) {
      written.push([name, version, buffer]);
      unstored++;
    },
    flush() {
      for (const file of written.splice(0)) store(...file);
    },
    pending: () => unstored,
  };
  // The files a returning visit needs first.
  if (memory.stored) {
    request("vehicles");
    if (memory.theme) request(memory.theme);
  }
}
