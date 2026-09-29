/** Stats for nerds in a room: the panel of `game/nerd-stats.ts` (same markup and
 * styles), filled from `NetGame.stats_json()` with the network rows. */
type Row = [label: string, value: string | number, tip: string];

/** The part of `NetGame.stats_json()` this panel reads. */
export interface NetworkStatsSource {
  drawCalls: number;
  triangles: number;
  meshes: number;
  textures: number;
  scene?: {
    tanks: number;
    alive: number;
    mines: number;
    pickups: number;
    pickupsReady: number;
    shots: number;
    fragments: number;
    elapsed: number;
  };
  network: {
    rttMs: number;
    receivedUpdates: number;
    snapshotAgeMs: number;
    bufferMs: number;
    marginMs: number;
    underrun: number;
    serverTick: number;
    inputSeq: number;
    inputAck: number;
    connected: boolean;
  };
}

const REFRESH_MS = 500;
const SECTIONS = ["Performance", "Network", "Render", "Battle", "Configuration"];

export class NetworkStats {
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
  private updateTotal = 0;
  private renderTotal = 0;
  private sampleMs = 0;
  private sampleUpdates = 0;

  constructor(
    root: HTMLElement,
    private readonly source: (now: number) => NetworkStatsSource,
    private readonly pixelRatio: () => number,
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
    for (const title of SECTIONS) {
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
      "CPU timings are frame averages, not GPU time or CPU utilization. Sim time is elapsed simulation time. Pickups ready counts available/total. Draw calls and triangles are per rendered frame.";
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
        this.refresh(performance.now());
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
    this.frames = this.updateTotal = this.renderTotal = 0;
  }

  /** One drawn frame and its CPU costs in milliseconds. */
  frame(now: number, updateCost: number, renderCost: number): void {
    if (!this.open || document.hidden) {
      return;
    }
    if (this.start === 0) {
      this.start = now;
      return;
    }
    this.frames++;
    this.updateTotal += updateCost;
    this.renderTotal += renderCost;
    if (now - this.start < REFRESH_MS) {
      return;
    }
    this.refresh(now, (now - this.start) / this.frames);
    this.reset();
  }

  private refresh(now: number, frameMs?: number): void {
    const stats = this.source(now);
    const network = stats.network;
    const scene = stats.scene;
    const rate = this.sampleMs
      ? ((network.receivedUpdates - this.sampleUpdates) * 1000) / (now - this.sampleMs)
      : 0;
    this.sampleMs = now;
    this.sampleUpdates = network.receivedUpdates;
    const average = (total: number) =>
      this.frames ? `${(total / this.frames).toFixed(2)} ms` : "—";
    const sections: [string, Row[]][] = [
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
            "Update CPU / frame",
            average(this.updateTotal),
            "Client interpolation, input and effect update CPU time per frame. Excludes message decoding and server physics.",
          ],
          [
            "Render CPU / frame",
            average(this.renderTotal),
            "Average CPU time spent submitting draw calls per frame. Excludes GPU work.",
          ],
        ],
      ],
      [
        "Network",
        [
          [
            "RTT",
            `${Math.round(network.rttMs)} ms`,
            "Measured round-trip time to the game server.",
          ],
          [
            "Updates received",
            network.receivedUpdates,
            "Full-state messages and snapshot batches received during this page session. A batch can contain several simulation snapshots.",
          ],
          [
            "Update rate",
            `${rate.toFixed(1)} /s`,
            "Full-state messages and snapshot batches received per second, not rendered FPS.",
          ],
          [
            "Snapshot age",
            `${Math.round(network.snapshotAgeMs)} ms`,
            "Time since the last full state or snapshot arrived.",
          ],
          [
            "Playout buffer",
            `${Math.round(network.bufferMs)} ms`,
            "How far other tanks are drawn behind the fastest recent snapshot arrival. It grows when snapshots arrive late and shrinks slowly afterwards.",
          ],
          [
            "Buffered ahead",
            `${Math.round(network.marginMs)} ms`,
            "Received simulation not yet displayed. Negative means snapshots are late and other tanks are briefly extrapolated.",
          ],
          [
            "Underrun",
            `${(network.underrun * 100).toFixed(1)} %`,
            "Share of recent frames drawn past the newest snapshot. Sustained values mean visible stutter.",
          ],
          ["Server tick", network.serverTick, "Latest authoritative simulation tick received."],
          [
            "Input seq sent / ack",
            `${network.inputSeq} / ${network.inputAck}`,
            "Latest input sequence sent and acknowledged by the server. Active input sends up to 20/s; unchanged idle input refreshes once/s to retain your seat. These are not received state updates.",
          ],
          [
            "Connection",
            network.connected ? "Connected" : "Reconnecting",
            "Current game-server connection state.",
          ],
        ],
      ],
      [
        "Render",
        [
          ["Draw calls / frame", stats.drawCalls, "GPU draw calls issued per rendered frame."],
          [
            "Triangles / frame",
            stats.triangles.toLocaleString(),
            "Triangles submitted per rendered frame.",
          ],
          [
            "GPU geometries",
            stats.meshes,
            "Distinct geometry buffers currently uploaded to the GPU. Changes on map load, not per frame.",
          ],
          [
            "GPU textures",
            stats.textures,
            "Textures currently uploaded to the GPU. Changes on map load, not per frame.",
          ],
        ],
      ],
      [
        "Battle",
        scene
          ? [
              ["Tanks", `${scene.alive} / ${scene.tanks}`, "Tanks alive out of total spawned."],
              ["Mines", scene.mines, "Live mines on the field."],
              [
                "Pickups ready",
                `${scene.pickupsReady} / ${scene.pickups}`,
                "Pickups available now out of total placed.",
              ],
              ["Projectiles", scene.shots, "Shots currently flying."],
              [
                "Debris bodies",
                scene.fragments,
                "Debris pieces in the received scene; server physics allocation is not measured here.",
              ],
              [
                "Sim time",
                `${scene.elapsed.toFixed(1)}s`,
                "Elapsed simulation time since the round started.",
              ],
            ]
          : [],
      ],
      [
        "Configuration",
        [
          [
            "Pixel ratio",
            this.pixelRatio(),
            "Renderer resolution multiplier, capped from the display pixel ratio.",
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
