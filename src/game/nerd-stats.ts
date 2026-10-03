import type { EngineStats } from "./engine-api";

export type StatsRow = [label: string, value: string | number, tip: string];
/** Rows per panel section; Performance is measured by the panel itself. */
export type StatsSections = Partial<Record<string, StatsRow[]>>;

const SINGLE_PLAYER_SECTIONS = ["Performance", "Physics", "Render", "Battle", "Configuration"];
const NETWORK_SECTIONS = ["Performance", "Network", "Render", "Battle", "Configuration"];

/** The single-player panel's rows from the engine's `stats_json`. */
export function engineStatsSections(stats: EngineStats): StatsSections {
  return {
    Physics: [
      ["Bodies", stats.bodies, "Rigid bodies in the physics world."],
      [
        "Fixed / dynamic",
        `${stats.fixedBodies} / ${stats.dynamicBodies}`,
        "Static bodies versus simulated bodies.",
      ],
      [
        "Awake / sleeping",
        `${stats.dynamicBodies - stats.sleepingBodies} / ${stats.sleepingBodies}`,
        "Simulated bodies awake versus sleeping. Dynamic bodies only.",
      ],
      ["Colliders", stats.colliders, "Collision shapes in the physics world."],
    ],
    Render: [
      ["Draw calls / frame", stats.drawCalls, "GPU draw calls issued per rendered frame."],
      [
        "Triangles / frame",
        stats.triangles.toLocaleString(),
        "Triangles submitted per rendered frame.",
      ],
      [
        "Shadow / reflection / main triangles",
        [stats.shadowTriangles, stats.reflectionTriangles, stats.mainTriangles]
          .map((count) => count.toLocaleString())
          .join(" / "),
        "Triangles submitted to each scene pass per rendered frame.",
      ],
      [
        "GPU geometries",
        stats.meshes,
        "Distinct meshes currently uploaded to the GPU, sharing a few mesh page buffers. Changes on map load, not per frame.",
      ],
      [
        "GPU textures",
        stats.textures,
        "Textures currently uploaded to the GPU. Changes on map load, not per frame.",
      ],
      [
        "GPU memory",
        `${(stats.gpuBytes / 1048576).toFixed(1)} MB (${(stats.meshSlackBytes / 1048576).toFixed(1)} MB page slack)`,
        "Estimated GPU memory for mesh pages, textures, render targets, the shadow map and instances. Page slack is mesh page space no mesh uses.",
      ],
    ],
    Battle: [
      ["Tanks", `${stats.tanksAlive} / ${stats.tanks}`, "Tanks alive out of total spawned."],
      ["Mines", stats.mines, "Live mines on the field."],
      [
        "Pickups ready",
        `${stats.pickupsReady} / ${stats.pickups}`,
        "Pickups available now out of total placed.",
      ],
      ["Projectiles", stats.shots, "Shots currently flying."],
      [
        "Visual particles",
        stats.particles,
        "Active chips, sparks and leaves. Smoke uses separate buffers.",
      ],
      [
        "Debris bodies",
        `${stats.fragments} / ${stats.maxFragments}`,
        "Physics debris pieces alive out of the pool cap.",
      ],
      [
        "Sim time",
        `${stats.elapsed.toFixed(1)}s`,
        "Elapsed simulation time since the round started.",
      ],
    ],
    Configuration: [
      [
        "Pixel ratio",
        stats.pixelRatio,
        "Renderer resolution multiplier, capped from the display pixel ratio.",
      ],
    ],
  };
}

/** Counts refresh twice a second, only while the panel is open. */
export class NerdStats {
  private readonly element: HTMLElement;
  private readonly button: HTMLButtonElement;
  private readonly details: HTMLElement;
  private readonly sections = new Map<
    string,
    { list: HTMLElement; rows: Map<string, HTMLPreElement> }
  >();
  private readonly network: boolean;
  private open = false;
  private start = 0;
  private frames = 0;
  private simTotal = 0;
  private renderTotal = 0;

