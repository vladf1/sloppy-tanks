import type { Controls } from "./controls";
import type { Phase } from "./engine-api";
import type { TouchControls } from "./touch-controls";
import type { TouchMode } from "./touch-input";
/** What the touch overlay shows: the mine button's cooldown and whether to show at all. */
export interface TouchState {
  readonly human: { readonly mineCooldown: number };
  readonly match: { readonly phase: Phase };
}

/** Desktop retains only detection/settings; joystick code and CSS load on first enable. */
export class TouchModeController {
  private mode: TouchMode;
  private detected = navigator.maxTouchPoints > 0 || matchMedia("(pointer: coarse)").matches;
  private enabled = false;
  private view: TouchControls | undefined;
  private loading = false;

  constructor(
    private readonly root: HTMLElement,
    private readonly controls: Controls,
    private readonly simulation: TouchState,
    private readonly zoom: (amount: number) => void,
  ) {
    let saved: string | null = null;
    try {
      saved = localStorage.getItem("sloppy-touch");
    } catch {
      /* Settings remain usable without storage. */
    }
    this.mode = saved === "on" || saved === "off" ? saved : "auto";
    this.applyMode();
  }

  /** Settings saved a preference: "auto", "on" or "off". */
  setPreference(mode: string): void {
    if ((mode !== "auto" && mode !== "on" && mode !== "off") || mode === this.mode) {
      return;
    }
    this.mode = mode;
    try {
      localStorage.setItem("sloppy-touch", this.mode);
    } catch {
      /* Session-only preference. */
    }
    this.controls.clear();
    this.applyMode();
  }

  private readonly detectTouch = (event: PointerEvent) => {
    if (event.pointerType !== "touch") {
      return;
    }
    this.detected = true;
    this.applyMode();
  };

  private applyMode(): void {
    this.enabled = this.mode === "on" || (this.mode === "auto" && this.detected);
    // No global pointer listener is needed when Off or after touch is already known.
    window.removeEventListener("pointerdown", this.detectTouch, true);
    if (this.mode === "auto" && !this.detected) {
      window.addEventListener("pointerdown", this.detectTouch, true);
    }
    if (this.view) {
      this.view.setEnabled(this.enabled);
    } else if (this.enabled && !this.loading) {
      this.loading = true;
      void import("./touch-controls")
        .then(({ TouchControls }) => {
          // A user can select Off while the module is still downloading.
          if (!this.enabled) {
            return;
          }
          this.view = new TouchControls(this.root, this.controls, this.simulation, this.zoom);
          this.view.setEnabled(true);
        })
        .catch((error: unknown) => {
          console.error("Touch controls could not load", error);
          const toast = this.root.querySelector<HTMLElement>("#toast");
          if (toast) {
            toast.textContent = "Touch controls could not load. Turn them On in Settings to retry.";
            toast.classList.add("visible");
          }
        })
        .finally(() => {
          this.loading = false;
        });
    }
  }

  /** The saved preference: "auto", "on" or "off". */
  get preference(): TouchMode {
    return this.mode;
  }

  /** Call after each HUD read: the touch overlay follows the HUD state. */
  update(): void {
    if (this.enabled) {
      this.view?.update();
    }
  }
}
