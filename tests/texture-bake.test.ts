import { test } from "node:test";
import assert from "node:assert/strict";
import { bakeAhead, startTextureBake } from "../src/game/texture-bake";

/** Stands in for the bake workers: each answers with its band's index as a byte. */
class FakeWorker {
  static started = 0;
  onmessage: ((event: { data: Uint8Array | string }) => void) | null = null;
  onerror: ((event: { message: string }) => void) | null = null;
  constructor() {
    FakeWorker.started++;
  }
  postMessage({ band }: { band: number }): void {
    setTimeout(() => this.onmessage?.({ data: new Uint8Array([band]) }), 0);
  }
  terminate(): void {}
}

const engine = { module: {} as WebAssembly.Module, webgl: false };

/** A game that hands out `key` once and records what it was supplied. */
function game(key: string) {
  let claimed = false;
  const supplied: [string, Uint8Array][] = [];
  return {
    supplied,
    claim_texture_bake(): string | undefined {
      if (claimed) {
        return undefined;
      }
      claimed = true;
      return key;
    },
    supply_texture(claimedKey: string, rgba: Uint8Array): boolean {
      supplied.push([claimedKey, rgba]);
      return true;
    },
    release_texture_bake(): void {
      assert.fail("the bake succeeded");
    },
  };
}

test("an arena takes over the bake started ahead for it, and starts none of its own", async () => {
  const host = globalThis as { Worker?: unknown };
  const original = host.Worker;
  host.Worker = FakeWorker;
  try {
    void bakeAhead(engine, ["soil"]);
    const ahead = FakeWorker.started;
    assert.ok(ahead > 0, "the bake starts at once");
    void bakeAhead(engine, ["soil"]);
    assert.equal(FakeWorker.started, ahead, "a key already baking is not started again");

    const first = game("soil");
    startTextureBake(first, engine);
    assert.equal(FakeWorker.started, ahead, "the claim reuses the bake ahead");
    await new Promise((resolve) => setTimeout(resolve, 10));
    assert.equal(first.supplied.length, 1);
    assert.equal(first.supplied[0][0], "soil");
    assert.deepEqual([...first.supplied[0][1]], [...Array(ahead).keys()], "bands in order");

    // Taken once: a later arena claiming the same key bakes it again.
    const second = game("soil");
    startTextureBake(second, engine);
    assert.equal(FakeWorker.started, 2 * ahead);
    await new Promise((resolve) => setTimeout(resolve, 10));
    assert.equal(second.supplied.length, 1);
  } finally {
    host.Worker = original;
  }
});

/** A bake worker whose script loads only when the test says so (it then posts `null`). */
class LoadingWorker {
  static all: LoadingWorker[] = [];
  onmessage: ((event: { data: Uint8Array | string | null }) => void) | null = null;
  onerror: ((event: { message: string }) => void) | null = null;
  constructor() {
    LoadingWorker.all.push(this);
  }
  load(): void {
    this.onmessage?.({ data: null });
  }
  postMessage(): void {}
  terminate(): void {}
}

test("starting bakes ahead settles once their workers have loaded", async () => {
  const host = globalThis as { Worker?: unknown };
  const original = host.Worker;
  host.Worker = LoadingWorker;
  try {
    let settled = false;
    const loading = bakeAhead(engine, ["dust"]).then(() => {
      settled = true;
    });
    await new Promise((resolve) => setTimeout(resolve, 5));
    assert.equal(settled, false, "the workers have not loaded yet");
    for (const worker of LoadingWorker.all) {
      worker.load();
    }
    await loading;
    assert.equal(settled, true);
    // A map without bakes builds its arena at once.
    await bakeAhead(engine, []);
  } finally {
    host.Worker = original;
  }
});
