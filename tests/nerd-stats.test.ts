import { test } from "node:test";
import assert from "node:assert/strict";
import { NerdStats } from "../src/game/nerd-stats";
import type { StatsSection } from "../src/game/engine-api";
import { debugPage } from "../src/game/map-options";

// The rows' text and hints are the engine's (`crates/web/src/nerd_rows.rs`, tested
// there); the page lays them out.

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

/** A room's sections as `NetGame.nerd_stats_json()` writes them. */
function roomSections(updates: number): StatsSection[] {
  return [
    {
      title: "Performance",
      rows: [{ label: "FPS", value: "60", hint: "Rendered frames per second." }],
    },
    {
      title: "Network",
      rows: [
        { label: "RTT", value: "42 ms", hint: "Measured round-trip time to the game server." },
        {
          label: "Updates received",
          value: String(updates),
          hint: `Full-state messages and snapshot batches (${updates} so far).`,
        },
      ],
    },
    {
      title: "Render",
      rows: [{ label: "Graphics API", value: "WebGPU", hint: "WebGPU, or WebGL." }],
    },
    { title: "Battle", rows: [] },
    {
      title: "Configuration",
      rows: [{ label: "Pixel ratio", value: "1.5", hint: "Renderer resolution multiplier." }],
    },
  ];
}

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
  const root = new StubElement("DIV");
  let reads = 0;
  const stats = new NerdStats(
    root as unknown as HTMLElement,
    () => roomSections(++reads),
    () => true,
    { network },
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
  assert.equal(button.textContent, "nerd stats");
  assert.equal(button.attributes.get("aria-expanded"), "false");
  assert.equal(button.attributes.get("aria-controls"), container.id);
  assert.equal(button.attributes.get("aria-keyshortcuts"), "N");
  assert.equal(container.hidden, true);
  const rowText = (label: string) =>
    container.querySelectorAll("pre").find((row) => row.textContent.startsWith(label));
  return {
    panel,
    button,
    container,
    stats,
    rowText,
    get reads() {
      return reads;
    },
    dispose() {
      Reflect.deleteProperty(globalThis, "window");
      Reflect.deleteProperty(globalThis, "document");
    },
  };
}

test("the open panel lays out the engine's sections and refreshes twice a second", () => {
  const f = fixture(true);
  try {
    assert.equal(f.reads, 0, "a closed panel does not read the engine");
    f.button.click();
    assert.equal(f.reads, 1);
    assert.equal(f.button.attributes.get("aria-expanded"), "true");
    const sections = f.container.querySelectorAll("details");
    assert.deepEqual(
      sections.map((section) => section.querySelector("summary")?.textContent),
      ["Performance", "Network", "Render", "Battle", "Configuration"],
    );
    for (const section of sections) {
      const title = section.querySelector("summary")?.textContent;
      assert.equal(section.open, title !== "Configuration", `${title} open state`);
    }
    const rtt = f.rowText("RTT");
    assert.equal(rtt?.textContent, "RTT                  42 ms");
    assert.equal(rtt?.title, "Measured round-trip time to the game server.");
    assert.equal(f.rowText("Updates received")?.title.includes("(1 so far)"), true);
    f.stats.frame(1);
    f.stats.frame(400);
    assert.equal(f.reads, 1, "no refresh within half a second");
    f.stats.frame(501);
    assert.equal(f.reads, 2);
    // Rows update in place, hints with their values.
    assert.equal(f.container.querySelectorAll("pre").length, 5);
    assert.equal(f.rowText("Updates received")?.textContent, "Updates received     2");
    assert.equal(f.rowText("Updates received")?.title.includes("(2 so far)"), true);
    f.button.click();
    assert.equal(f.container.hidden, true);
    assert.equal(f.button.attributes.get("aria-expanded"), "false");
    f.stats.frame(1001);
    f.stats.frame(2001);
    assert.equal(f.reads, 2, "a closed panel stops reading");
  } finally {
    f.dispose();
  }
});

test("each panel explains its counters as a whole", () => {
  const room = fixture(true);
  try {
    assert.ok(!room.container.title.includes("Awake/sleeping"));
    assert.ok(room.container.title.includes("per rendered frame"));
  } finally {
    room.dispose();
  }
  const single = fixture();
  try {
    assert.ok(single.container.title.includes("Awake/sleeping"));
    assert.ok(single.container.title.includes("Pickups ready"));
  } finally {
    single.dispose();
  }
});

test("the panel's corner button closes it, and only an open panel", () => {
  const f = fixture();
  try {
    const close = f.panel.querySelectorAll("button")[1];
    assert.equal(close.attributes.get("aria-label"), "Close nerd stats");
    close.click();
    assert.equal(f.container.hidden, true, "a closed panel stays closed");
    f.button.click();
    assert.equal(f.container.hidden, false);
    assert.equal(f.button.textContent, "Stats for Nerds", "the open panel's title");
    close.click();
    assert.equal(f.container.hidden, true);
    assert.equal(f.button.attributes.get("aria-expanded"), "false");
    assert.equal(f.button.textContent, "nerd stats", "the closed panel's link");
  } finally {
    f.dispose();
  }
});

test("Settings' remembered Debug tools make every page a debug page", () => {
  const saved = new Map<string, string>();
  const storage = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: { getItem: (key: string) => saved.get(key) ?? null },
  });
  try {
    saved.set("sloppy-debug", "off");
    assert.equal(debugPage("?room=ABCD2345"), false);
    saved.set("sloppy-debug", "on");
    assert.equal(debugPage(""), true);
    assert.equal(debugPage("?room=ABCD2345"), true);
  } finally {
    if (storage) {
      Object.defineProperty(globalThis, "localStorage", storage);
    } else {
      delete (globalThis as { localStorage?: Storage }).localStorage;
    }
  }
});
