import { TouchInput } from "./touch-input";
import { AMMO_ORDER } from "./ammo-options";
import { INPUT, type Weapon } from "./engine-api";
const PRIMARY_BUTTON = 0;
const SECONDARY_BUTTON = 2;
const ZOOM_STEP = 2;
/** Browsers may release a captured pointer on Esc just before delivering the key;
 * a key this soon after the release belongs to the same press. */
const ESC_RELEASE_WINDOW_MS = 250;

/** A queued ammo choice: a weapon, or -1/+1 to cycle stocked ammunition. */
export type AmmoSelection = Weapon | -1 | 1;

const ammoKeys = new Map<string, AmmoSelection>([
  ["KeyQ", -1],
  ["KeyE", 1],
]);
AMMO_ORDER.forEach((weapon, i) => {
  ammoKeys.set(`Digit${i + 1}`, weapon);
  ammoKeys.set(`Numpad${i + 1}`, weapon);
});

const held = (keys: Set<string>, ...codes: string[]) =>
  codes.some((code) => keys.has(code)) ? 1 : 0;

/** Raw keyboard, mouse and touch state for the engine. Continuous input stays held;
 * one-shot presses (a mine, an ammo choice, a wheel step) queue until `takeInput`
 * hands them to the engine, which applies them on its next simulation tick. */
export class Controls {
  readonly touch = new TouchInput();
  keys = new Set<string>();
  fire = false;
  mine = false;
  // Normalized device coordinates: -1..1, with positive Y toward the top of the canvas.
  nx = 0;
  ny = 0;
  ammoSelection: AmmoSelection | undefined;
  /** Plain wheel since the last frame: -1 previous ammo, +1 next. */
  wheelAmmo = 0;
  /** Horizontal mouse travel in pixels since the last `takeLook()`, for first person. */
  look = 0;
  /** Touch controls in first person: the drive stick's sideways push turns the view, as
   * the engine's aim-stick turn would, and only its forward push drives (no strafing). */
  stickTurns = false;
  /** Called on V; the owner decides whether the view may change. */
  toggleView = () => {};
  /** While first person steers, clicks on the arena capture the pointer. */
  private pointerWanted = false;
  /** Set once the browser grants a lock; until then mouse look works uncaptured. */
  private lockWorks = false;
  /** When the browser, not the game, last released the pointer (Esc or a window switch). */
  private lockReleasedAt = -Infinity;
  /** The game asked for the pending release: leaving first person, a menu, or Esc itself. */
  private releaseRequested = false;
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
          // This press is spent; the next Esc opens the menu.
          this.lockReleasedAt = -Infinity;
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
      // A freed cursor can reach the HUD buttons without spinning the view.
      if (!this.aimWaitsForClick) {
        this.look += e.movementX ?? 0;
      }
      this.aimAt(e.clientX, e.clientY);
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
        } else if (e.deltaY && this.active()) {
          this.wheelAmmo = e.deltaY > 0 ? 1 : -1;
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
        this.releaseRequested = false;
        this.lockReleasedAt = -Infinity;
        return;
      }
      // Esc, a window switch or a menu freed the cursor. The round keeps running;
      // aiming waits for a click. Only a release the browser made on its own may
      // be followed by the same Esc press arriving as a key.
      if (!this.releaseRequested) {
        this.lockReleasedAt = performance.now();
      }
      this.releaseRequested = false;
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
      this.releaseRequested = true;
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
  /** Aim at a point on the canvas, as the mouse does (phones aim by touching it). */
  aimAt(clientX: number, clientY: number): void {
    this.touch.aiming = false;
    const r = this.canvas.getBoundingClientRect();
    this.nx = ((clientX - r.left) / r.width) * 2 - 1;
    this.ny = 1 - ((clientY - r.top) / r.height) * 2;
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
    this.wheelAmmo = 0;
  }
  /** Pack this frame's raw control state into `out` (see `INPUT`) and consume the
   * one-shot presses. Zoom and the view toggle belong to the caller. */
  takeInput(out: Float32Array): Float32Array {
    const touch = this.touch;
    out[INPUT.up] = held(this.keys, "KeyW", "ArrowUp");
    out[INPUT.down] = held(this.keys, "KeyS", "ArrowDown");
    out[INPUT.left] = held(this.keys, "KeyA", "ArrowLeft");
    out[INPUT.right] = held(this.keys, "KeyD", "ArrowRight");
    out[INPUT.touchMoveX] = touch.moveX;
    out[INPUT.touchMoveZ] = touch.moveZ;
    out[INPUT.fire] = this.fire || touch.fire ? 1 : 0;
    out[INPUT.mine] = this.mine ? 1 : 0;
    const selection = this.ammoSelection;
    out[INPUT.ammoSlot] =
      typeof selection === "string" ? AMMO_ORDER.indexOf(selection as never) + 1 : 0;
    out[INPUT.ammoStep] = typeof selection === "number" ? selection : 0;
    out[INPUT.wheelAmmo] = this.wheelAmmo;
    out[INPUT.pointerX] = this.nx;
    out[INPUT.pointerY] = this.ny;
    out[INPUT.touchAiming] = touch.aiming ? 1 : 0;
    out[INPUT.touchAimX] = touch.aimX;
    out[INPUT.touchAimY] = touch.aimY;
    out[INPUT.aimStickHeld] = touch.pointers.aim === null ? 0 : 1;
    out[INPUT.lookPixels] = this.takeLook();
    if (this.stickTurns) {
      out[INPUT.touchMoveX] = 0;
      out[INPUT.touchAimX] = touch.moveX;
      out[INPUT.aimStickHeld] = touch.moveX ? 1 : 0;
    }
    this.mine = false;
    this.ammoSelection = undefined;
    this.wheelAmmo = 0;
    return out;
  }
}
