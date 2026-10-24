import "../touch-controls.css";
import { bindPress } from "./button-input";
import type { Controls } from "./controls";

const STICK_RADIUS = 58;
const ANCHOR_SHIFT = 22;

/** One overlay for the game lifetime; round resets only clear pointer state. */
export class TouchControls {
  private readonly layer: HTMLElement;
  private readonly mine: HTMLButtonElement;
  private readonly stick: HTMLElement;
  /** How to aim and fire, over the arena until the first touch there. */
  private readonly fireHint: HTMLElement;
  private origin = { x: 0, y: 0 };
  private enabled = false;
  /** The cooldown a mine started with: the button fills over the engine's reload,
   * whatever length the engine gives it. */
  private mineReload = 0;
  /** The animation frame that shows the hidden controls again (see `update`). */
  private reveal = 0;

  constructor(
    private readonly root: HTMLElement,
    private readonly controls: Controls,
    private readonly mineCooldown: () => number,
    zoom: (amount: number) => void,
  ) {
    root.insertAdjacentHTML(
      "beforeend",
      `<div class="touch-controls" hidden>
      <div class="touch-stick touch-drive" role="group" aria-label="Drive joystick"><div class="stick-base"><i class="stick-knob"></i></div><span>DRIVE</span></div>
      <button class="touch-mine" type="button" aria-label="Drop mine"><svg class="touch-mine-icon" aria-hidden="true" focusable="false"><use href="#hud-mine" /></svg><small>MINE</small></button>
      <div class="touch-fire-hint" aria-hidden="true"><i class="touch-fire-ring"></i><span><b>TOUCH THE ARENA</b>to aim and fire</span></div>
      </div>`,
    );
    this.layer = root.querySelector(".touch-controls")!;
    this.fireHint = this.layer.querySelector(".touch-fire-hint")!;
    this.mine = this.layer.querySelector(".touch-mine")!;
    this.stick = this.layer.querySelector(".touch-drive")!;
    this.bindStick();
    const canvas = root.querySelector<HTMLCanvasElement>("#game");
    if (canvas) {
      this.bindArenaFire(canvas);
      // A held finger otherwise brings up iOS's magnifier (or a long-press menu) over
      // the arena; pointer events, which the game reads, still arrive.
      for (const element of [canvas, this.layer]) {
        element.addEventListener("touchstart", (event) => event.preventDefault(), {
          passive: false,
        });
      }
    }
    this.controls.touch.changed = () => this.syncReleasedControls();
    bindPress(this.mine, () => {
      if (this.controls.active() && !this.mine.disabled) {
        this.controls.mine = true;
      }
    });
    root
      .querySelector(".hud-actions")!
      .insertAdjacentHTML(
        "afterbegin",
        `<button type="button" class="quiet touch-zoom" id="zoom-out" aria-label="Zoom out">−</button><button type="button" class="quiet touch-zoom" id="zoom-in" aria-label="Zoom in">+</button>`,
      );
    bindPress(root.querySelector("#zoom-out")!, () => zoom(2));
    bindPress(root.querySelector("#zoom-in")!, () => zoom(-2));
  }

  private readonly resized = () => this.controls.clear();

  setEnabled(enabled: boolean): void {
    if (enabled === this.enabled) {
      return;
    }
    this.enabled = enabled;
    this.root.classList.toggle("touch-enabled", enabled);
    if (enabled) {
      window.addEventListener("resize", this.resized);
      this.update();
    } else {
      window.removeEventListener("resize", this.resized);
      this.controls.touch.clear();
      cancelAnimationFrame(this.reveal);
      this.reveal = 0;
      this.layer.hidden = true;
    }
  }

