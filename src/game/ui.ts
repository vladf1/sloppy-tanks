import { bindPress } from "./button-input";
import { AMMO_OPTIONS, type AmmoWeapon } from "./ammo-options";
import { bindGameOptions, syncGameOptions, type GameOptions } from "./game-options";
import { bindPlayModes, initialPlayMode } from "./play-modes";
import type { EngineEvent, HudState } from "./engine-api";
import {
  type DeathReport,
  type FeedRow,
  deathCause,
  deathReport,
  isOwnKill,
  killFeedNames,
  killFeedSides,
  newFeedRow,
  placeDamageRing,
  setText,
  showDamageRing,
  showFeed,
  showTankStatus,
  type ScreenPoint,
} from "./hud-feedback";
import { duration } from "./round-recap";
import { controlsMarkup, hudIcon, hudMarkup, menuMarkup } from "./ui-markup";
import { fitPanels } from "./panel-fit";
import { bindDialogTabs, SettingsDialog } from "./settings-dialog";
/** The in-game battle setup's status once its arena is prepared. */
export const MENU_READY_STATUS = "Arena ready";
/** Seconds the hit marker shows after the player's shell lands. */
const HIT_MARKER_TIME = 0.28;
/** Seconds the elimination banner shows after the player wrecks a tank. */
const KILL_BANNER_TIME = 1.9;
/** Seconds the red screen-edge flash fades over after the player's hull is hit. */
const HURT_FLASH_TIME = 0.6;
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
  hitTime = 0;
  killBannerTime = 0;
  hurtFlashTime = 0;
  deathCause = "";
  /** The killer and weapon of the viewer's latest death, for the respawn card. */
  deathReport: DeathReport | null = null;
  /** The respawn countdown's length when it started, so its bar drains to zero. */
  respawnTotal = 0;
  lastRound = 0;
  /** Where the player aims on screen, in normalized device coordinates; null for the
   * screen centre, as in first person. The hit marker appears there. */
  aimPoint: () => ScreenPoint | null = () => null;
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
      setVolume: (value) => actions.setting("volume", value),
      controls: controlsMarkup(),
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
    this.overlay.style.display = phase === "playing" && state.human.alive ? "none" : "grid";
    this.hud.style.opacity = phase === "ready" ? "0" : "1";
    this.playModes?.close();
    this.playModes = undefined;
    if (phase === "ready") {
      this.overlay.replaceChildren(this.battleSetup.cloneNode(true));
      fitPanels(this.overlay);
    } else {
      this.overlay.innerHTML = menuMarkup(
        state,
        this.deathCause ? { report: this.deathReport!, text: this.deathCause } : null,
      );
      this.overlay
        .querySelector(".respawn")
        ?.append(this.battleSetup.querySelector(".vehicles")!.cloneNode(true));
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
    const death = this.overlay.querySelector("#death-cause");
    if (death) {
      death.textContent = this.deathCause;
    }
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
    this.overlay.querySelector("#restart")?.addEventListener("click", () => actions.restart());
  }
  /** The battle report's hero tile shows the viewer's tank in its team's paint: Battle
   * Setup's render of that tank, from the sheet's row for the team. */
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
    this.hitTime = 0;
    this.killBannerTime = 0;
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
  /** One drained engine event; `state` names the tanks it mentions. */
  event(event: EngineEvent, state: HudState): void {
    this.syncRound(state.match.round);
    const human = state.human;
    if (event.playerHit) {
      this.showHitMarker(event.type === "death");
    }
    if (event.id === human.id) {
      if (event.type === "hurt") {
        this.hurtFlashTime = HURT_FLASH_TIME;
      }
      if (event.type === "notice" && human.alive) {
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
      if (event.type === "death") {
        this.deathCause = deathCause(event, state.scoreboard);
        this.deathReport = deathReport(event, state.scoreboard);
        this.respawnTotal = 0;
        this.hud.querySelector("#ammo-notice")!.textContent = "";
      }
      if (event.type === "respawn") {
        this.deathCause = "";
        this.deathReport = null;
        this.damageTime = 0;
      }
      if (event.type === "pickup" || event.type === "promotion" || event.type === "death") {
        this.toast.textContent = event.type === "death" ? this.deathCause : (event.label ?? "");
        this.toast.dataset.tone = event.type;
        this.toastTime = event.type === "pickup" ? 1.5 : 2;
        this.toast.classList.remove("visible");
        // Commit the hidden state so a toast replacing a visible one pops in again.
        void this.toast.offsetWidth;
        this.toast.classList.add("visible");
      }
    }
    if (event.type === "death") {
      const names = killFeedNames(event, human.id, state.scoreboard);
      const ownKill = isOwnKill(event, human.id);
      this.feedRows.unshift(
        newFeedRow(names, ownKill, {
          cause: event.damageSource?.cause,
          sides: killFeedSides(event),
          ownDeath: event.id === human.id,
        }),
      );
      if (ownKill) {
        this.showKillBanner(names[1]);
      }
    }
  }
  /** Seats the hit marker on the reticle. It follows the reticle while it shows, so a
   * marker never hangs where the aim was when the shell landed. */
  private placeHitMarker(marker: HTMLElement): void {
    const aim = this.aimPoint();
    const left = aim ? `${(((aim.x + 1) / 2) * 100).toFixed(2)}%` : "50%";
    const top = aim ? `${(((1 - aim.y) / 2) * 100).toFixed(2)}%` : "50%";
    if (marker.style.left !== left) {
      marker.style.left = left;
    }
    if (marker.style.top !== top) {
      marker.style.top = top;
    }
  }
  /** The hit marker at the player's aim: four ticks that snap out, red on a kill. */
  private showHitMarker(kill: boolean): void {
    const marker = this.hud.querySelector<HTMLElement>("#hit-marker")!;
    this.placeHitMarker(marker);
    marker.classList.remove("visible");
    void marker.offsetWidth;
    marker.classList.toggle("kill", kill);
    marker.classList.add("visible");
    this.hitTime = HIT_MARKER_TIME * (kill ? 1.6 : 1);
  }
  /** The elimination banner below the crosshair, naming the tank the player wrecked. */
  private showKillBanner(victim: string): void {
    const banner = this.hud.querySelector<HTMLElement>("#kill-banner")!;
    banner.querySelector("b")!.textContent = victim;
    banner.classList.remove("visible");
    void banner.offsetWidth;
    banner.classList.add("visible");
    this.killBannerTime = KILL_BANNER_TIME;
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
      this.hitTime = Math.max(0, this.hitTime - dt);
      this.killBannerTime = Math.max(0, this.killBannerTime - dt);
      this.hurtFlashTime = Math.max(0, this.hurtFlashTime - dt);
    }
    const playing = match.phase === "playing";
    // A wrecked tank aims at nothing: its hit marker and kill banner go with it, and the
    // tank panel steps aside for the respawn card.
    this.hud.classList.toggle("dead", dead && match.phase !== "ready");
    const hitMarker = this.hud.querySelector<HTMLElement>("#hit-marker")!;
    if (this.hitTime <= 0 || !playing || dead) {
      hitMarker.classList.remove("visible");
    } else {
      this.placeHitMarker(hitMarker);
    }
    if (this.killBannerTime <= 0 || !playing || dead) {
      this.hud.querySelector("#kill-banner")!.classList.remove("visible");
    }
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
    indicator.hidden = this.damageTime <= 0 || match.phase !== "playing";
    if (!indicator.hidden) {
      placeDamageRing(indicator, this.viewerPoint());
    }
    indicator.style.opacity = String(Math.min(1, this.damageTime * 2));
    if (this.ammoNoticeTime <= 0) {
      this.hud.querySelector("#ammo-notice")!.textContent = "";
    }
    if (this.lastPhase !== match.phase || dead !== this.lastDead) {
      this.lastPhase = match.phase;
      this.lastDead = dead;
      this.show(state);
    }
    const set = (id: string, text: string) => setText(document, id, text);
    const solo = state.gameMode === "solo";
    set("label0", solo ? "KILLS" : "BLUE");
    set("label1", solo ? "ACTIVE" : "RED");
    // Each side's bar fills toward the score limit; your side's plate is marked.
    const scoreboard = this.hud.querySelector<HTMLElement>(".scoreboard")!;
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
    scoreboard.classList.toggle("overtime", !state.endlessMatch && match.overtime);
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
    set("time", !state.endlessMatch && match.overtime ? "NEXT KILL" : duration(sec));
    showTankStatus(this.hud, tank, dead || match.phase !== "playing");
    set("respawn-count", String(Math.ceil(tank.respawn)));
    if (dead) {
      this.respawnTotal = Math.max(this.respawnTotal, tank.respawn);
      const fill = this.overlay.querySelector<HTMLElement>("#respawn-fill");
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
