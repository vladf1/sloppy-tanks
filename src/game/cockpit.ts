import { bindPress } from "./button-input";

/** The first-person frame over the arena: a periscope vignette, a compass that
 * shows where the hull points relative to the turret, and the HUD toggle. */
export class Cockpit {
  private readonly hud: HTMLElement;
  private readonly toggle: HTMLButtonElement;
  private readonly hull: HTMLElement;
  private readonly aimHint: HTMLElement;
  private shown = false;
  private hullAngle = NaN;

  constructor(root: HTMLElement, toggleView: () => void) {
    this.hud = root.querySelector("#hud")!;
    this.toggle = root.querySelector("#view-mode")!;
    this.hull = root.querySelector("#cockpit .hull")!;
    this.aimHint = root.querySelector("#cockpit .aim-hint")!;
    bindPress(this.toggle, () => {
      toggleView();
      // Keyboard play continues on the arena, not the button.
      this.toggle.blur();
    });
  }

  /** `hullAngle` is the hull's clockwise screen angle, with the turret straight up;
   * `aimWaitsForClick` shows how to take the freed cursor back. */
  update(shown: boolean, hullAngle: number, aimWaitsForClick: boolean): void {
    if (shown !== this.shown) {
      this.shown = shown;
      this.hud.classList.toggle("first-person", shown);
      this.toggle.setAttribute("aria-pressed", String(shown));
    }
    if (this.aimHint.hidden === aimWaitsForClick) {
      this.aimHint.hidden = !aimWaitsForClick;
    }
    // Only the compass changes per frame; skip style writes for sub-degree turns.
    const rounded = Math.round((hullAngle * 180) / Math.PI);
    if (shown && rounded !== this.hullAngle) {
      this.hullAngle = rounded;
      this.hull.style.transform = `rotate(${rounded}deg)`;
    }
  }
}
