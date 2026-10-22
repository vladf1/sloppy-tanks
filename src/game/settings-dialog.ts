import type { HudState } from "./engine-api";
import { savedPreference } from "./player-preferences";

type SpeedKey = keyof HudState["speedTuning"];

const SPEED_LABELS: Record<SpeedKey, string> = {
  "tank-speed": "Tank base speed",
  "bullet-speed": "Projectile base speed",
};
const DEFAULT_VOLUME = 0.6;
/** The touch preference's choices. A segmented control shows them; the hidden select
 * behind it holds the value Save reads. */
const TOUCH_MODES = [
  ["auto", "Auto"],
  ["on", "On"],
  ["off", "Off"],
] as const;

/** The HUD's corner button that opens the dialog. */
export const SETTINGS_BUTTON = `<button id="settings-open" class="quiet" type="button" aria-label="Settings" title="Settings" aria-haspopup="dialog"><svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M10.23 4.61L10.8 1.87h2.4l.57 2.74 2.2.91 2.34-1.53 1.7 1.7-1.53 2.34.91 2.2 2.74.57v2.4l-2.74.57-.91 2.2 1.53 2.34-1.7 1.7-2.34-1.53-2.2.91-.57 2.74h-2.4l-.57-2.74-2.2-.91-2.34 1.53-1.7-1.7 1.53-2.34-.91-2.2-2.74-.57v-2.4l2.74-.57.91-2.2-1.53-2.34 1.7-1.7 2.34 1.53z" /><circle cx="12" cy="12" r="3.2" /></svg></button>`;

/** The saved sound level, shared by single player and rooms. */
export function savedVolume(): number {
  const value = Number(savedPreference("volume") ?? DEFAULT_VOLUME);
  return Number.isFinite(value) ? value : DEFAULT_VOLUME;
}

export interface SettingsHandlers {
  /** The touch preference in use, and the one Save picks. */
  touchMode(): string;
  setTouchMode(mode: string): void;
  setVolume(value: number): void;
  /** Single player tunes its own battle's speeds; a room plays the server's. */
  speeds?: {
    get(): HudState["speedTuning"] | undefined;
    set(key: SpeedKey, value: number): void;
  };
  /** Runs before the dialog shows and after it closes, so the battle can wait for it. */
  opened(): void;
  closed(): void;
}

const percent = (value: number) => `${Math.round(value * 100)}%`;

/** Where a slider's default sits along its track, for the tick that marks it. */
function defaultMark(min: number, max: number, value: number): string {
  return `--default:${(((value - min) / (max - min)) * 100).toFixed(1)}%`;
}

function settingsMarkup(eyebrow: string, speeds: boolean): string {
  const speedRows = (Object.keys(SPEED_LABELS) as SpeedKey[])
    .map(
      (key) =>
        `<label class="setting-row" for="${key}"><span>${SPEED_LABELS[key]}<small>Default 100%</small></span><input id="${key}" type="range" min="0.5" max="2" step="0.05" value="1" style="${defaultMark(0.5, 2, 1)}"><output id="${key}-value" for="${key}">100%</output></label>`,
    )
    .join("");
  // The dialog itself takes focus when it opens, so no control starts out ringed as if
  // the keyboard had chosen it; Tab moves to the first one.
  return `<dialog id="settings" class="settings" aria-labelledby="settings-title" tabindex="-1">
    <header class="dialog-head"><div class="dialog-eyebrow"><span class="eyebrow">${eyebrow}</span></div><div class="dialog-title"><h2 id="settings-title">SETTINGS</h2></div></header>
    <div class="setting-group-label">GAME</div>
    <div class="setting-rows">
    <div class="setting-row"><span id="touch-mode-label">Touch controls<small>Auto shows them on touchscreens</small></span><div class="setting-choice" role="radiogroup" aria-labelledby="touch-mode-label">${TOUCH_MODES.map(([value, label]) => `<label><input type="radio" name="touch-mode-choice" value="${value}"><span>${label}</span></label>`).join("")}<select id="touch-mode" class="sr-only" tabindex="-1" aria-hidden="true">${TOUCH_MODES.map(([value, label]) => `<option value="${value}">${label}</option>`).join("")}</select></div></div>
    <label class="setting-row" for="volume"><span>Sound</span><input id="volume" type="range" min="0" max="1" step="0.05" style="${defaultMark(0, 1, DEFAULT_VOLUME)}"><output id="volume-value" for="volume"></output></label>
    ${speeds ? `<div class="setting-group-label">BATTLE SPEED<button class="chip-button speeds-reset" type="button" title="Both speeds back to 100%">RESET</button></div>${speedRows}` : ""}
    </div>
    <div class="menu-actions"><button class="primary settings-save" type="button">SAVE</button><button class="secondary settings-cancel" type="button">CANCEL</button></div>
  </dialog>`;
}

