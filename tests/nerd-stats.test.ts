import { test } from "node:test";
import assert from "node:assert/strict";
import { NerdStats, engineStatsSections } from "../src/game/nerd-stats";
import type { EngineStats } from "../src/game/engine-api";
import { networkStatsSections, type NetworkStatsSource } from "../src/net/network-stats";

type Listener = () => void;

class StubElement {
  tagName: string;
  id = "";
  children: StubElement[] = [];
  textContent = "";
  type = "";
  title = "";
  hidden = false;
  open = false;
  attributes = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  classList = { toggle(_name: string, _force?: boolean): void {} };

  constructor(tagName = "DIV") {
    this.tagName = tagName;
  }

  private descendants(): StubElement[] {
    return this.children.flatMap((child) => [child, ...child.descendants()]);
  }

  querySelector(selector: string): StubElement | null {
    return this.querySelectorAll(selector)[0] ?? null;
  }

  querySelectorAll(selector: string): StubElement[] {
    const upper = selector.toUpperCase();
    return this.descendants().filter((element) =>
      selector.startsWith("#") ? element.id === selector.slice(1) : element.tagName === upper,
    );
  }

  append(...children: unknown[]): void {
    this.children.push(...(children as StubElement[]));
  }

  setAttribute(key: string, value: string): void {
    this.attributes.set(key, String(value));
  }

  addEventListener(type: string, listener: Listener): void {
    const list = this.listeners.get(type) ?? [];
    list.push(listener);
    this.listeners.set(type, list);
  }

  click(): void {
    for (const listener of this.listeners.get("click") ?? []) {
      listener();
    }
  }
}

const SECTION_TITLES = ["Performance", "Physics", "Render", "Battle", "Configuration"];

function fixture(network = false) {
  const created: StubElement[] = [];
  const win = { addEventListener(_type: string, _listener: Listener): void {} };
  const doc = {
    hidden: false,
    addEventListener(_type: string, _listener: Listener): void {},
    createElement: (tag: string) => {
      const element = new StubElement(tag.toUpperCase());
      created.push(element);
      return element;
    },
  };
  Object.defineProperty(globalThis, "window", { value: win, configurable: true });
  Object.defineProperty(globalThis, "document", { value: doc, configurable: true });
  // A room's `NetGame.stats_json()`: the received scene and the network timeline.
  const room: NetworkStatsSource = {
    drawCalls: 42,
    triangles: 123456,
    shadowTriangles: 23000,
    reflectionTriangles: 40000,
    mainTriangles: 60456,
    meshes: 9,
    textures: 11,
    scene: {
      tanks: 3,
      alive: 2,
      mines: 2,
      pickups: 2,
      pickupsReady: 1,
      shots: 3,
      fragments: 1,
      elapsed: 12.34,
    },
    network: {
      rttMs: 42,
      receivedUpdates: 7,
      snapshotAgeMs: 12,
      bufferMs: 80,
      marginMs: 20,
      underrun: 0,
      serverTick: 900,
      inputSeq: 10,
      inputAck: 9,
      connected: true,
    },
  };
  const engine = {
    bodies: 10,
    fixedBodies: 6,
    dynamicBodies: 4,
    sleepingBodies: 1,
    colliders: 4,
    drawCalls: 42,
    triangles: 123456,
    shadowTriangles: 23000,
    reflectionTriangles: 40000,
    mainTriangles: 60456,
    meshes: 9,
    textures: 11,
    gpuBytes: 3 * 1048576,
    tanks: 3,
    tanksAlive: 2,
    mines: 2,
    pickups: 2,
    pickupsReady: 1,
    shots: 3,
    particles: 3,
    fragments: 1,
    maxFragments: 8,
    elapsed: 12.34,
    pixelRatio: 1.5,
  } as EngineStats;
  const root = new StubElement("DIV");
  let reads = 0;
  const stats = network
    ? new NerdStats(
        root as unknown as HTMLElement,
        networkStatsSections(
          () => {
            reads++;
            return room;
          },
          () => 2,
        ),
        () => true,
        { network: true },
      )
    : new NerdStats(
        root as unknown as HTMLElement,
        () => {
          reads++;
          return engineStatsSections(engine);
        },
        () => true,
      );
  assert.ok(created.length > 0);
  const panel = created[0];
  assert.equal(panel.tagName, "ASIDE");
  assert.equal(root.children[0], panel);
  assert.equal(panel.id, "nerd-stats");
  assert.equal(panel.attributes.get("aria-label"), "Game statistics");
  const button = panel.querySelector("button");
  const container = panel.querySelector("#nerd-stats-details");
  assert.ok(button);
  assert.ok(container);
  assert.equal(button.type, "button");
  assert.equal(button.textContent, "Stats for nerds");
  assert.equal(button.attributes.get("aria-expanded"), "false");
  assert.equal(button.attributes.get("aria-controls"), container.id);
  assert.equal(button.attributes.get("aria-keyshortcuts"), "N");
  assert.equal(container.hidden, true);
  return {
    panel,
    button,
    container,
    stats,
    get reads() {
      return reads;
    },
    dispose() {
      Reflect.deleteProperty(globalThis, "window");
      Reflect.deleteProperty(globalThis, "document");
    },
  };
}

