import type { HudState } from "./engine-api";

type SpeedKey = keyof HudState["speedTuning"];

const SPEED_LABELS: Record<SpeedKey, string> = {
  "tank-speed": "Tank base speed",
  "bullet-speed": "Projectile base speed",
};
const DEFAULT_VOLUME = 0.6;

/** The HUD's corner button that opens the dialog. */
export const SETTINGS_BUTTON = `<button id="settings-open" class="quiet" type="button" aria-label="Settings" title="Settings" aria-haspopup="dialog"><svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M10.23 4.61L10.8 1.87h2.4l.57 2.74 2.2.91 2.34-1.53 1.7 1.7-1.53 2.34.91 2.2 2.74.57v2.4l-2.74.57-.91 2.2 1.53 2.34-1.7 1.7-2.34-1.53-2.2.91-.57 2.74h-2.4l-.57-2.74-2.2-.91-2.34 1.53-1.7-1.7 1.53-2.34-.91-2.2-2.74-.57v-2.4l2.74-.57.91-2.2-1.53-2.34 1.7-1.7 2.34 1.53z" /><circle cx="12" cy="12" r="3.2" /></svg></button>`;

/** The saved sound level, shared by single player and rooms. */
export function savedVolume(): number {
  try {
    const value = Number(localStorage.getItem("sloppy-volume") ?? DEFAULT_VOLUME);
    return Number.isFinite(value) ? value : DEFAULT_VOLUME;
  } catch {
    return DEFAULT_VOLUME;
  }
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

function settingsMarkup(eyebrow: string, speeds: boolean): string {
  const speedRows = (Object.keys(SPEED_LABELS) as SpeedKey[])
    .map(
      (key) =>
        `<label class="setting-row" for="${key}"><span>${SPEED_LABELS[key]}</span><input id="${key}" type="range" min="0.5" max="2" step="0.05" value="1"><output id="${key}-value" for="${key}">100%</output></label>`,
    )
    .join("");
  return `<dialog id="settings" class="settings" aria-labelledby="settings-title">
    <header class="dialog-head"><div class="dialog-eyebrow"><span class="eyebrow">${eyebrow}</span></div><div class="dialog-title"><h2 id="settings-title">SETTINGS</h2></div></header>
    <div class="setting-rows">
    <label class="setting-row" for="touch-mode"><span>Touch controls</span><select id="touch-mode"><option value="auto">Auto</option><option value="on">On</option><option value="off">Off</option></select></label>
    <label class="setting-row" for="volume"><span>Sound</span><input id="volume" type="range" min="0" max="1" step="0.05"><output id="volume-value" for="volume"></output></label>
    ${speeds ? `<div class="setting-group-label">BATTLE SPEED<small>50–200% · 100% is the default</small></div>${speedRows}` : ""}
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
    for (const id of ["volume", ...Object.keys(SPEED_LABELS)]) {
      const input = this.dialog.querySelector<HTMLInputElement>("#" + id);
      input?.addEventListener("input", () => this.show(id, Number(input.value)));
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
    this.show("volume", savedVolume());
    const speeds = this.handlers.speeds?.get();
    for (const key of Object.keys(SPEED_LABELS) as SpeedKey[]) {
      if (speeds) {
        this.show(key, speeds[key]);
      }
    }
    this.handlers.opened();
    this.dialog.showModal();
  }
}
