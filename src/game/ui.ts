import { AMMO_ORDER, equippedWeapon, hasAmmo } from "./ammunition";
import { SCORE_LIMIT, VEHICLES, WEAPONS } from "./data";
import { healthBarState } from "./health-bar";
import type { Simulation } from "./simulation";
import type { SimEvent, VehicleKind } from "./types";
import { hudMarkup, menuMarkup } from "./ui-markup";
import { rankIndex, RANKS, REPAIR_DELAY } from "./veterancy";
export class UI {
  overlay: HTMLElement;
  hud: HTMLElement;
  toast: HTMLElement;
  feed: HTMLElement;
  lastPhase = "";
  lastDead = false;
  toastTime = 0;
  feedRows: { text: string; time: number }[] = [];
  constructor(
    root: HTMLElement,
    public simulation: Simulation,
    public start: () => void,
    public resume: () => void,
    public restart: () => void,
    public setting: (key: string, value: number) => void,
    public pause: () => void,
  ) {
    root.insertAdjacentHTML("beforeend", hudMarkup());
    this.overlay = root.querySelector("#overlay")!;
    this.hud = root.querySelector("#hud")!;
    this.toast = root.querySelector("#toast")!;
    this.feed = root.querySelector("#feed")!;
    root.querySelector("#pause")!.addEventListener("click", () => {
      if (simulation.match.phase === "playing") {
        this.pause();
        this.lastPhase = "";
      }
    });
  }
  bindCards(): void {
    this.overlay.querySelectorAll<HTMLButtonElement>("[data-kind]").forEach((b) =>
      b.addEventListener("click", () => {
        this.simulation.humanKind = b.dataset.kind as VehicleKind;
        if (this.simulation.match.phase === "ready") {
          this.start();
          return;
        }
        this.overlay.querySelectorAll(".vehicle").forEach((n) => n.classList.remove("selected"));
        b.classList.add("selected");
      }),
    );
  }
  show(): void {
    const simulation = this.simulation;
    const phase = simulation.match.phase;
    this.overlay.style.display = phase === "playing" && simulation.human.alive ? "none" : "grid";
    this.hud.style.opacity = phase === "ready" ? "0" : "1";
    this.overlay.innerHTML = menuMarkup(simulation);
    this.bindCards();
    for (const key of ["gameMode", "mapMode"] as const) {
      this.overlay.querySelectorAll<HTMLInputElement>(`input[name="${key}"]`).forEach((input) =>
        input.addEventListener("change", () => {
          if (key === "gameMode") {
            simulation.gameMode = input.value as Simulation["gameMode"];
          } else {
            simulation.mapMode = input.value as Simulation["mapMode"];
          }
          this.show();
        }),
      );
    }

    this.overlay.querySelector("#resume")?.addEventListener("click", this.resume);
    this.overlay.querySelector("#restart")?.addEventListener("click", this.restart);
    for (const key of ["volume", "tank-speed", "bullet-speed"]) {
      this.overlay.querySelector<HTMLInputElement>("#" + key)?.addEventListener("input", (e) => {
        const value = +(e.target as HTMLInputElement).value;
        this.setting(key, value);
        const output = this.overlay.querySelector(`#${key}-value`);
        if (output) {
          output.textContent = `${Math.round(value * 100)}%`;
        }
      });
    }
  }
  event(event: SimEvent): void {
    if (
      (event.type === "pickup" || event.type === "promotion" || event.type === "death") &&
      event.id === this.simulation.human.id
    ) {
      this.toast.textContent =
        event.type === "death"
          ? `KILLED BY ${this.simulation.tanks.find((tank) => tank.id === event.owner)?.name ?? "YARD"}`
          : (event.label ?? "");
      this.toastTime = event.type === "pickup" ? 1.5 : 2;
      this.toast.classList.add("visible");
    }
    if (event.type === "death" && event.label) {
      this.feedRows.unshift({ text: event.label, time: 5 });
    }
  }
  update(dt: number): void {
    const simulation = this.simulation;
    const tank = simulation.human;
    const dead = !tank.alive;
    if (this.lastPhase !== simulation.match.phase || dead !== this.lastDead) {
      this.lastPhase = simulation.match.phase;
      this.lastDead = dead;
      this.show();
    }
    const set = (id: string, text: string) => {
      const e = document.getElementById(id);
      if (e && e.textContent !== text) {
        e.textContent = text;
      }
    };
    const solo = simulation.gameMode === "solo";
    set("label0", solo ? "KILLS" : "◆ BLUE");
    set("label1", solo ? "ACTIVE" : "RED Ⅱ");
    set("objective", solo ? "SURVIVE · ONE LIFE" : `FIRST TO ${SCORE_LIMIT}`);
    set("score0", String(solo ? tank.kills : simulation.match.scores[0]));
    set(
      "score1",
      String(
        solo
          ? simulation.tanks.filter((tank) => !tank.human && tank.alive).length
          : simulation.match.scores[1],
      ),
    );
    const sec = Math.ceil(simulation.match.time);
    set(
      "time",
      simulation.match.overtime
        ? "NEXT KILL"
        : `${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, "0")}`,
    );
    set("hp", String(Math.ceil(tank.hp)));
    set("vehicle-name", VEHICLES[tank.kind].name);
    const rank = rankIndex(tank);
    const stats = RANKS[rank];
    set("rank", stats.name.toUpperCase());
    const rankLabel = document.getElementById("rank")!;
    rankLabel.dataset.rank = String(rank);
    rankLabel.title =
      rank === 0
        ? "Earn XP from enemy hull damage and kills. Ranks reset on respawn."
        : `+${Math.round((stats.damage - 1) * 100)}% damage · +${Math.round((stats.fireRate - 1) * 100)}% fire rate · +${Math.round((stats.health - 1) * 100)}% hull${stats.repair ? ` · repairs ${stats.repair * 100}% hull/s after ${REPAIR_DELAY}s out of combat` : ""}`;
    const selected = equippedWeapon(tank);
    for (const weapon of AMMO_ORDER) {
      const count = weapon === "standard" ? "∞" : String(tank.ammo[weapon]);
      set(`ammo-count-${weapon}`, count);
      const slot = document.getElementById(`ammo-${weapon}`)!;
      slot.classList.toggle("selected", selected === weapon);
      slot.classList.toggle("empty", !hasAmmo(tank, weapon));
      const label = `${WEAPONS[weapon].label}: ${count}${selected === weapon ? ", selected" : ""}`;
      if (slot.getAttribute("aria-label") !== label) {
        slot.setAttribute("aria-label", label);
      }
    }
    set(
      "mine",
      tank.mineCooldown > 0 ? `MINE ${tank.mineCooldown.toFixed(1)}s` : "MINE READY · RMB",
    );
    set(
      "effects",
      [
        tank.protection > 0 ? "SPAWN SHIELD" : null,
        tank.shield > 0
          ? `◇ SHIELD ${Math.ceil(tank.shieldPoints)} HP · ${Math.ceil(tank.shield)}s`
          : null,
        tank.rapid > 0 ? `» RAPID ${Math.ceil(tank.rapid)}s` : null,
        tank.speed > 0 ? `ϟ BOOST ${Math.ceil(tank.speed)}s` : null,
        tank.laser > 0 ? `✧ LASER DEFENSE ${Math.ceil(tank.laser)}s` : null,
        tank.alive &&
        stats.repair &&
        tank.hp < simulation.maxHealth(tank) &&
        simulation.elapsed - tank.lastCombat >= REPAIR_DELAY
          ? "SELF-REPAIR"
          : null,
      ]
        .filter(Boolean)
        .join("  "),
    );
    set("respawn-count", String(Math.ceil(tank.respawn)));
    const health = healthBarState(tank.hp, simulation.maxHealth(tank), tank.team);
    this.hud
      .querySelector(".status")!
      .classList.toggle("critical-health", tank.alive && health.ratio < 0.25);
    this.hud.classList.toggle("paused", simulation.match.phase !== "playing");
    const hpbar = document.getElementById("hpbar")!;
    hpbar.style.width = `${health.ratio * 100}%`;
    hpbar.style.backgroundColor = `#${health.color.toString(16).padStart(6, "0")}`;
    this.toastTime -= dt;
    if (this.toastTime <= 0) {
      this.toast.classList.remove("visible");
    }
    for (const row of this.feedRows) {
      row.time -= dt;
    }
    this.feedRows = this.feedRows.filter((r) => r.time > 0).slice(0, 4);
    const feed = this.feedRows.map((r) => `<div>${r.text}</div>`).join("");
    if (this.feed.innerHTML !== feed) {
      this.feed.innerHTML = feed;
    }
  }
}
