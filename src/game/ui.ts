import { bindPress } from "./button-input";
import { AMMO_OPTIONS, type AmmoWeapon } from "./ammo-options";
import { bindGameOptions, syncGameOptions, type GameOptions } from "./game-options";
import { bindPlayModes, initialPlayMode } from "./play-modes";
import type { DeathReport, EngineEvent, HudState, PauseReport } from "./engine-api";
import {
  type FeedRow,
  newFeedRow,
  placeDamageRing,
  setText,
  showDamageRing,
  showFeed,
  showTankStatus,
  type ScreenPoint,
} from "./hud-feedback";
import { duration } from "./round-recap";
import { hudIcon, hudMarkup, menuMarkup, respawnMarkup, wreckedByMarkup } from "./ui-markup";
import { fitPanels } from "./panel-fit";
import { isPhone } from "./phone-mode";
import { bindDialogTabs, SettingsDialog } from "./settings-dialog";
/** The in-game battle setup's status once its arena is prepared. */
export const MENU_READY_STATUS = "Arena ready";
/** Seconds the red screen-edge flash fades over after the player's hull is hit. */
const HURT_FLASH_TIME = 0.6;
/** Seconds a pickup's toast shows, and a promotion's or the player's death's. */
const PICKUP_TOAST_TIME = 1.5;
const TOAST_TIME = 2;
/** Below this share of its hull the tank's HUD warns, like the status panel's pulse. */
const CRITICAL_HEALTH = 0.25;

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
  /** The Graphics setting Settings shows, and the one they save. */
  graphics(): string;
  setGraphics(setting: string): void;
  selectAmmo(weapon: AmmoWeapon): void;
  /** Pause's facts and standing line from the engine, read as the pause menu opens. */
  pauseReport(): PauseReport;
}

