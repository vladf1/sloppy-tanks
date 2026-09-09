import { AMMO_ORDER, AMMO_SCROLL_INTERVAL_MS } from "./ammunition";
import type { AmmoSelection, VehicleCommand } from "./types";
const PRIMARY_BUTTON = 0;
const SECONDARY_BUTTON = 2;
const ZOOM_STEP = 2;

const ammoKeys = new Map<string, AmmoSelection>([
  ["KeyQ", -1],
  ["KeyE", 1],
]);
AMMO_ORDER.forEach((weapon, i) => {
  ammoKeys.set(`Digit${i + 1}`, weapon);
  ammoKeys.set(`Numpad${i + 1}`, weapon);
});
export class Controls {
  keys = new Set<string>();
  fire = false;
  mine = false;
  // Normalized device coordinates: -1..1, with positive Y toward the top of the canvas.
  nx = 0;
  ny = 0;
  ammoSelection: AmmoSelection | undefined;
  lastAmmoScroll = -Infinity;
  constructor(
    canvas: HTMLCanvasElement,
    public pause: () => void,
    zoom: (amount: number) => void,
    public active: () => boolean = () => true,
  ) {
    window.addEventListener("keydown", (e) => {
      if (e.code === "Escape") {
        this.clear();
        pause();
        return;
      }
      const target = e.target as HTMLElement | null;
      if (
        target?.isContentEditable ||
        ["INPUT", "TEXTAREA", "SELECT"].includes(target?.tagName ?? "") ||
        e.metaKey ||
        e.ctrlKey ||
        e.altKey
      ) {
        return;
      }
      const selection = ammoKeys.get(e.code);
      if (selection !== undefined) {
        if (this.active()) {
          e.preventDefault();
          if (!e.repeat) {
            this.ammoSelection = selection;
          }
        }
        return;
      }
      if (
        [
          "KeyW",
          "KeyA",
          "KeyS",
          "KeyD",
          "ArrowUp",
          "ArrowLeft",
          "ArrowDown",
          "ArrowRight",
          "Space",
        ].includes(e.code)
      ) {
        e.preventDefault();
        this.keys.add(e.code);
      }
    });
    window.addEventListener("keyup", (e) => this.keys.delete(e.code));
    canvas.addEventListener("pointermove", (e) => {
      const r = canvas.getBoundingClientRect();
      this.nx = ((e.clientX - r.left) / r.width) * 2 - 1;
      this.ny = 1 - ((e.clientY - r.top) / r.height) * 2;
    });
    canvas.addEventListener("pointerdown", (e) => {
      if (e.button === PRIMARY_BUTTON) {
        this.fire = true;
      }
      if (e.button === SECONDARY_BUTTON) {
        this.mine = true;
      }
      canvas.focus();
    });
    window.addEventListener("pointerup", (e) => {
      if (e.button === PRIMARY_BUTTON) {
        this.fire = false;
      }
    });
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());
    canvas.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        if (e.shiftKey) {
          // Shift-wheel can arrive as horizontal scrolling on desktop browsers.
          const delta = e.deltaY || e.deltaX;
          if (delta) {
            zoom(Math.sign(delta) * ZOOM_STEP);
          }
        } else if (
          e.deltaY &&
          this.active() &&
          performance.now() - this.lastAmmoScroll >= AMMO_SCROLL_INTERVAL_MS
        ) {
          this.ammoSelection = e.deltaY > 0 ? 1 : -1;
          this.lastAmmoScroll = performance.now();
        }
      },
      { passive: false },
    );
    window.addEventListener("blur", () => {
      this.clear();
      pause();
    });
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) {
        this.clear();
        pause();
      }
    });
  }
  clear(): void {
    this.keys.clear();
    this.fire = false;
    this.mine = false;
    this.ammoSelection = undefined;
    this.lastAmmoScroll = -Infinity;
  }
  /** Consume queued actions once per physics tick; continuous movement/fire stay held. */
  command(aim: number): VehicleCommand {
    const mine = this.mine;
    const ammoSelection = this.active() ? this.ammoSelection : undefined;
    this.ammoSelection = undefined;
    this.mine = false;
    return {
      moveX:
        Number(this.keys.has("KeyD") || this.keys.has("ArrowRight")) -
        Number(this.keys.has("KeyA") || this.keys.has("ArrowLeft")),
      moveZ:
        Number(this.keys.has("KeyS") || this.keys.has("ArrowDown")) -
        Number(this.keys.has("KeyW") || this.keys.has("ArrowUp")),
      aim,
      fire: this.fire,
      mine,
      ammoSelection,
    };
  }
}