  private bindStick(): void {
    const element = this.stick;
    const base = element.querySelector<HTMLElement>(".stick-base")!;
    const knob = element.querySelector<HTMLElement>(".stick-knob")!;
    const move = (event: PointerEvent) => {
      if (this.controls.touch.pointers.drive !== event.pointerId) {
        return;
      }
      if (!this.controls.active()) {
        this.controls.clear();
        return;
      }
      event.preventDefault();
      const x = (event.clientX - this.origin.x) / STICK_RADIUS;
      const y = (event.clientY - this.origin.y) / STICK_RADIUS;
      this.controls.touch.moveStick(event.pointerId, x, y);
      const push = Math.hypot(x, y);
      const scale = STICK_RADIUS / Math.max(1, push);
      knob.style.transform = `translate(${x * scale}px, ${y * scale}px)`;
      // The rim lights toward the drive, brighter the further the knob is pushed.
      base.style.setProperty(
        "--stick-angle",
        `${Math.round((Math.atan2(x, -y) * 180) / Math.PI)}deg`,
      );
      base.style.setProperty("--stick-push", Math.min(1, push).toFixed(2));
    };
    element.addEventListener("pointerdown", (event) => {
      if (!this.enabled || !this.controls.active() || event.button !== 0) {
        return;
      }
      if (!this.controls.touch.begin("drive", event.pointerId)) {
        return;
      }
      const rect = element.getBoundingClientRect();
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      const dx = Math.max(-ANCHOR_SHIFT, Math.min(ANCHOR_SHIFT, event.clientX - centerX));
      const dy = Math.max(-ANCHOR_SHIFT, Math.min(ANCHOR_SHIFT, event.clientY - centerY));
      this.origin = { x: centerX + dx, y: centerY + dy };
      base.style.transform = `translate(${dx}px, ${dy}px)`;
      element.setPointerCapture(event.pointerId);
      element.classList.add("held");
      move(event);
    });
    element.addEventListener("pointermove", move);
    for (const name of ["pointerup", "pointercancel", "lostpointercapture"] as const) {
      element.addEventListener(name, (event) => this.controls.touch.end("drive", event.pointerId));
    }
  }

  /** Touch players aim and fire by touching the arena: the turret turns toward the
   * finger, which may stay down and slide, and fires until it lifts. In first person
   * the finger's sideways travel turns the view instead, like mouse look. */
  private bindArenaFire(canvas: HTMLCanvasElement): void {
    const touching = (event: PointerEvent) =>
      event.pointerType === "touch" && this.enabled && this.controls.active();
    let lastX = 0;
    canvas.addEventListener("pointerdown", (event) => {
      // Only the finger that owns the shot aims; a second one cannot pull the turret.
      if (touching(event) && this.controls.touch.begin("arena", event.pointerId)) {
        this.fireHint.classList.add("learned");
        lastX = event.clientX;
        this.controls.aimAt(event.clientX, event.clientY);
      }
    });
    canvas.addEventListener("pointermove", (event) => {
      if (touching(event) && this.controls.touch.pointers.arena === event.pointerId) {
        // The engine reads look travel only in first person and the point only overhead.
        this.controls.look += event.clientX - lastX;
        lastX = event.clientX;
        this.controls.aimAt(event.clientX, event.clientY);
      }
    });
    for (const name of ["pointerup", "pointercancel", "lostpointercapture"] as const) {
      canvas.addEventListener(name, (event) => this.controls.touch.end("arena", event.pointerId));
    }
  }

  private syncReleasedControls(): void {
    const element = this.stick;
    if (this.controls.touch.pointers.drive !== null || !element.classList.contains("held")) {
      return;
    }
    element.classList.remove("held");
    const base = element.querySelector<HTMLElement>(".stick-base")!;
    base.style.transform = "";
    base.style.removeProperty("--stick-push");
    element.querySelector<HTMLElement>(".stick-knob")!.style.transform = "";
  }

  update(): void {
    if (!this.enabled) {
      return;
    }
    const playing = this.controls.active();
    // The controls come back a frame after the rest of the HUD's respawn changes: laid
    // out and painted on the same frame, they held an iPad's respawn frame 6 ms longer.
    if (!playing) {
      cancelAnimationFrame(this.reveal);
      this.reveal = 0;
      this.layer.hidden = true;
    } else if (this.layer.hidden && !this.reveal) {
      this.reveal = requestAnimationFrame(() => {
        this.reveal = 0;
        this.layer.hidden = !this.controls.active();
      });
    }
    const cooldown = this.mineCooldown();
    this.mineReload = cooldown > 0 ? Math.max(this.mineReload, cooldown) : 0;
    this.mine.disabled = !playing || cooldown > 0;
    // Only what changed is written: the HUD updates the button 15 times a second, and
    // WebKit repaints it, rim, shadow and filtered icon, for any write of its text,
    // even the same "MINE".
    const ready = `${cooldown > 0 ? (1 - cooldown / this.mineReload) * 100 : 100}%`;
    if (this.mine.style.getPropertyValue("--mine-ready") !== ready) {
      this.mine.style.setProperty("--mine-ready", ready);
    }
    const label = this.mine.querySelector("small")!;
    const text = cooldown > 0 ? `${cooldown.toFixed(1)}s` : "MINE";
    if (label.textContent !== text) {
      label.textContent = text;
    }
    const description = cooldown > 0 ? `Mine ready in ${cooldown.toFixed(1)} seconds` : "Drop mine";
    if (this.mine.getAttribute("aria-label") !== description) {
      this.mine.setAttribute("aria-label", description);
    }
  }
}
