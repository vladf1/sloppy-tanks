import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { NerdStats } from "../src/game/nerd-stats";

type Listener = () => void;

class StubElement {
  tagName: string;
  id = "";
  children: StubElement[] = [];
  textContent = "";
  title = "";
  hidden = true;
  open = false;
  attributes = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  classList = { toggle(_name: string, _force?: boolean): void {} };

  constructor(tagName = "DIV") {
    this.tagName = tagName;
  }

  set innerHTML(_html: string) {
    const button = new StubElement("BUTTON");
    const container = new StubElement("DIV");
    container.id = "nerd-stats-details";
    this.children = [button, container];
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

type SimParam = ConstructorParameters<typeof NerdStats>[1];
type ViewParam = ConstructorParameters<typeof NerdStats>[2];

const SECTION_TITLES = ["Performance", "Physics", "Render", "Battle", "Configuration"];

function fixture() {
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
  const sim = {
    world: {
      bodies: { forEach(_cb: unknown): void {}, len: () => 10 },
      colliders: { len: () => 4 },
    },
    tanks: [{ alive: true }, { alive: false }, { alive: true }],
    mines: [{}, {}],
    pickups: [{ available: true }, { available: false }],
    fragments: [{}],
    maxFragments: 8,
    shots: [{}, {}, {}],
    match: { phase: "playing", round: 2, time: 84.2, scores: [3, 2], overtime: false },
    elapsed: 12.34,
    seed: 123,
    mapName: "VILLAGE",
  };
  const view = {
    renderer: {
      info: {
        render: { calls: 42, triangles: 123456 },
        memory: { geometries: 9, textures: 11 },
      },
      getPixelRatio: () => 2,
    },
    particles: [{}, {}, {}],
  };
  const root = new StubElement("DIV");
  const stats = new NerdStats(
    root as unknown as HTMLElement,
    sim as unknown as SimParam,
    view as unknown as ViewParam,
    () => true,
  );
  assert.ok(created.length > 0);
  const panel = created[0];
  assert.equal(panel.tagName, "ASIDE");
  const button = panel.querySelector("button");
  const container = panel.querySelector("#nerd-stats-details");
  assert.ok(button);
  assert.ok(container);
  void stats;
  return {
    panel,
    button,
    container,
    dispose() {
      Reflect.deleteProperty(globalThis, "window");
      Reflect.deleteProperty(globalThis, "document");
    },
  };
}

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
    assert.equal(bodies.length, 21);
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
      "Draw calls / frame",
      "Triangles / frame",
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

test("panel header, section headings, and data use different fonts", () => {
  const css = readFileSync(new URL("../src/style.css", import.meta.url), "utf8");
  const ruleBody = (selector: string): string => {
    const escaped = selector.replace(".", "\\.");
    const match = css.match(new RegExp(`${escaped}\\s*\\{([^}]*)}`));
    assert.ok(match, `missing ${selector} rule`);
    return match[1];
  };
  const familyOf = (body: string): string => {
    const match = body.match(/font-family\s*:\s*([^;]+);/);
    assert.ok(match, "rule has no font-family");
    return match[1].trim().toLowerCase();
  };
  const headerFont = familyOf(ruleBody("#nerd-stats button"));
  const sectionFont = familyOf(ruleBody("#nerd-stats summary"));
  const dataRule = ruleBody("#nerd-stats pre");
  const dataFont = familyOf(dataRule);
  assert.ok(
    /pointer-events\s*:\s*auto/.test(dataRule),
    "data rows must re-enable pointer events or hover tooltips never fire",
  );
  assert.ok(!headerFont.includes("mono"), `header must not be monospace: ${headerFont}`);
  assert.ok(!sectionFont.includes("mono"), `section heading must not be monospace: ${sectionFont}`);
  assert.ok(dataFont.includes("mono"), `data must stay monospace: ${dataFont}`);
  assert.notEqual(headerFont, dataFont);
  const headerSize = ruleBody("#nerd-stats button").match(/font-size\s*:\s*([\d.]+)px/);
  assert.ok(headerSize, "header needs an explicit font size");
  assert.ok(
    Number(headerSize[1]) > 11,
    `header must be larger than the 11px data: ${headerSize[1]}px`,
  );
});
