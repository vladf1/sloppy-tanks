import type { Controls } from "./controls";
import type { Match, Tank } from "./types";
import type { TouchControls } from "./touch-controls";
import type { TouchMode } from "./touch-input";
export interface TouchState {
  readonly human: Pick<Tank, "mineCooldown">;
  readonly match: Pick<Match, "phase">;
}

/** Desktop retains only detection/settings; joystick code and CSS load on first enable. */
export class TouchModeController {
  private mode: TouchMode;
  private detected = navigator.maxTouchPoints > 0 || matchMedia("(pointer: coarse)").matches;
  private enabled = false;
  private view: TouchControls | undefined;
  private loading = false;
  private lastPhase = "";

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
    root.addEventListener("change", (event) => {
      const target = event.target;
      if (!(target instanceof HTMLSelectElement) || target.id !== "touch-mode") {
        return;
      }
      if (target.value !== "auto" && target.value !== "on" && target.value !== "off") {
        return;
      }
      this.mode = target.value;
      try {
        localStorage.setItem("sloppy-touch", this.mode);
      } catch {
        /* Session-only preference. */
      }
      this.controls.clear();
      this.applyMode();
    });
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
            toast.textContent =
              "Touch controls could not load. Toggle them On in the pause menu to retry.";
            toast.classList.add("visible");
          }
        })
        .finally(() => {
          this.loading = false;
        });
    }
  }

  update(): void {
    const phase = this.simulation.match.phase;
    // UI rebuilds the pause menu only on phase transitions. Do not query DOM every frame.
    if (phase !== this.lastPhase) {
      this.lastPhase = phase;
      if (phase === "paused") {
        const selector = this.root.querySelector<HTMLSelectElement>("#touch-mode");
        if (selector) {
          selector.value = this.mode;
        }
      }
    }
    if (this.enabled) {
      this.view?.update();
    }
  }
}
