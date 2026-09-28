import { TouchInput } from "./touch-input";
import { AMMO_ORDER, AMMO_SCROLL_INTERVAL_MS } from "./ammunition";
import type { AmmoSelection, VehicleCommand } from "./types";
const PRIMARY_BUTTON = 0;
const SECONDARY_BUTTON = 2;
const ZOOM_STEP = 2;
/** Browsers may release a captured pointer on Esc just before delivering the key;
 * a key this soon after the release belongs to the same press. */
const ESC_RELEASE_WINDOW_MS = 250;

const ammoKeys = new Map<string, AmmoSelection>([
  ["KeyQ", -1],
  ["KeyE", 1],
]);
AMMO_ORDER.forEach((weapon, i) => {
  ammoKeys.set(`Digit${i + 1}`, weapon);
  ammoKeys.set(`Numpad${i + 1}`, weapon);
});
export class Controls {
  readonly touch = new TouchInput();
  keys = new Set<string>();
  fire = false;
  mine = false;
  // Normalized device coordinates: -1..1, with positive Y toward the top of the canvas.
  nx = 0;
  ny = 0;
  ammoSelection: AmmoSelection | undefined;
  lastAmmoScroll = -Infinity;
  /** Horizontal mouse travel in pixels since the last `takeLook()`, for first person. */
  look = 0;
  /** Called on V; the owner decides whether the view may change. */
  toggleView = () => {};
  /** While first person steers, clicks on the arena capture the pointer. */
  private pointerWanted = false;
  /** Set once the browser grants a lock; until then mouse look works uncaptured. */
  private lockWorks = false;
  private lockReleasedAt = -Infinity;
  constructor(
    private readonly canvas: HTMLCanvasElement,
    public pause: () => void,
    zoom: (amount: number) => void,
    public active: () => boolean = () => true,
    pauseWhenHidden = true,
  ) {
    window.addEventListener("keydown", (e) => {
      if (e.code === "Escape") {
        // The browser spends Esc on releasing a captured pointer, so in first person
        // the first press only frees the cursor; Esc with a free cursor opens the menu.
        if (
          document.pointerLockElement === canvas ||
          performance.now() - this.lockReleasedAt < ESC_RELEASE_WINDOW_MS
        ) {
          this.releasePointer();
          return;
        }
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
      if (e.code === "KeyV") {
        if (!e.repeat) {
          this.toggleView();
        }
        return;
      }
      if (e.code === "Space" && target?.tagName === "BUTTON") {
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
      if (e.pointerType === "touch") {
        return;
      }
      this.touch.aiming = false;
      // A freed cursor can reach the HUD buttons without spinning the view.
      if (!this.aimWaitsForClick) {
        this.look += e.movementX ?? 0;
      }
      const r = canvas.getBoundingClientRect();
      this.nx = ((e.clientX - r.left) / r.width) * 2 - 1;
      this.ny = 1 - ((e.clientY - r.top) / r.height) * 2;
    });
    canvas.addEventListener("pointerdown", (e) => {
      if (e.pointerType === "touch" || !this.active()) {
        return;
      }
      this.touch.aiming = false;
      canvas.focus();
      // The click that takes the pointer back only aims; it does not fire.
      if (this.aimWaitsForClick) {
        this.capturePointer();
        return;
      }
      if (e.button === PRIMARY_BUTTON) {
        this.fire = true;
      }
      if (e.button === SECONDARY_BUTTON) {
        this.mine = true;
      }
      this.capturePointer();
    });
    window.addEventListener("pointerup", (e) => {
      if (e.pointerType === "touch") {
        return;
      }
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
      // Blur also fires when the user clicks browser chrome or another visible
      // window. Release held input, but keep the round running in those cases.
      this.clear();
    });
    document.addEventListener("pointerlockchange", () => {
      if (document.pointerLockElement === canvas) {
        this.lockWorks = true;
        return;
      }
      // Esc, a window switch or a menu freed the cursor, sometimes without a keydown
      // reaching the page. The round keeps running; aiming waits for a click.
      this.lockReleasedAt = performance.now();
      this.fire = false;
      this.look = 0;
    });
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) {
        this.clear();
        if (pauseWhenHidden) {
          pause();
        }
      }
    });
  }
  /** First person wants raw mouse motion while its input steers. A death keeps the
   * pointer captured, because browsers only re-capture after a click and show
   * their lock notice again; Esc frees it to pick another tank. Leaving first
   * person or opening a menu releases it for the menu's buttons. */
  holdPointer(firstPerson: boolean, menuOpen = false): void {
    this.pointerWanted = firstPerson && !menuOpen && this.active();
    if (!firstPerson || menuOpen) {
      this.releasePointer();
    }
  }
  /** First person steers with a free cursor where the browser grants locks: the
   * view holds still and the next arena click captures the pointer. */
  get aimWaitsForClick(): boolean {
    return this.pointerWanted && this.lockWorks && document.pointerLockElement !== this.canvas;
  }
  private releasePointer(): void {
    if (document.pointerLockElement === this.canvas) {
      document.exitPointerLock();
    }
  }
  /** Needs a user gesture: a click on the arena or the key that entered first person. */
  capturePointer(): void {
    if (this.pointerWanted && document.pointerLockElement !== this.canvas) {
      // Unsupported or refused locks still turn with ordinary mouse motion.
      Promise.resolve(this.canvas.requestPointerLock?.()).catch(() => {});
    }
  }
  takeLook(): number {
    const look = this.look;
    this.look = 0;
    return look;
  }
  clear(): void {
    this.look = 0;
    this.touch.clear();
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
          Number(this.keys.has("KeyA") || this.keys.has("ArrowLeft")) || this.touch.moveX,
      moveZ:
        Number(this.keys.has("KeyS") || this.keys.has("ArrowDown")) -
          Number(this.keys.has("KeyW") || this.keys.has("ArrowUp")) || this.touch.moveZ,
      aim,
      fire: this.fire || this.touch.fire,
      mine,
      ammoSelection,
    };
  }
}
