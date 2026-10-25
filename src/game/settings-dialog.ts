import type { HudState } from "./engine-api";
import { GRAPHICS_SETTINGS, savedPreference } from "./player-preferences";

type SpeedKey = keyof HudState["speedTuning"];

const SPEED_LABELS: Record<SpeedKey, string> = {
  "tank-speed": "Tank base speed",
  "bullet-speed": "Projectile base speed",
};
/** What each speed changes, under its name. */
const SPEED_NOTES: Record<SpeedKey, string> = {
  "tank-speed": "How fast every tank drives",
  "bullet-speed": "How fast shells and rockets fly",
};
const DEFAULT_VOLUME = 0.6;
/** The touch preference's choices. */
const TOUCH_MODES = [
  ["auto", "Auto"],
  ["on", "On"],
  ["off", "Off"],
] as const;
/** The settings offered as a few named choices, each a segmented control; the hidden
 * select behind it holds the value Save reads. */
const CHOICE_IDS = ["touch-mode", "graphics"] as const;
type ChoiceId = (typeof CHOICE_IDS)[number];

/** The HUD's corner button that opens the dialog. */
export const SETTINGS_BUTTON = `<button id="settings-open" class="quiet" type="button" aria-label="Settings" title="Settings" aria-haspopup="dialog"><svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M10.23 4.61L10.8 1.87h2.4l.57 2.74 2.2.91 2.34-1.53 1.7 1.7-1.53 2.34.91 2.2 2.74.57v2.4l-2.74.57-.91 2.2 1.53 2.34-1.7 1.7-2.34-1.53-2.2.91-.57 2.74h-2.4l-.57-2.74-2.2-.91-2.34 1.53-1.7-1.7 1.53-2.34-.91-2.2-2.74-.57v-2.4l2.74-.57.91-2.2-1.53-2.34 1.7-1.7 2.34 1.53z" /><circle cx="12" cy="12" r="3.2" /></svg></button>`;

/** Wires every `.dialog-tabs` tab list under `root`: a click or the arrow keys pick a
 * tab, which shows its panel (`aria-controls`) and hides the others. */
