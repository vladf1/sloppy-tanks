import { beforeEach, mock, test } from "bun:test";
import assert from "node:assert/strict";
import type { WorkerReply, WorkerRequest } from "../src/geometry-worker.ts";

// The page's IndexedDB reader, standing in for the store the worker and page share.
const stored = new Map<string, ArrayBuffer>();
const pageReads: string[] = [];
const pageWrites: string[] = [];
mock.module("../src/geometry-store.ts", () => ({
  readFile: async (name: string) => {
    pageReads.push(name);
    return stored.get(name)?.slice(0) ?? null;
  },
  writeFile: async (name: string, _version: string, buffer: ArrayBuffer) => {
    pageWrites.push(name);
    stored.set(name, buffer);
  },
}));
const { startGeometryCache } = await import("../src/geometry-cache.ts");

/** A worker the test answers by hand. */
class StubWorker {
  static last: StubWorker | undefined;
  onmessage: ((event: MessageEvent<WorkerReply>) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onmessageerror: ((event: MessageEvent) => void) | null = null;
  requests: WorkerRequest[] = [];
  terminated = false;
  constructor() {
    StubWorker.last = this;
  }
  postMessage(request: WorkerRequest) {
    this.requests.push(request);
  }
  terminate() {
    this.terminated = true;
  }
  reply(data: WorkerReply) {
    this.onmessage?.({ data } as MessageEvent<WorkerReply>);
  }
}

let memory: Record<string, string> = {};
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  stored.clear();
  pageReads.length = 0;
  pageWrites.length = 0;
  StubWorker.last = undefined;
  memory = {};
  Object.assign(globalThis, {
    location: { search: "" },
    indexedDB: {},
    Worker: StubWorker,
    localStorage: {
      getItem: (key: string) => memory[key] ?? null,
      setItem: (key: string, value: string) => (memory[key] = value),
    },
  });
});

const remembered = () => JSON.parse(memory["sloppy-geometry"] ?? "{}");

test("a failed worker settles its requests and the page reads and writes instead", async () => {
  memory["sloppy-geometry"] = JSON.stringify({ stored: true, theme: "scenery-village" });
  stored.set("vehicles", new ArrayBuffer(8));
  stored.set("scenery-village", new ArrayBuffer(16));
  startGeometryCache();
  const cache = globalThis.sloppyGeometry!;
  const worker = StubWorker.last!;
  assert.deepEqual(
    worker.requests.map(({ name }) => name),
    ["vehicles", "scenery-village"],
  );
  cache.write("scenery-desert", "v1", new ArrayBuffer(4));
  cache.flush();
  await settle();
  assert.equal(worker.requests.length, 3);

  worker.onerror?.(new Event("error"));
  await cache.when("vehicles", 1000);
  await cache.when("scenery-village", 1000);
  assert.ok(worker.terminated);
  assert.deepEqual(pageReads, ["vehicles", "scenery-village"]);
  assert.equal(cache.take("vehicles")?.byteLength, 8);
  assert.equal(cache.take("scenery-village")?.byteLength, 16);
  assert.equal(cache.stats["vehicles"]?.["reader"], "page");

  // The write the worker held failed; the next goes through the page.
  await settle();
  assert.equal(cache.pending(), 0);
  cache.write("scenery-forest", "v1", new ArrayBuffer(4));
  cache.flush();
  await settle();
  assert.equal(cache.pending(), 0);
  assert.deepEqual(pageWrites, ["scenery-forest"]);
  assert.equal(worker.requests.length, 3);
});

test("a scenery read that lands after the shell changed theme is not kept", async () => {
  memory["sloppy-geometry"] = JSON.stringify({ stored: true, theme: "scenery-village" });
  startGeometryCache();
  const cache = globalThis.sloppyGeometry!;
  const worker = StubWorker.last!;
  assert.equal(cache.request("scenery-desert") instanceof Promise, true);
  worker.reply({ op: "read", name: "scenery-village", buffer: new ArrayBuffer(16) });
  worker.reply({ op: "read", name: "scenery-desert", buffer: new ArrayBuffer(32) });
  await cache.when("scenery-village", 1000);
  await cache.when("scenery-desert", 1000);
  assert.equal(cache.take("scenery-village"), null);
  assert.equal(cache.take("scenery-desert")?.byteLength, 32);
});

test("asking again for a theme already handed over makes it the remembered one", () => {
  memory["sloppy-geometry"] = JSON.stringify({ stored: false });
  startGeometryCache();
  const cache = globalThis.sloppyGeometry!;
  assert.equal(cache.request("scenery-village"), null);
  assert.equal(cache.request("scenery-desert"), null);
  assert.equal(remembered().theme, "scenery-desert");
  assert.equal(cache.request("scenery-village"), null);
  assert.equal(remembered().theme, "scenery-village");
});
