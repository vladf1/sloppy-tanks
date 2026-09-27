import { bindPress } from "./button-input";

/** The first-person frame over the arena: a periscope vignette, a compass that
 * shows where the hull points relative to the turret, and the HUD toggle. */
export class Cockpit {
  private readonly hud: HTMLElement;
  private readonly toggle: HTMLButtonElement;
  private readonly hull: HTMLElement;
  private shown = false;
  private hullAngle = NaN;

  constructor(root: HTMLElement, toggleView: () => void) {
    this.hud = root.querySelector("#hud")!;
    this.toggle = root.querySelector("#view-mode")!;
    this.hull = root.querySelector("#cockpit .hull")!;
    bindPress(this.toggle, () => {
      toggleView();
      // Keyboard play continues on the arena, not the button.
      this.toggle.blur();
    });
  }

  /** `hullAngle` is the hull's clockwise screen angle, with the turret straight up. */
  update(shown: boolean, hullAngle: number): void {
    if (shown !== this.shown) {
      this.shown = shown;
      this.hud.classList.toggle("first-person", shown);
      this.toggle.setAttribute("aria-pressed", String(shown));
    }
    // Only the compass changes per frame; skip style writes for sub-degree turns.
    const rounded = Math.round((hullAngle * 180) / Math.PI);
    if (shown && rounded !== this.hullAngle) {
      this.hullAngle = rounded;
      this.hull.style.transform = `rotate(${rounded}deg)`;
    }
  }
}
