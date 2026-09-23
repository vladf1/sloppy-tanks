import { bindPress } from "./button-input";
import { endBattle } from "./match";
import { AMMO_ORDER, equippedWeapon, hasAmmo } from "./ammunition";
import { SCORE_LIMIT, VEHICLES, WEAPONS } from "./data";
import { bindGameOptions, syncGameOptions } from "./game-options";
import { healthBarState } from "./health-bar";
import type { Simulation } from "./simulation";
import type { RenderState } from "./render-state";
import type { DamageCause, SimEvent, Weapon } from "./types";
import { hudMarkup, menuMarkup } from "./ui-markup";
import { rankIndex, RANKS, REPAIR_DELAY } from "./veterancy";
const DAMAGE_LABELS: Record<DamageCause, string> = {
  standard: "Standard shell",
  spread: "Spread shot",
  rocket: "Rocket blast",
  tow: "TOW missile",
  ricochet: "Ricochet shell",
  piercing: "Piercing shell",
  mine: "Mine explosion",
  drum: "Exploding barrel",
  interception: "Shell collision blast",
  explosion: "Explosion",
};
export class UI {
  displayState?: RenderState;
  overlay: HTMLElement;
  hud: HTMLElement;
  toast: HTMLElement;
  feed: HTMLElement;
  lastPhase = "";
  lastDead = false;
  toastTime = 0;
  ammoNoticeTime = 0;
  damageTime = 0;
  deathCause = "";
  lastRound = 0;
  private readonly battleSetup: HTMLElement;
  feedRows: { text: string; time: number }[] = [];
  constructor(
    root: HTMLElement,
    public simulation: Simulation,
    public start: () => void,
    public resume: () => void,
    public restart: () => void,
    public setting: (key: string, value: number) => void,
    public pause: () => void,
    public selectAmmo: (weapon: Weapon) => void,
    public damageAngle: (event: SimEvent) => number | null,
  ) {
    // Keep the HTML-delivered UI for later rounds, without retaining event handlers.
    this.battleSetup = document
      .querySelector<HTMLElement>("#startup-overlay .start")!
      .cloneNode(true) as HTMLElement;
    const status = this.battleSetup.querySelector("#startup-status");
    if (status) {
      status.textContent = "Ready when you are";
    }
    const hint = this.battleSetup.querySelector(".startup-hint");
    if (hint) {
      hint.textContent = "Choose your next battlefield.";
    }
    const startButton = this.battleSetup.querySelector<HTMLButtonElement>("#start")!;
    startButton.disabled = false;
    startButton.removeAttribute("aria-busy");
    startButton.textContent = "GO!";
    root.insertAdjacentHTML("beforeend", hudMarkup());
    this.overlay = root.querySelector("#overlay")!;
    this.hud = root.querySelector("#hud")!;
    this.toast = root.querySelector("#toast")!;
    this.feed = root.querySelector("#feed")!;
    for (const weapon of AMMO_ORDER) {
      const button = root.querySelector<HTMLButtonElement>(`#ammo-${weapon}`)!;
      bindPress(button, () => this.selectAmmo(weapon));
    }
    bindPress(root.querySelector("#pause")!, () => {
      if (simulation.match.phase === "playing") {
        this.pause();
        this.lastPhase = "";
      }
    });
    const fullscreen = root.querySelector<HTMLButtonElement>("#fullscreen")!;
    fullscreen.hidden = !document.fullscreenEnabled;
    const syncFullscreen = () => {
      const active = document.fullscreenElement !== null;
      const label = active ? "Exit fullscreen" : "Enter fullscreen";
      fullscreen.textContent = active ? "↘↙\n↗↖" : "⛶";
      fullscreen.setAttribute("aria-pressed", String(active));
      fullscreen.setAttribute("aria-label", label);
      fullscreen.title = label;
    };
    document.addEventListener("fullscreenchange", syncFullscreen);
    syncFullscreen();
    const toggleFullscreen = async () => {
      try {
        if (document.fullscreenElement) {
          await document.exitFullscreen();
        } else {
          await document.documentElement.requestFullscreen();
        }
      } catch {
        this.toast.textContent = "Fullscreen unavailable. Try again in a browser tab.";
        this.toastTime = 3;
        this.toast.classList.add("visible");
      }
      fullscreen.blur();
    };
    fullscreen.addEventListener("click", () => {
      void toggleFullscreen();
    });
  }
  show(): void {
    const simulation = this.simulation;
    const phase = simulation.match.phase;
    this.overlay.style.display =
      phase === "playing" && (this.displayState?.viewer.alive ?? simulation.human.alive)
        ? "none"
        : "grid";
    this.hud.style.opacity = phase === "ready" ? "0" : "1";
    if (phase === "ready") {
      this.overlay.replaceChildren(this.battleSetup.cloneNode(true));
      if (simulation.customMap) {
        const details = [
          ["30-Tank Stress Battle", "15 vs 15 · Endless respawns and scoring · No victory"],
          [simulation.customMap.name, simulation.customMap.description],
        ];
        this.overlay.querySelectorAll(".mode-options fieldset").forEach((fieldset, index) => {
          const choices = fieldset.querySelectorAll(".mode-option");
          choices.forEach((choice, i) => {
            if (i > 0) {
              choice.remove();
            }
          });
          choices[0].classList.add("fixed");
          choices[0].querySelector("input")?.remove();
          choices[0].querySelector("b")!.textContent = details[index][0];
          choices[0].querySelector("small")!.textContent = details[index][1];
        });
      }
    } else {
      this.overlay.innerHTML = menuMarkup(
        simulation,
        this.battleSetup.querySelector(".menu-help")!.innerHTML,
        this.displayState?.viewer,
      );
      this.overlay
        .querySelector(".respawn")
        ?.append(this.battleSetup.querySelector(".vehicles")!.cloneNode(true));
    }
    syncGameOptions(this.overlay, simulation);
    bindGameOptions(this.overlay, simulation);
    this.overlay.parentElement?.classList.toggle("menu-ready", phase === "ready");
    this.overlay.dataset.state = "ready";
    const death = this.overlay.querySelector("#death-cause");
    if (death) {
      death.textContent = this.deathCause;
    }
    this.overlay.querySelector("#start")?.addEventListener("click", this.start);
    this.overlay.querySelector("#play-again")?.addEventListener("click", this.start);
    this.overlay.querySelector("#resume")?.addEventListener("click", this.resume);
    this.overlay.querySelector("#end-battle")?.addEventListener("click", () => {
      endBattle(simulation.match);
      this.update(0);
    });
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
  private syncRound(): void {
    if (this.lastRound === this.simulation.match.round) {
      return;
    }
    this.lastRound = this.simulation.match.round;
    this.lastPhase = "";
    this.deathCause = "";
    this.damageTime = 0;
    this.ammoNoticeTime = 0;
    this.toastTime = 0;
    this.toast.classList.remove("visible");
    this.hud.querySelector("#ammo-notice")!.textContent = "";
  }
  event(event: SimEvent): void {
    this.syncRound();
    if (event.id === this.simulation.human.id) {
      if (event.type === "notice" && this.simulation.human.alive) {
        this.hud.querySelector("#ammo-notice")!.textContent = event.label ?? "";
        this.ammoNoticeTime = 3;
      }
      if (event.type === "hurt" || event.type === "death") {
        const angle = this.damageAngle(event);
        const indicator = this.hud.querySelector<HTMLElement>("#damage-direction")!;
        if (angle !== null) {
          indicator.style.transform = `translate(-50%, -50%) rotate(${angle}rad)`;
          indicator.hidden = false;
          this.damageTime = 1.2;
        }
      }
      if (event.type === "death") {
        const killer = this.simulation.tanks.find((tank) => tank.id === event.owner);
        const cause = event.damageSource
          ? DAMAGE_LABELS[event.damageSource.cause]
          : "Unknown weapon";
        const weapon = `${/^[aeiou]/i.test(cause) ? "an" : "a"} ${cause.toLowerCase()}`;
        this.deathCause =
          event.owner === event.id
            ? `You destroyed yourself with ${weapon}.`
            : killer
              ? `${killer.name} killed you with ${weapon}.`
              : `You were destroyed by ${weapon}.`;
        this.hud.querySelector("#ammo-notice")!.textContent = "";
      }
      if (event.type === "respawn") {
        this.deathCause = "";
        this.damageTime = 0;
      }
    }
    if (
      (event.type === "pickup" || event.type === "promotion" || event.type === "death") &&
      event.id === this.simulation.human.id
    ) {
      this.toast.textContent = event.type === "death" ? this.deathCause : (event.label ?? "");
      this.toastTime = event.type === "pickup" ? 1.5 : 2;
      this.toast.classList.add("visible");
    }
    if (event.type === "death") {
      const state = this.displayState;
      const viewerId = state?.viewerId ?? this.simulation.human.id;
      const tanks = state?.tanks ?? this.simulation.tanks;
      const name = (id: number | undefined) =>
        id === viewerId ? "YOU" : (tanks.find((tank) => tank.id === id)?.name ?? "YARD");
      this.feedRows.unshift({ text: `${name(event.owner)}  ▸  ${name(event.id)}`, time: 5 });
    }
  }
  update(dt: number): void {
    const simulation = this.simulation;
    const tank = this.displayState?.viewer ?? simulation.human;
    const maxHp = this.displayState?.viewer.maxHp ?? simulation.maxHealth(simulation.human);
    const match =
      this.displayState && simulation.match.phase === "playing"
        ? this.displayState.match
        : simulation.match;
    const elapsed = this.displayState?.elapsed ?? simulation.elapsed;
    const dead = !tank.alive;
    this.syncRound();
    if (match.phase === "playing") {
      this.damageTime = Math.max(0, this.damageTime - dt);
      this.ammoNoticeTime = Math.max(0, this.ammoNoticeTime - dt);
    }
    const indicator = this.hud.querySelector<HTMLElement>("#damage-direction")!;
    indicator.hidden = this.damageTime <= 0 || match.phase !== "playing";
    indicator.style.opacity = String(Math.min(1, this.damageTime * 2));
    if (this.ammoNoticeTime <= 0) {
      this.hud.querySelector("#ammo-notice")!.textContent = "";
    }
    if (this.lastPhase !== match.phase || dead !== this.lastDead) {
      this.lastPhase = match.phase;
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
    set(
      "objective",
      solo
        ? "SURVIVE · ONE LIFE"
        : simulation.endlessMatch
          ? "ENDLESS STRESS"
          : `FIRST TO ${SCORE_LIMIT}`,
    );
    set("score0", String(solo ? tank.kills : match.scores[0]));
    set(
      "score1",
      String(
        solo
          ? simulation.tanks.filter((tank) => !tank.human && tank.alive).length
          : match.scores[1],
      ),
    );
    const sec = simulation.endlessMatch ? Math.floor(elapsed) : Math.ceil(match.time);
    set(
      "time",
      simulation.endlessMatch
        ? `${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, "0")}`
        : match.overtime
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
      const slot = document.getElementById(`ammo-${weapon}`)! as HTMLButtonElement;
      slot.disabled = dead || match.phase !== "playing";
      slot.setAttribute("aria-pressed", String(selected === weapon));
      slot.classList.toggle("selected", selected === weapon);
      slot.classList.toggle("empty", !hasAmmo(tank, weapon));
      const label = `${AMMO_ORDER.indexOf(weapon) + 1}: ${WEAPONS[weapon].label}, ${count === "0" ? "empty, collect an ammo crate" : count === "∞" ? "unlimited" : count + " remaining"}${selected === weapon ? ", selected" : ""}`;
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
        tank.alive && stats.repair && tank.hp < maxHp && elapsed - tank.lastCombat >= REPAIR_DELAY
          ? "SELF-REPAIR"
          : null,
      ]
        .filter(Boolean)
        .join("  "),
    );
    set("respawn-count", String(Math.ceil(tank.respawn)));
    const health = healthBarState(tank.hp, maxHp, tank.team);
    this.hud
      .querySelector(".status")!
      .classList.toggle("critical-health", tank.alive && health.ratio < 0.25);
    this.hud.classList.toggle("paused", match.phase !== "playing");
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
    while (this.feed.childElementCount > this.feedRows.length) {
      this.feed.lastElementChild!.remove();
    }
    for (let i = 0; i < this.feedRows.length; i++) {
      let row = this.feed.children[i];
      if (!row) {
        row = document.createElement("div");
        this.feed.append(row);
      }
      if (row.textContent !== this.feedRows[i].text) {
        row.textContent = this.feedRows[i].text;
      }
    }
  }
}