/** The DOM HUD, menus and battle report, drawn from the engine's HUD state. */
export class UI {
  overlay: HTMLElement;
  /** The respawn card's layer, under the menus (see `showRespawn`). */
  respawnLayer: HTMLElement;
  hud: HTMLElement;
  toast: HTMLElement;
  feed: HTMLElement;
  lastPhase = "";
  lastDead = false;
  toastTime = 0;
  ammoNoticeTime = 0;
  damageTime = 0;
  hurtFlashTime = 0;
  deathCause = "";
  /** The killer and weapon of the viewer's latest death, for the respawn card. */
  deathReport: DeathReport | null = null;
  /** The respawn countdown's length when it started, so its bar drains to zero. */
  respawnTotal = 0;
  /** The animation frame that shows the filled respawn card (see `showRespawn`). */
  private respawnShow = 0;
  lastRound = 0;
  /** Where the viewer's tank is on screen (NDC); null in first person. The damage ring
   * wraps it. */
  viewerPoint: () => ScreenPoint | null = () => null;
  /** The HUD state of the latest `update`. */
  state?: HudState;
  private readonly battleSetup: HTMLElement;
  private readonly settings: SettingsDialog;
  private playModes?: { close(): void };
  feedRows: FeedRow[] = [];
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
      hint.textContent = "";
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
    this.respawnLayer = root.querySelector("#respawn-layer")!;
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
    this.settings = new SettingsDialog(root, "SINGLE PLAYER", {
      touchMode: () => actions.touchMode(),
      setTouchMode: (mode) => actions.setTouchMode(mode),
      graphics: () => actions.graphics(),
      setGraphics: (setting) => actions.setGraphics(setting),
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
      fullscreen.innerHTML = hudIcon(active ? "hud-collapse" : "hud-expand");
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
    this.overlay.style.display = phase === "playing" ? "none" : "grid";
    if (phase === "ready") {
      // Battle Setup has the tank cards; the battle builds its own respawn card.
      this.respawnLayer.replaceChildren();
    } else if (!this.respawnLayer.firstElementChild) {
      this.buildRespawn();
    }
    this.showRespawn(phase === "playing" && !state.human.alive);
    this.hud.style.opacity = phase === "ready" ? "0" : "1";
    this.playModes?.close();
    this.playModes = undefined;
    if (phase === "ready") {
      this.overlay.replaceChildren(this.battleSetup.cloneNode(true));
      fitPanels(this.overlay);
    } else {
      // In play the menus are away: the respawn card has a layer of its own.
      const death = this.deathCause ? { report: this.deathReport!, text: this.deathCause } : null;
      const pause = phase === "paused" ? this.actions.pauseReport() : null;
      this.overlay.innerHTML = phase === "playing" ? "" : menuMarkup(state, death, pause);
      bindDialogTabs(this.overlay);
      this.drawRecapTank(state);
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
    const { actions } = this;
    this.overlay.querySelector("#start")?.addEventListener("click", () => actions.start());
    this.overlay.querySelector("#play-again")?.addEventListener("click", () => actions.start());
    for (const resume of this.overlay.querySelectorAll("#resume, #pause-close")) {
      resume.addEventListener("click", () => actions.resume());
    }
    this.overlay
      .querySelector("#pause-settings")
      ?.addEventListener("click", () => this.settings.open());
    this.overlay.querySelector("#end-battle")?.addEventListener("click", () => actions.endBattle());
    for (const restart of this.overlay.querySelectorAll("#restart")) {
      restart.addEventListener("click", () => actions.restart());
    }
  }
  /** Build the battle's view before its round starts: the respawn card and the HUD at
   * full opacity, so the browser paints them before the round's first frame. */
  prepareBattle(): void {
    if (!this.respawnLayer.firstElementChild) {
      this.buildRespawn();
    }
    this.hud.style.opacity = "1";
  }
  /** The battle report's hero tile shows the viewer's tank in its team's paint: Battle
   * Setup's render of that tank, from the sheet's row for the team. */
  /** The battle's respawn card with its tank cards, built as the battle starts. */
  private buildRespawn(): void {
    const layer = this.respawnLayer;
    layer.innerHTML = respawnMarkup(null);
    layer
      .querySelector(".respawn")!
      .append(this.battleSetup.querySelector(".vehicles")!.cloneNode(true));
    bindGameOptions(layer, this.choices);
  }
  /** Show or hide the respawn card. It stays built and drawn, unseen, between deaths, so
   * a death only fills in who did it and shows it: building and painting the card on the
   * death frame cost Safari on an iPad 30 ms, every death. The card is filled on the
   * death frame and shown on the next, so Safari repaints it and starts its entrance on
   * different frames: together, with the death's other HUD changes, they held an iPad's
   * death frame 10 ms longer. */
  private showRespawn(shown: boolean): void {
    const layer = this.respawnLayer;
    cancelAnimationFrame(this.respawnShow);
    this.respawnShow = 0;
    if (!shown) {
      layer.classList.remove("shown");
      layer.inert = true;
      return;
    }
    if (layer.firstElementChild) {
      layer.querySelector(".wrecked-by")!.outerHTML = wreckedByMarkup(
        this.deathCause ? this.deathReport : null,
      );
      layer.querySelector("#death-cause")!.textContent = this.deathCause;
      syncGameOptions(layer, this.choices);
    }
    this.respawnShow = requestAnimationFrame(() => {
      this.respawnShow = 0;
      layer.classList.add("shown");
      layer.inert = false;
    });
  }
  private drawRecapTank(state: HudState): void {
    const slot = this.overlay.querySelector(".recap-tank");
    const preview = this.battleSetup.querySelector(
      `.vehicle[data-kind="${state.human.kind}"] .tank-preview`,
    );
    if (!slot || !preview) {
      return;
    }
    const tank = preview.cloneNode(true) as SVGElement;
    tank.querySelector("image")?.setAttribute("y", String(-state.humanTeam * 400));
    tank.removeAttribute("role");
    tank.setAttribute("aria-hidden", "true");
    slot.replaceChildren(tank);
  }
  /** Re-seats a showing damage ring after the view changes (first person opens it to the
   * screen's edges), without waiting for the next HUD frame. */
  replaceDamageRing(): void {
    const indicator = this.hud.querySelector<HTMLElement>("#damage-direction")!;
    if (!indicator.hidden) {
      placeDamageRing(indicator, this.viewerPoint());
    }
  }
  /** A new round, or a Battle Setup world, starts with no feedback from the last. */
  private syncRound(round: number): void {
    if (this.lastRound === round) {
      return;
    }
    this.lastRound = round;
    this.lastPhase = "";
    this.deathCause = "";
    this.deathReport = null;
    this.damageTime = 0;
    this.hurtFlashTime = 0;
    this.ammoNoticeTime = 0;
    this.toastTime = 0;
    this.toast.classList.remove("visible");
    this.hud.querySelector("#ammo-notice")!.textContent = "";
  }
  /** Show the menu for the current phase again (after a pause the page caused). */
  refresh(): void {
    this.lastPhase = "";
  }
  /** One drained engine event, with the feedback the engine attached; `state` is the
   * latest HUD. */
  event(event: EngineEvent, state: HudState): void {
    this.syncRound(state.match.round);
    if (event.own) {
      if (event.type === "hurt") {
        this.hurtFlashTime = HURT_FLASH_TIME;
      }
      if (event.type === "notice" && state.human.alive) {
        this.hud.querySelector("#ammo-notice")!.textContent = event.label ?? "";
        this.ammoNoticeTime = 3;
      }
      if (event.type === "hurt" || event.type === "death") {
        const indicator = this.hud.querySelector<HTMLElement>("#damage-direction")!;
        if (event.damageAngle !== null) {
          showDamageRing(indicator, event.damageAngle, this.viewerPoint());
          this.damageTime = 1.2;
        }
      }
      if (event.deathReport) {
        this.deathCause = event.deathReport.sentence;
        this.deathReport = event.deathReport;
        this.respawnTotal = 0;
        this.hud.querySelector("#ammo-notice")!.textContent = "";
      }
      if (event.type === "respawn") {
        this.deathCause = "";
        this.deathReport = null;
        this.damageTime = 0;
      }
    }
    if (event.toast) {
      // A fresh element pops in from the start, also over a toast still showing;
      // restarting the old one's entrance would need a forced style and layout pass.
      const toast = this.toast.cloneNode(false) as HTMLElement;
      toast.textContent = event.toast.text;
      toast.dataset.tone = event.toast.tone;
      toast.classList.add("visible");
      this.toast.replaceWith(toast);
      this.toast = toast;
      this.toastTime = event.toast.tone === "pickup" ? PICKUP_TOAST_TIME : TOAST_TIME;
    }
    if (event.feed) {
      const { names, ownKill, ...details } = event.feed;
      this.feedRows.unshift(newFeedRow(names, ownKill, details));
    }
  }
  update(state: HudState, dt: number): void {
    this.state = state;
    const tank = state.human;
    const { match } = state;
    const dead = !tank.alive;
    this.syncRound(match.round);
    if (match.phase === "playing") {
      this.damageTime = Math.max(0, this.damageTime - dt);
      this.ammoNoticeTime = Math.max(0, this.ammoNoticeTime - dt);
      this.hurtFlashTime = Math.max(0, this.hurtFlashTime - dt);
    }
    const playing = match.phase === "playing";
    // A wrecked tank's panel steps aside for the respawn card.
    this.hud.classList.toggle("dead", dead && match.phase !== "ready");
    const vignette = this.hud.querySelector<HTMLElement>("#damage-vignette")!;
    const flash = playing ? (this.hurtFlashTime / HURT_FLASH_TIME).toFixed(2) : "0";
    if (vignette.style.opacity !== flash) {
      vignette.style.opacity = flash;
    }
    this.hud.classList.toggle(
      "critical",
      playing && tank.alive && tank.healthRatio < CRITICAL_HEALTH,
    );
    const indicator = this.hud.querySelector<HTMLElement>("#damage-direction")!;
    const hideIndicator = this.damageTime <= 0 || match.phase !== "playing";
    if (indicator.hidden !== hideIndicator) {
      indicator.hidden = hideIndicator;
    }
    if (!indicator.hidden) {
      placeDamageRing(indicator, this.viewerPoint());
    }
    indicator.style.opacity = String(Math.min(1, this.damageTime * 2));
    if (this.ammoNoticeTime <= 0) {
      this.hud.querySelector("#ammo-notice")!.textContent = "";
    }
    if (this.lastPhase !== match.phase || dead !== this.lastDead) {
      // A death or respawn in play only shows or hides the respawn card: the menus
      // stay away, and rebuilding them restyled the page on Safari's death frames.
      const inPlay =
        this.lastPhase === "playing" &&
        match.phase === "playing" &&
        this.respawnLayer.firstElementChild !== null;
      this.lastPhase = match.phase;
      this.lastDead = dead;
      if (inPlay) {
        this.showRespawn(dead);
      } else {
        this.show(state);
      }
    }
    const set = (id: string, text: string) => setText(document, id, text);
    const solo = state.gameMode === "solo";
    set("label0", solo ? "KILLS" : "BLUE");
    // Each side's bar fills toward the score limit; your side's plate is marked. A solo run
    // has no other side (its bots refill to six at once), so its pill is your kills and the
    // clock.
    const scoreboard = this.hud.querySelector<HTMLElement>(".scoreboard")!;
    scoreboard.classList.toggle("solo", solo);
    scoreboard.classList.toggle("no-limit", solo || state.endlessMatch);
    match.scores.forEach((score, team) => {
      const fill = `${Math.min(1, score / Math.max(1, state.scoreLimit)) * 100}%`;
      const bar = this.hud.querySelector<HTMLElement>(`#progress${team}`)!;
      if (bar.style.width !== fill) {
        bar.style.width = fill;
      }
      scoreboard
        .querySelector(team === 0 ? ".mint" : ".coral")!
        .classList.toggle("yours", !solo && team === state.humanTeam);
    });
    // The tank panel's glass takes a tint of your team (none in a solo run).
    const side = solo ? "" : state.humanTeam === 0 ? "blue" : "red";
    if ((this.hud.dataset.team ?? "") !== side) {
      this.hud.dataset.team = side;
    }
    const pill = state.scorePill;
    scoreboard.classList.toggle("overtime", pill.clockLabel !== null);
    // A phone's score pill has room for the goal alone; its pause menu adds the rule.
    set("objective", solo && isPhone() ? "SURVIVE" : (pill.objective ?? ""));
    set("score0", String(solo ? tank.kills : match.scores[0]));
    set("score1", String(match.scores[1]));
    set("time", pill.clockLabel ?? duration(pill.seconds));
    showTankStatus(this.hud, tank, dead || match.phase !== "playing");
    set("respawn-count", String(Math.ceil(tank.respawn)));
    if (dead) {
      this.respawnTotal = Math.max(this.respawnTotal, tank.respawn);
      const fill = this.respawnLayer.querySelector<HTMLElement>("#respawn-fill");
      const left = this.respawnTotal > 0 ? tank.respawn / this.respawnTotal : 0;
      fill?.style.setProperty("--left", left.toFixed(3));
    }
    this.hud.classList.toggle("paused", match.phase !== "playing");
    // Notices belong to play: a menu or the battle report never shows one behind it.
    this.toastTime = playing ? this.toastTime - dt : 0;
    if (this.toastTime <= 0) {
      this.toast.classList.remove("visible");
    }
    this.feedRows = showFeed(this.feed, this.feedRows, dt);
  }
}