test("network stats use received scene counts and never require a client physics world", () => {
  const f = fixture(true);
  try {
    assert.equal(f.reads, 0, "closed diagnostics do not sample the source");
    f.button.click();
    assert.equal(f.reads, 1);
    assert.equal(f.button.attributes.get("aria-expanded"), "true");
    const titles = f.container
      .querySelectorAll("details")
      .map((section) => section.querySelector("summary")?.textContent);
    assert.ok(titles.includes("Network"));
    assert.ok(!titles.includes("Physics"));
    const text = f.container
      .querySelectorAll("pre")
      .map((row) => row.textContent)
      .join("\n");
    assert.ok(text.includes("RTT") && text.includes("42 ms"));
    assert.ok(text.includes("Input seq sent / ack") && text.includes("10 / 9"));
    assert.ok(text.includes("2 / 3"), "Tanks alive come from the received scene");
    assert.ok(text.includes("Update CPU / frame"));
    assert.ok(!text.includes("Sim CPU / frame"));
    f.stats.frame(1, 1, 2);
    f.stats.frame(501, 1, 2);
    assert.equal(f.reads, 2);
    f.button.click();
    assert.equal(f.container.hidden, true);
    assert.equal(f.button.attributes.get("aria-expanded"), "false");
    f.stats.frame(1001, 1, 2);
    assert.equal(f.reads, 2);
  } finally {
    f.dispose();
  }
});

test("panel has one open section per group with the expected rows", () => {
  const f = fixture();
  try {
    f.button.click();
    assert.equal(f.container.hidden, false);
    const sections = f.container.querySelectorAll("details");
    assert.deepEqual(
      sections.map((section) => section.querySelector("summary")?.textContent),
      SECTION_TITLES,
    );
    for (const section of sections) {
      const title = section.querySelector("summary")?.textContent;
      assert.equal(section.open, title !== "Configuration", `${title} open state`);
    }
    const bodies = f.container.querySelectorAll("pre");
    assert.equal(bodies.length, 22);
    const renderRows = sections
      .find((section) => section.querySelector("summary")?.textContent === "Render")!
      .querySelectorAll("pre")
      .map((row) => row.textContent);
    assert.ok(renderRows.some((row) => row.startsWith("GPU geometries")));
    assert.ok(renderRows.some((row) => row.startsWith("GPU textures")));
    assert.ok(!bodies.some((row) => row.textContent.startsWith("Backend")));
    for (const body of bodies) {
      assert.ok(body.title.length > 0, `row missing tooltip: ${body.textContent}`);
    }
    const tipOf = (label: string): string => {
      const row = bodies.find((body) => body.textContent.startsWith(label));
      assert.ok(row, `missing row: ${label}`);
      return row.title;
    };
    assert.ok(tipOf("Draw calls / frame").includes("per rendered frame"));
    assert.ok(tipOf("GPU geometries").includes("uploaded to the GPU"));
    assert.ok(tipOf("GPU textures").includes("uploaded to the GPU"));
    const text = bodies.map((body) => body.textContent).join("\n");
    for (const row of [
      "Pixel ratio",
      "12.3s",
      "Tanks",
      "2 / 3",
      "Mines",
      "Pickups ready",
      "1 / 2",
      "GPU geometries",
      "GPU textures",
      "GPU memory",
      "3.0 MB",
      "Fixed / dynamic",
      "6 / 4",
      "Awake / sleeping",
      "3 / 1",
      "1 / 8",
      "Draw calls / frame",
      "Triangles / frame",
      "Shadow / reflection / main triangles",
      "23,000 / 40,000 / 60,456",
    ]) {
      assert.ok(text.includes(row), `missing row content: ${row}`);
    }
    assert.ok(!text.includes("Score"));
    assert.ok(!text.includes("per rendered frame")); // tooltip lives on title, not rows
    assert.ok(f.container.title.includes("per rendered frame"));
    assert.ok(!text.includes("Map "));
    assert.ok(!text.includes("Seed"));
    assert.ok(f.container.title.includes("Pickups ready"));
  } finally {
    f.dispose();
  }
});