export function bindDialogTabs(root: ParentNode): void {
  for (const list of root.querySelectorAll<HTMLElement>(".dialog-tabs")) {
    const tabs = [...list.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
    const select = (chosen: HTMLButtonElement) => {
      for (const tab of tabs) {
        const selected = tab === chosen;
        tab.setAttribute("aria-selected", String(selected));
        tab.tabIndex = selected ? 0 : -1;
        const panel = document.getElementById(tab.getAttribute("aria-controls") ?? "");
        if (panel) {
          panel.hidden = !selected;
        }
      }
    };
    tabs.forEach((tab, index) => {
      tab.addEventListener("click", () => select(tab));
      tab.addEventListener("keydown", (event) => {
        const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
        if (step) {
          event.preventDefault();
          const next = tabs[(index + step + tabs.length) % tabs.length];
          select(next);
          next.focus();
        }
      });
    });
  }
}

/** The saved sound level, shared by single player and rooms. */
export function savedVolume(): number {
  const value = Number(savedPreference("volume") ?? DEFAULT_VOLUME);
  return Number.isFinite(value) ? value : DEFAULT_VOLUME;
}

export interface SettingsHandlers {
  /** The touch preference in use, and the one Save picks. */
  touchMode(): string;
  setTouchMode(mode: string): void;
  /** The Graphics setting in use, and the one Save picks. */
  graphics(): string;
  setGraphics(setting: string): void;
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
  const ratio = (value - min) / (max - min);
  return `--default:${(ratio * 100).toFixed(1)}%;--default-ratio:${ratio.toFixed(3)}`;
}

/** A setting row of named choices: a segmented control over the hidden select whose value
 * Save reads. */
function choiceRow(
  id: ChoiceId,
  label: string,
  note: string,
  choices: readonly (readonly [string, string])[],
): string {
  return `<div class="setting-row"><span id="${id}-label">${label}<small>${note}</small></span><div class="setting-choice" role="radiogroup" aria-labelledby="${id}-label">${choices.map(([value, text]) => `<label><input type="radio" name="${id}-choice" value="${value}"><span>${text}</span></label>`).join("")}<select id="${id}" class="sr-only" tabindex="-1" aria-hidden="true">${choices.map(([value, text]) => `<option value="${value}">${text}</option>`).join("")}</select></div></div>`;
}

/** The scale under a slider: what its ends mean in words. The value reads once, in the
 * box beside the slider, and the default is a notch on the track itself. */
function sliderScale(low: string, high: string, mark: string): string {
  return `<span class="slider-scale" style="${mark}" aria-hidden="true"><i>${low}</i><i>${high}</i></span>`;
}

/** The corner X that every dialog with a way back carries. */
export const CLOSE_ICON = `<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M6.4 5 12 10.6 17.6 5 19 6.4 13.4 12l5.6 5.6-1.4 1.4-5.6-5.6L6.4 19 5 17.6l5.6-5.6L5 6.4z"/></svg>`;

function settingsMarkup(eyebrow: string, speeds: boolean): string {
  const speedRows = (Object.keys(SPEED_LABELS) as SpeedKey[])
    .map(
      (key) =>
        `<label class="setting-row slider-row" for="${key}"><span>${SPEED_LABELS[key]}<small>${SPEED_NOTES[key]}</small></span><input id="${key}" type="range" min="0.5" max="2" step="0.05" value="1" style="${defaultMark(0.5, 2, 1)}" title="Default 100%"><output id="${key}-value" for="${key}">100%</output>${sliderScale("Slower", "Faster", defaultMark(0.5, 2, 1))}</label>`,
    )
    .join("");
  // The dialog itself takes focus when it opens, so no control starts out ringed as if
  // the keyboard had chosen it; Tab moves to the first one.
  return `<dialog id="settings" class="settings" aria-labelledby="settings-title" tabindex="-1">
    <button class="dialog-close settings-close" type="button" aria-label="Close settings without saving" title="Close (Esc)">${CLOSE_ICON}</button>
    <header class="dialog-head"><div class="dialog-eyebrow"><span class="eyebrow">${eyebrow}</span></div><div class="dialog-title"><h2 id="settings-title">SETTINGS</h2></div></header>
    <div class="dialog-pane setting-rows">
    <div class="setting-group-label"><span>SOUND &amp; INPUT</span></div>
    <label class="setting-row slider-row" for="volume"><span>Sound<small>Effects and engine noise</small></span><input id="volume" type="range" min="0" max="1" step="0.05" style="${defaultMark(0, 1, DEFAULT_VOLUME)}"><output id="volume-value" for="volume"></output>${sliderScale("Mute", "Loud", defaultMark(0, 1, DEFAULT_VOLUME))}</label>
    ${choiceRow("touch-mode", "Touch controls", "Auto shows them on touchscreens", TOUCH_MODES)}
    <div class="setting-group-label"><span>DISPLAY</span></div>
    ${choiceRow("graphics", "Graphics", "Auto keeps the frame rate smooth", GRAPHICS_SETTINGS)}
    ${speeds ? `<div class="setting-group-label"><span>BATTLE SPEED</span><button class="group-action speeds-reset" type="button" title="Both speeds back to 100%"><svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M4 12a8 8 0 1 0 2.4-5.7M4 4v4.5h4.5" /></svg>Reset to 100%</button></div>${speedRows}` : ""}
    </div>
    <div class="menu-actions"><button class="primary settings-save" type="button">SAVE</button><button class="secondary settings-cancel" type="button">CANCEL</button></div>
  </dialog>`;
}

/** Touch controls, sound, graphics and (single player) battle speeds, in a dialog the
 * HUD's corner button opens during play. Nothing changes until Save; Cancel, Esc or a click outside
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
    for (const exit of this.dialog.querySelectorAll(".settings-cancel, .settings-close")) {
      exit.addEventListener("click", () => this.close());
    }
    this.dialog.querySelector(".settings-save")!.addEventListener("click", () => {
      this.save();
      this.close();
    });
    this.dialog.querySelector(".speeds-reset")?.addEventListener("click", () => {
      for (const key of Object.keys(SPEED_LABELS)) {
        this.show(key, 1);
      }
    });
    for (const id of CHOICE_IDS) {
      const select = this.dialog.querySelector<HTMLSelectElement>("#" + id)!;
      select.addEventListener("change", () => this.showChoice(id));
      for (const choice of this.dialog.querySelectorAll<HTMLInputElement>(
        `input[name="${id}-choice"]`,
      )) {
        choice.addEventListener("change", () => {
          select.value = choice.value;
        });
      }
    }
    for (const id of ["volume", ...Object.keys(SPEED_LABELS)]) {
      const input = this.dialog.querySelector<HTMLInputElement>("#" + id);
      input?.addEventListener("input", () => this.show(id, Number(input.value)));
    }
  }

  /** A segmented choice follows its select's value. */
  private showChoice(id: ChoiceId): void {
    const value = this.value(id);
    for (const choice of this.dialog.querySelectorAll<HTMLInputElement>(
      `input[name="${id}-choice"]`,
    )) {
      choice.checked = choice.value === value;
    }
  }

  /** Put a choice's select at `value` and show it. */
  private choose(id: ChoiceId, value: string): void {
    this.dialog.querySelector<HTMLSelectElement>("#" + id)!.value = value;
    this.showChoice(id);
  }

  private value(id: string): string {
    return this.dialog.querySelector<HTMLInputElement | HTMLSelectElement>("#" + id)!.value;
  }

  private save(): void {
    this.handlers.setTouchMode(this.value("touch-mode"));
    this.handlers.setGraphics(this.value("graphics"));
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
    this.showResetState();
  }

  /** Reset is offered only while a speed differs from its default. */
  private showResetState(): void {
    const reset = this.dialog.querySelector<HTMLButtonElement>(".speeds-reset");
    if (reset) {
      reset.disabled = Object.keys(SPEED_LABELS).every((key) => Number(this.value(key)) === 1);
      // A disabled control says why it is out of reach.
      reset.title = reset.disabled
        ? "Both speeds are at their 100% default"
        : "Both speeds back to 100%";
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
    this.choose("touch-mode", this.handlers.touchMode());
    this.choose("graphics", this.handlers.graphics());
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
