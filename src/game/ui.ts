import { bindPress } from "./button-input";
import { AMMO_OPTIONS, type AmmoWeapon } from "./ammo-options";
import { bindGameOptions, syncGameOptions, type GameOptions } from "./game-options";
import { bindPlayModes, initialPlayMode } from "./play-modes";
import type { EngineEvent, HudState } from "./engine-api";
import { deathCause, effectsLabel, killFeedText, rankTitle } from "./hud-feedback";
import { hudMarkup, menuMarkup } from "./ui-markup";
import { SettingsDialog } from "./settings-dialog";
/** The in-game battle setup's status once its arena is prepared. */
export const MENU_READY_STATUS = "Ready when you are";

export interface UIActions {
  start(): void;
  resume(): void;
  /** BATTLE SETUP: a fresh world behind the menu. */
  restart(): void;
  endBattle(): void;
  pause(): void;
  setting(key: string, value: number): void;
  /** The touch preference Settings shows, and the one they save. */
  touchMode(): string;
  setTouchMode(mode: string): void;
  selectAmmo(weapon: AmmoWeapon): void;
}

/** The DOM HUD, menus and battle report, drawn from the engine's HUD state. */
export class UI {
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
  /** The HUD state of the latest `update`. */
  state?: HudState;
  private readonly battleSetup: HTMLElement;
  private playModes?: { close(): void };
  feedRows: { text: string; time: number }[] = [];
  constructor(
    root: HTMLElement,
    /** The Battle Setup choices the in-game menu edits. */
    private readonly choices: GameOptions,
    private readonly actions: UIActions,
  ) {
    // Keep the HTML-delivered UI for later rounds, without retaining event handlers.
    this.battleSetup = document
      .querySelector<HTMLElement>("#startup-overlay .start")!
      .cloneNode(true) as HTMLElement;
    const status = this.battleSetup.querySelector("#startup-status");
    if (status) {
      status.textContent = MENU_READY_STATUS;
    }
    const hint = this.battleSetup.querySelector(".startup-hint");
    if (hint) {
      hint.textContent = "Choose your next battlefield.";
    }
    const startButton = this.battleSetup.querySelector<HTMLButtonElement>("#start")!;
    startButton.disabled = false;
    startButton.removeAttribute("aria-busy");
    startButton.textContent = "GO!";
    this.battleSetup.querySelectorAll<HTMLButtonElement>('[role="tab"]').forEach((tab) => {
      tab.disabled = false;
    });
    root.insertAdjacentHTML("beforeend", hudMarkup());
    this.overlay = root.querySelector("#overlay")!;
    this.hud = root.querySelector("#hud")!;
    this.toast = root.querySelector("#toast")!;
    this.feed = root.querySelector("#feed")!;
    for (const { weapon } of AMMO_OPTIONS) {
      const button = root.querySelector<HTMLButtonElement>(`#ammo-${weapon}`)!;
      bindPress(button, () => actions.selectAmmo(weapon));
    }
    bindPress(root.querySelector("#pause")!, () => {
      if (this.state?.match.phase === "playing") {
        actions.pause();
        this.lastPhase = "";
      }
    });
    // Settings open only during play (the corner button hides behind menus): they pause
    // the battle, and closing them carries it on.
    let resumeAfterSettings = false;
    new SettingsDialog(root, "SINGLE PLAYER", {
      touchMode: () => actions.touchMode(),
      setTouchMode: (mode) => actions.setTouchMode(mode),
      setVolume: (value) => actions.setting("volume", value),
      speeds: {
        get: () => this.state?.speedTuning,
        set: (key, value) => actions.setting(key, value),
      },
      opened: () => {
        resumeAfterSettings = this.state?.match.phase === "playing";
        if (resumeAfterSettings) {
          actions.pause();
          this.lastPhase = "";
        }
      },
      closed: () => {
        if (resumeAfterSettings && this.state?.match.phase === "paused") {
          actions.resume();
        }
        resumeAfterSettings = false;
      },
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
  private show(state: HudState): void {
    const phase = state.match.phase;
    this.overlay.style.display = phase === "playing" && state.human.alive ? "none" : "grid";
    this.hud.style.opacity = phase === "ready" ? "0" : "1";
    this.playModes?.close();
    this.playModes = undefined;
    if (phase === "ready") {
      this.overlay.replaceChildren(this.battleSetup.cloneNode(true));
    } else {
      this.overlay.innerHTML = menuMarkup(
        state,
        this.battleSetup.querySelector(".menu-help")!.innerHTML,
      );
      this.overlay
        .querySelector(".respawn")
        ?.append(this.battleSetup.querySelector(".vehicles")!.cloneNode(true));
    }
    const choices = this.choices;
    syncGameOptions(this.overlay, choices);
    bindGameOptions(this.overlay, choices);
    const setup = this.overlay.querySelector<HTMLElement>(".start");
    if (setup) {
      // This page already runs a single-player arena, so a chosen room opens in a fresh page.
      this.playModes = bindPlayModes(setup, initialPlayMode(location.search), {
        choices: () => choices,
        single: () => {},
        enterRoom: (_selection, reload) => reload(),
      });
    }
    this.overlay.parentElement?.classList.toggle("menu-ready", phase === "ready");
    this.overlay.dataset.state = "ready";
    const death = this.overlay.querySelector("#death-cause");
    if (death) {
      death.textContent = this.deathCause;
    }
    const { actions } = this;
    this.overlay.querySelector("#start")?.addEventListener("click", () => actions.start());
    this.overlay.querySelector("#play-again")?.addEventListener("click", () => actions.start());
    this.overlay.querySelector("#resume")?.addEventListener("click", () => actions.resume());
    this.overlay.querySelector("#end-battle")?.addEventListener("click", () => actions.endBattle());
    this.overlay.querySelector("#restart")?.addEventListener("click", () => actions.restart());
  }
  /** A new round, or a Battle Setup world, starts with no feedback from the last. */
  private syncRound(round: number): void {
    if (this.lastRound === round) {
      return;
    }
    this.lastRound = round;
    this.lastPhase = "";
    this.deathCause = "";
    this.damageTime = 0;
    this.ammoNoticeTime = 0;
    this.toastTime = 0;
    this.toast.classList.remove("visible");
    this.hud.querySelector("#ammo-notice")!.textContent = "";
  }
  /** Show the menu for the current phase again (after a pause the page caused). */
  refresh(): void {
    this.lastPhase = "";
  }
  /** One drained engine event; `state` names the tanks it mentions. */
  event(event: EngineEvent, state: HudState): void {
    this.syncRound(state.match.round);
    const human = state.human;
    if (event.id === human.id) {
      if (event.type === "notice" && human.alive) {
        this.hud.querySelector("#ammo-notice")!.textContent = event.label ?? "";
        this.ammoNoticeTime = 3;
      }
      if (event.type === "hurt" || event.type === "death") {
        const indicator = this.hud.querySelector<HTMLElement>("#damage-direction")!;
        if (event.damageAngle !== null) {
          indicator.style.transform = `translate(-50%, -50%) rotate(${event.damageAngle}rad)`;
          indicator.hidden = false;
          this.damageTime = 1.2;
        }
      }
      if (event.type === "death") {
        this.deathCause = deathCause(event, state.scoreboard);
        this.hud.querySelector("#ammo-notice")!.textContent = "";
      }
      if (event.type === "respawn") {
        this.deathCause = "";
        this.damageTime = 0;
      }
    }
    if (
      (event.type === "pickup" || event.type === "promotion" || event.type === "death") &&
      event.id === human.id
    ) {
      this.toast.textContent = event.type === "death" ? this.deathCause : (event.label ?? "");
      this.toastTime = event.type === "pickup" ? 1.5 : 2;
      this.toast.classList.add("visible");
    }
    if (event.type === "death") {
      this.feedRows.unshift({ text: killFeedText(event, human.id, state.scoreboard), time: 5 });
    }
  }
  update(state: HudState, dt: number): void {
    this.state = state;
    const tank = state.human;
    const { match, elapsed } = state;
    const dead = !tank.alive;
    this.syncRound(match.round);
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
      this.show(state);
    }
    const set = (id: string, text: string) => {
      const e = document.getElementById(id);
      if (e && e.textContent !== text) {
        e.textContent = text;
      }
    };
    const solo = state.gameMode === "solo";
    set("label0", solo ? "KILLS" : "◆ BLUE");
    set("label1", solo ? "ACTIVE" : "RED Ⅱ");
    set(
      "objective",
      solo
        ? "SURVIVE · ONE LIFE"
        : state.endlessMatch
          ? "ENDLESS STRESS"
          : `FIRST TO ${state.scoreLimit}`,
    );
    set("score0", String(solo ? tank.kills : match.scores[0]));
    set("score1", String(solo ? state.activeEnemies : match.scores[1]));
    const sec = state.endlessMatch ? Math.floor(elapsed) : Math.ceil(match.time);
    set(
      "time",
      state.endlessMatch
        ? `${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, "0")}`
        : match.overtime
          ? "NEXT KILL"
          : `${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, "0")}`,
    );
    set("hp", String(Math.ceil(tank.hp)));
    set("vehicle-name", tank.vehicleName);
    const rank = tank.rank;
    set("rank", tank.rankName.toUpperCase());
    const rankLabel = document.getElementById("rank")!;
    rankLabel.dataset.rank = String(rank);
    rankLabel.title = rankTitle(tank);
    AMMO_OPTIONS.forEach(({ weapon, label: name }, index) => {
      const slotState = tank.ammo.find((slot) => slot.weapon === weapon);
      const selected = !!slotState?.selected;
      const count = slotState?.count === null ? "∞" : String(slotState?.count ?? 0);
      set(`ammo-count-${weapon}`, count);
      const slot = document.getElementById(`ammo-${weapon}`)! as HTMLButtonElement;
      slot.disabled = dead || match.phase !== "playing";
      slot.setAttribute("aria-pressed", String(selected));
      slot.classList.toggle("selected", selected);
      slot.classList.toggle("empty", !slotState?.available);
      const label = `${index + 1}: ${name}, ${count === "0" ? "empty, collect an ammo crate" : count === "∞" ? "unlimited" : count + " remaining"}${selected ? ", selected" : ""}`;
      if (slot.getAttribute("aria-label") !== label) {
        slot.setAttribute("aria-label", label);
      }
    });
    set(
      "mine",
      tank.mineCooldown > 0 ? `MINE ${tank.mineCooldown.toFixed(1)}s` : "MINE READY · RMB",
    );
    set("effects", effectsLabel(tank));
    set("respawn-count", String(Math.ceil(tank.respawn)));
    this.hud
      .querySelector(".status")!
      .classList.toggle("critical-health", tank.alive && tank.healthRatio < 0.25);
    this.hud.classList.toggle("paused", match.phase !== "playing");
    const hpbar = document.getElementById("hpbar")!;
    hpbar.style.width = `${tank.healthRatio * 100}%`;
    hpbar.style.backgroundColor = `#${tank.healthColor.toString(16).padStart(6, "0")}`;
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
