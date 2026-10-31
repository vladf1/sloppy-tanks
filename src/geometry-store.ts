// The IndexedDB store of the page's geometry cache (`geometry-cache.ts`), shared by the
// page and its reading worker (`geometry-worker.ts`): one record per file, its version
// (the content version and graphics backend the shell wrote it with) beside the bytes.

const DATABASE = "sloppy-geometry";
const FILES = "files";

interface FileRecord {
  name: string;
  version: string;
  data: Blob;
}

function settled<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

let database: Promise<IDBDatabase> | undefined;

function open(): Promise<IDBDatabase> {
  database ??= (() => {
    const request = indexedDB.open(DATABASE, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(FILES, { keyPath: "name" });
    return settled(request);
  })();
  return database;
}

/** A file's bytes, or null when there is none. */
export async function readFile(name: string): Promise<ArrayBuffer | null> {
  const files = (await open()).transaction(FILES).objectStore(FILES);
  const record = (await settled(files.get(name))) as FileRecord | undefined;
  return record ? record.data.arrayBuffer() : null;
}

/** Keep `buffer` as the file `name` of `version`, and drop every file of another
 * version: a page keeps one version's few files (the vehicles and a scenery per
 * theme, about 60 MB at most). */
export async function writeFile(name: string, version: string, buffer: ArrayBuffer): Promise<void> {
  const transaction = (await open()).transaction(FILES, "readwrite");
  const files = transaction.objectStore(FILES);
  files.put({ name, version, data: new Blob([buffer]) } satisfies FileRecord);
  for (const record of (await settled(files.getAll())) as FileRecord[]) {
    if (record.version !== version) files.delete(record.name);
  }
  await new Promise<void>((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error);
  });
}
