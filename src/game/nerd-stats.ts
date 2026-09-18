import type { Presentation } from "./presentation";
import type { Simulation } from "./simulation";

/** Counts refresh twice a second, only while the panel is open. */
export class NerdStats {
  private readonly element: HTMLElement;
  private readonly button: HTMLButtonElement;
  private readonly details: HTMLPreElement;
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
      '<button type="button" aria-expanded="false" aria-controls="nerd-stats-details" aria-keyshortcuts="N">Stats for nerds</button><pre id="nerd-stats-details" hidden></pre>';
    root.append(this.element);
    this.button = this.element.querySelector("button")!;
    this.details = this.element.querySelector("pre")!;
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
    const rows: [string, string | number][] = [
      ["FPS", frameMs ? Math.round(1000 / frameMs) : "—"],
      ["Frame interval", frameMs ? `${frameMs.toFixed(1)} ms` : "—"],
      ["Sim CPU / frame", this.frames ? `${(this.simTotal / this.frames).toFixed(2)} ms` : "—"],
      [
        "Render CPU / frame",
        this.frames ? `${(this.renderTotal / this.frames).toFixed(2)} ms` : "—",
      ],
      ["Bodies", sim.world.bodies.len()],
      ["Fixed / dynamic", `${fixed} / ${dynamic}`],
      ["Awake / sleeping", `${dynamic - sleeping} / ${sleeping}`],
      ["Colliders", sim.world.colliders.len()],
      ["Debris bodies", `${sim.fragments.length} / ${sim.maxFragments}`],
      ["Projectiles", sim.shots.length],
      ["Visual particles", view.particles.length],
      ["Draw calls", info.calls],
      ["Triangles", info.triangles.toLocaleString()],
    ];
    this.details.textContent = rows
      .map(([label, value]) => `${label.padEnd(20)} ${value}`)
      .join("\n");
    this.details.title =
      "Awake/sleeping counts include dynamic bodies only. CPU timings are frame averages, not GPU time or CPU utilization. Visual particles count chips, sparks and leaves; smoke has separate buffers.";
  }
}
