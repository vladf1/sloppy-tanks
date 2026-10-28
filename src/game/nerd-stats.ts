import { typingOrShortcut } from "./controls";
import type { StatsSection } from "./engine-api";

/** The corner link that opens the panel (phones shorten it to "nerds" in CSS), and the
 * panel's title once open. */
const LINK_LABEL = "nerd stats";
const PANEL_TITLE = "Stats for Nerds";
/** Rows show a label padded to this many columns, then the value. */
const LABEL_COLUMNS = 20;
/** The open panel refreshes this often while frames run. */
const REFRESH_MS = 500;

/** Stats for nerds: the engine's sections and rows (`nerd_stats_json`) in folding
 * sections, each row explained on hover. Rows refresh twice a second, only while the
 * panel is open. */
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

  /** `source` reports the panel's sections when it refreshes; `active` says whether the
   * panel may open (a round is on screen). A room's panel (`network`) shows network rows
   * instead of physics, which runs on the server. */
  constructor(
    root: HTMLElement,
    private readonly source: () => StatsSection[] | undefined,
    private readonly active: () => boolean,
    { network = false }: { network?: boolean } = {},
  ) {
    this.element = document.createElement("aside");
    this.element.id = "nerd-stats";
    this.element.setAttribute("aria-label", "Game statistics");
    this.button = document.createElement("button");
    this.button.type = "button";
    this.button.className = "nerd-stats-toggle";
    this.button.setAttribute("aria-expanded", "false");
    this.button.setAttribute("aria-controls", "nerd-stats-details");
    this.button.setAttribute("aria-keyshortcuts", "N");
    this.button.textContent = LINK_LABEL;
    // The open panel's header stays put while its rows scroll, so the close button
    // in its corner is always in reach.
    const close = document.createElement("button");
    close.type = "button";
    close.className = "nerd-stats-close";
    close.setAttribute("aria-label", "Close nerd stats");
    close.textContent = "×";
    const head = document.createElement("div");
    head.className = "nerd-stats-head";
    head.append(this.button, close);
    this.details = document.createElement("div");
    this.details.id = "nerd-stats-details";
    this.details.hidden = true;
    this.element.append(head, this.details);
    root.append(this.element);
    this.details.title = network
      ? "CPU timings are frame averages, not GPU time or CPU utilization. Sim time is elapsed simulation time. Pickups ready counts available/total. Draw calls and triangles are per rendered frame."
      : "Awake/sleeping counts include dynamic bodies only. CPU timings are frame averages, not GPU time or CPU utilization. Sim time is elapsed simulation time. Pickups ready counts available/total. Draw calls and triangles are per rendered frame. GPU geometries, textures and memory change on map load, not per frame. Visual particles count chips, sparks and leaves; smoke has separate buffers.";
    const toggle = () => {
      if (!this.active()) {
        return;
      }
      this.open = !this.open;
      this.element.classList.toggle("expanded", this.open);
      this.button.setAttribute("aria-expanded", String(this.open));
      this.button.textContent = this.open ? PANEL_TITLE : LINK_LABEL;
      this.details.hidden = !this.open;
      this.start = 0;
      if (this.open) {
        this.refresh();
      }
    };
    this.button.addEventListener("click", toggle);
    close.addEventListener("click", () => {
      if (this.open) {
        toggle();
      }
    });
    window.addEventListener("keydown", (event) => {
      if (event.code !== "KeyN" || event.repeat || typingOrShortcut(event)) {
        return;
      }
      if (!this.active()) {
        return;
      }
      event.preventDefault();
      toggle();
    });
    document.addEventListener("visibilitychange", () => {
      this.start = 0;
    });
  }

  /** A frame ran at `now`: refresh once `REFRESH_MS` have passed. */
  frame(now: number): void {
    if (!this.open || document.hidden) {
      return;
    }
    if (this.start === 0) {
      this.start = now;
      return;
    }
    if (now - this.start < REFRESH_MS) {
      return;
    }
    this.refresh();
    this.start = 0;
  }

  private refresh(): void {
    const sections = this.source();
    if (!sections) {
      return;
    }
    for (const { title, rows } of sections) {
      const target = this.sections.get(title) ?? this.addSection(title);
      for (const { label, value, hint } of rows) {
        let row = target.rows.get(label);
        if (!row) {
          row = document.createElement("pre");
          target.list.append(row);
          target.rows.set(label, row);
        }
        // Hints carry counts ("12 so far"), so they refresh with the values.
        row.title = hint;
        row.textContent = `${label.padEnd(LABEL_COLUMNS)} ${value}`;
      }
    }
  }

  /** Sections appear in the engine's order, folded only for Configuration. */
  private addSection(title: string) {
    const section = document.createElement("details");
    section.open = title !== "Configuration";
    const heading = document.createElement("summary");
    heading.textContent = title;
    const list = document.createElement("div");
    section.append(heading, list);
    this.details.append(section);
    const target = { list, rows: new Map<string, HTMLPreElement>() };
    this.sections.set(title, target);
    return target;
  }
}