/** Touch controls, sound and (single player) battle speeds, in a dialog the HUD's corner
 * button opens during play. Nothing changes until Save; Cancel, Esc or a click outside
 * leave every setting as it was. Keys pressed in it never reach the game. */
export class SettingsDialog {
  private readonly dialog: HTMLDialogElement;
  private readonly opener: HTMLElement | null;

  constructor(
    root: HTMLElement,
    /** The line above the title, such as the mode the settings apply to. */
    eyebrow: string,
    private readonly handlers: SettingsHandlers,
  ) {
    root.insertAdjacentHTML("beforeend", settingsMarkup(eyebrow, !!handlers.speeds));
    this.dialog = root.querySelector<HTMLDialogElement>("#settings")!;
    this.opener = root.querySelector<HTMLElement>("#settings-open");
    this.opener?.addEventListener("click", () => this.open());
    for (const type of ["keydown", "keyup"]) {
      this.dialog.addEventListener(type, (event) => event.stopPropagation());
    }
    // Every way out closes through `close`, so the battle resumes at once; the dialog's
    // own close event would only arrive in a later task.
    this.dialog.addEventListener("cancel", (event) => {
      event.preventDefault();
      this.close();
    });
    // A click on the backdrop lands on the dialog itself.
    this.dialog.addEventListener("click", (event) => {
      if (event.target === this.dialog) {
        this.close();
      }
    });
    this.dialog.querySelector(".settings-cancel")!.addEventListener("click", () => this.close());
    this.dialog.querySelector(".settings-save")!.addEventListener("click", () => {
      this.save();
      this.close();
    });
    this.dialog.querySelector(".speeds-reset")?.addEventListener("click", () => {
      for (const key of Object.keys(SPEED_LABELS)) {
        this.show(key, 1);
      }
    });
    const touchMode = this.dialog.querySelector<HTMLSelectElement>("#touch-mode")!;
    touchMode.addEventListener("change", () => this.showTouchMode());
    for (const choice of this.dialog.querySelectorAll<HTMLInputElement>(
      'input[name="touch-mode-choice"]',
    )) {
      choice.addEventListener("change", () => {
        touchMode.value = choice.value;
      });
    }
    for (const id of ["volume", ...Object.keys(SPEED_LABELS)]) {
      const input = this.dialog.querySelector<HTMLInputElement>("#" + id);
      input?.addEventListener("input", () => this.show(id, Number(input.value)));
    }
  }

  /** The segmented touch choice follows the select's value. */
  private showTouchMode(): void {
    const value = this.value("touch-mode");
    for (const choice of this.dialog.querySelectorAll<HTMLInputElement>(
      'input[name="touch-mode-choice"]',
    )) {
      choice.checked = choice.value === value;
    }
  }

  private value(id: string): string {
    return this.dialog.querySelector<HTMLInputElement | HTMLSelectElement>("#" + id)!.value;
  }

  private save(): void {
    this.handlers.setTouchMode(this.value("touch-mode"));
    this.handlers.setVolume(Number(this.value("volume")));
    for (const key of Object.keys(SPEED_LABELS) as SpeedKey[]) {
      this.handlers.speeds?.set(key, Number(this.value(key)));
    }
  }

  private show(id: string, value: number): void {
    const input = this.dialog.querySelector<HTMLInputElement>("#" + id);
    if (input) {
      input.value = String(value);
      this.dialog.querySelector(`#${id}-value`)!.textContent = percent(value);
      // The track fills up to the thumb in the brand gold (`--fill`, 0–100%).
      const [min, max] = [Number(input.min), Number(input.max)];
      input.style.setProperty("--fill", `${((value - min) / (max - min)) * 100}%`);
    }
  }

  close(): void {
    if (!this.dialog.open) {
      return;
    }
    this.dialog.close();
    // Closing returns focus to the corner button; keys typed during play must not press
    // it again.
    this.opener?.blur();
    this.handlers.closed();
  }

  open(): void {
    if (this.dialog.open) {
      return;
    }
    this.dialog.querySelector<HTMLSelectElement>("#touch-mode")!.value = this.handlers.touchMode();
    this.showTouchMode();
    this.show("volume", savedVolume());
    const speeds = this.handlers.speeds?.get();
    for (const key of Object.keys(SPEED_LABELS) as SpeedKey[]) {
      if (speeds) {
        this.show(key, speeds[key]);
      }
    }
    this.handlers.opened();
    this.dialog.showModal();
    this.dialog.focus();
  }
}
