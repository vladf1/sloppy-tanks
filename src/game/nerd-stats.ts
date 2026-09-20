import type { Presentation } from "./presentation";
import type { Simulation } from "./simulation";

/** Counts refresh twice a second, only while the panel is open. */
export class NerdStats {
  private readonly element: HTMLElement;
  private readonly button: HTMLButtonElement;
  private readonly details: HTMLElement;
  private readonly sections = new Map<
    string,
    { list: HTMLElement; rows: Map<string, HTMLPreElement> }
  >();
  private open = false;
  private start = 0;
  private frames = 0;
  private simTotal = 0;
  private renderTotal = 0;

  constructor(
    root: HTMLElement,
    private sim: Simulation,
    private view: Presentation,
    active: () => boolean,
  ) {
    this.element = document.createElement("aside");
    this.element.id = "nerd-stats";
    this.element.setAttribute("aria-label", "Game statistics");
    this.element.innerHTML =
      '<button type="button" aria-expanded="false" aria-controls="nerd-stats-details" aria-keyshortcuts="N">Stats for nerds</button><div id="nerd-stats-details" hidden></div>';
    root.append(this.element);
    this.button = this.element.querySelector("button")!;
    this.details = this.element.querySelector("#nerd-stats-details")!;
    for (const title of ["Performance", "Physics", "Render", "Battle", "Configuration"]) {
      const section = document.createElement("details");
      section.open = title !== "Configuration";
      const heading = document.createElement("summary");
      heading.textContent = title;
      const list = document.createElement("div");
      section.append(heading, list);
      this.details.append(section);
      this.sections.set(title, { list, rows: new Map() });
    }
    this.details.title =
      "Awake/sleeping counts include dynamic bodies only. CPU timings are frame averages, not GPU time or CPU utilization. Sim time is elapsed simulation time. Pickups ready counts available/total. Draw calls and triangles are per rendered frame. GPU geometries and textures are allocated buffers; they change on map load, not per frame. Visual particles count chips, sparks and leaves; smoke has separate buffers.";
    const toggle = () => {
      if (!active()) {
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
      if (!active()) {
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
    const { sim, view } = this;
    let fixed = 0;
    let dynamic = 0;
    let sleeping = 0;
    sim.world.bodies.forEach((body) => {
      if (body.isFixed()) {
        fixed++;
      }
      if (body.isDynamic()) {
        dynamic++;
        if (body.isSleeping()) {
          sleeping++;
        }
      }
    });
    const info = view.renderer.info.render;
    const memory = view.renderer.info.memory;
    const alive = sim.tanks.filter((tank) => tank.alive).length;
    const pickupsReady = sim.pickups.filter((pickup) => pickup.available).length;
    const sections: [string, [string, string | number, string][]][] = [
      [
        "Performance",
        [
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
            "Sim CPU / frame",
            this.frames ? `${(this.simTotal / this.frames).toFixed(2)} ms` : "—",
            "Average CPU time spent stepping the simulation per frame. Excludes GPU work.",
          ],
          [
            "Render CPU / frame",
            this.frames ? `${(this.renderTotal / this.frames).toFixed(2)} ms` : "—",
            "Average CPU time spent submitting draw calls per frame. Excludes GPU work.",
          ],
        ],
      ],
      [
        "Physics",
        [
          ["Bodies", sim.world.bodies.len(), "Rigid bodies in the physics world."],
          ["Fixed / dynamic", `${fixed} / ${dynamic}`, "Static bodies versus simulated bodies."],
          [
            "Awake / sleeping",
            `${dynamic - sleeping} / ${sleeping}`,
            "Simulated bodies awake versus sleeping. Dynamic bodies only.",
          ],
          ["Colliders", sim.world.colliders.len(), "Collision shapes in the physics world."],
        ],
      ],
      [
        "Render",
        [
          ["Draw calls / frame", info.calls, "GPU draw calls issued per rendered frame."],
          [
            "Triangles / frame",
            info.triangles.toLocaleString(),
            "Triangles submitted per rendered frame.",
          ],
        ],
      ],
      [
        "Battle",
        [
          ["Tanks", `${alive} / ${sim.tanks.length}`, "Tanks alive out of total spawned."],
          ["Mines", sim.mines.length, "Live mines on the field."],
          [
            "Pickups ready",
            `${pickupsReady} / ${sim.pickups.length}`,
            "Pickups available now out of total placed.",
          ],
          ["Projectiles", sim.shots.length, "Shots currently flying."],
          [
            "Visual particles",
            view.particles.length,
            "Active chips, sparks and leaves. Smoke uses separate buffers.",
          ],
          [
            "Debris bodies",
            `${sim.fragments.length} / ${sim.maxFragments}`,
            "Physics debris pieces alive out of the pool cap.",
          ],
          [
            "Sim time",
            `${sim.elapsed.toFixed(1)}s`,
            "Elapsed simulation time since the round started.",
          ],
        ],
      ],
      [
        "Configuration",
        [
          [
            "Pixel ratio",
            view.renderer.getPixelRatio(),
            "Renderer resolution multiplier, capped from the display pixel ratio.",
          ],
          [
            "GPU geometries",
            memory.geometries,
            "Distinct geometry buffers currently uploaded to the GPU. Changes on map load, not per frame.",
          ],
          [
            "GPU textures",
            memory.textures,
            "Textures currently uploaded to the GPU. Changes on map load, not per frame.",
          ],
        ],
      ],
    ];
    for (const [title, rows] of sections) {
      const target = this.sections.get(title)!;
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