  /** `source` reports the panel's rows when it refreshes; `active` says whether the
   * panel may open (a round is on screen). A room's panel shows network rows instead
   * of physics, which runs on the server. */
  constructor(
    root: HTMLElement,
    private readonly source: () => StatsSections | undefined,
    private readonly active: () => boolean,
    { network = false }: { network?: boolean } = {},
  ) {
    this.network = network;
    this.element = document.createElement("aside");
    this.element.id = "nerd-stats";
    this.element.setAttribute("aria-label", "Game statistics");
    this.button = document.createElement("button");
    this.button.type = "button";
    this.button.setAttribute("aria-expanded", "false");
    this.button.setAttribute("aria-controls", "nerd-stats-details");
    this.button.setAttribute("aria-keyshortcuts", "N");
    this.button.textContent = "Stats for nerds";
    this.details = document.createElement("div");
    this.details.id = "nerd-stats-details";
    this.details.hidden = true;
    this.element.append(this.button, this.details);
    root.append(this.element);
    for (const title of this.network ? NETWORK_SECTIONS : SINGLE_PLAYER_SECTIONS) {
      const section = document.createElement("details");
      section.open = title !== "Configuration";
      const heading = document.createElement("summary");
      heading.textContent = title;
      const list = document.createElement("div");
      section.append(heading, list);
      this.details.append(section);
      this.sections.set(title, { list, rows: new Map() });
    }
    this.details.title = this.network
      ? "CPU timings are frame averages, not GPU time or CPU utilization. Sim time is elapsed simulation time. Pickups ready counts available/total. Draw calls and triangles are per rendered frame."
      : "Awake/sleeping counts include dynamic bodies only. CPU timings are frame averages, not GPU time or CPU utilization. Sim time is elapsed simulation time. Pickups ready counts available/total. Draw calls and triangles are per rendered frame. GPU geometries, textures and memory change on map load, not per frame. Visual particles count chips, sparks and leaves; smoke has separate buffers.";
    const toggle = () => {
      if (!this.active()) {
        return;
      }
      this.open = !this.open;
      this.element.classList.toggle("expanded", this.open);
      this.button.setAttribute("aria-expanded", String(this.open));
      this.details.hidden = !this.open;
      this.reset();
      if (this.open) {
        this.refresh();
      }
    };
    this.button.addEventListener("click", toggle);
    window.addEventListener("keydown", (event) => {
      const target = event.target as HTMLElement | null;
      if (
        event.code !== "KeyN" ||
        event.repeat ||
        event.ctrlKey ||
        event.metaKey ||
        event.altKey ||
        target?.isContentEditable ||
        ["INPUT", "TEXTAREA", "SELECT"].includes(target?.tagName ?? "")
      ) {
        return;
      }
      if (!this.active()) {
        return;
      }
      event.preventDefault();
      toggle();
    });
    document.addEventListener("visibilitychange", () => this.reset());
  }

  private reset(): void {
    this.start = 0;
    this.frames = this.simTotal = this.renderTotal = 0;
  }

  frame(now: number, simCost: number, renderCost: number): void {
    if (!this.open || document.hidden) {
      return;
    }
    if (this.start === 0) {
      this.start = now;
      return;
    }
    this.frames++;
    this.simTotal += simCost;
    this.renderTotal += renderCost;
    if (now - this.start < 500) {
      return;
    }
    this.refresh((now - this.start) / this.frames);
    this.reset();
  }

  private refresh(frameMs?: number): void {
    const sections = this.source();
    if (!sections) {
      return;
    }
    const performance: StatsRow[] = [
      [
        "FPS",
        frameMs ? Math.round(1000 / frameMs) : "—",
        "Rendered frames per second, averaged over the sampling window.",
      ],
      [
        "Frame interval",
        frameMs ? `${frameMs.toFixed(1)} ms` : "—",
        "Average wall-clock time per frame over the sampling window.",
      ],
      [
        this.network ? "Update CPU / frame" : "Sim CPU / frame",
        this.frames ? `${(this.simTotal / this.frames).toFixed(2)} ms` : "—",
        this.network
          ? "Client interpolation, input and effect update CPU time per frame. Excludes message decoding and server physics."
          : "Average CPU time spent stepping the simulation per frame. Excludes GPU work.",
      ],
      [
        "Render CPU / frame",
        this.frames ? `${(this.renderTotal / this.frames).toFixed(2)} ms` : "—",
        "Average CPU time spent submitting draw calls per frame. Excludes GPU work.",
      ],
    ];
    for (const [title, rows] of Object.entries({ Performance: performance, ...sections })) {
      const target = this.sections.get(title);
      if (!target || !rows) {
        continue;
      }
      for (const [label, value, tip] of rows) {
        let row = target.rows.get(label);
        if (!row) {
          row = document.createElement("pre");
          row.title = tip;
          target.list.append(row);
          target.rows.set(label, row);
        }
        row.textContent = `${label.padEnd(20)} ${value}`;
      }
    }
  }
}
