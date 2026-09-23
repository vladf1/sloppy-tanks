import "../touch-controls.css";
import { bindPress } from "./button-input";
import type { Controls } from "./controls";
import type { TouchState } from "./touch-mode";
import { MINE } from "./combat-rules";
import { FIRE_START, type StickKind } from "./touch-input";

const STICK_RADIUS = 58;
const ANCHOR_SHIFT = 22;

/** One overlay for the game lifetime; round resets only clear pointer state. */
export class TouchControls {
  private readonly layer: HTMLElement;
  private readonly mine: HTMLButtonElement;
  private readonly sticks: Record<StickKind, HTMLElement>;
  private readonly origins = { drive: { x: 0, y: 0 }, aim: { x: 0, y: 0 } };
  private enabled = false;

  constructor(
    private readonly root: HTMLElement,
    private readonly controls: Controls,
    private readonly simulation: TouchState,
    zoom: (amount: number) => void,
  ) {
    root.insertAdjacentHTML(
      "beforeend",
      `<div class="touch-controls" hidden>
      <div class="touch-stick touch-drive" role="group" aria-label="Drive joystick"><div class="stick-base"><i class="stick-knob"></i></div><span>DRIVE</span></div>
      <div class="touch-stick touch-aim" role="group" aria-label="Aim and fire joystick"><div class="stick-base"><i class="fire-ring"></i><i class="stick-knob"></i></div><span>AIM · PUSH TO FIRE</span></div>
      <button class="touch-mine" type="button" aria-label="Drop mine"><span>✹</span><small>MINE</small></button>
      </div>`,
    );
    this.layer = root.querySelector(".touch-controls")!;
    this.mine = this.layer.querySelector(".touch-mine")!;
    this.sticks = {
      drive: this.layer.querySelector(".touch-drive")!,
      aim: this.layer.querySelector(".touch-aim")!,
    };
    for (const kind of ["drive", "aim"] as const) {
      this.bindStick(kind);
    }
    this.controls.touch.changed = () => this.syncReleasedSticks();
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
      this.layer.hidden = true;
    }
  }

  private bindStick(kind: StickKind): void {
    const element = this.sticks[kind];
    const base = element.querySelector<HTMLElement>(".stick-base")!;
    const knob = element.querySelector<HTMLElement>(".stick-knob")!;
    const move = (event: PointerEvent) => {
      if (this.controls.touch.pointers[kind] !== event.pointerId) {
        return;
      }
      if (!this.controls.active()) {
        this.controls.clear();
        return;
      }
      event.preventDefault();
      const origin = this.origins[kind];
      const x = (event.clientX - origin.x) / STICK_RADIUS;
      const y = (event.clientY - origin.y) / STICK_RADIUS;
      this.controls.touch.move(kind, event.pointerId, x, y);
      const scale = STICK_RADIUS / Math.max(1, Math.hypot(x, y));
      knob.style.transform = `translate(${x * scale}px, ${y * scale}px)`;
      element.classList.toggle("firing", kind === "aim" && this.controls.touch.fire);
    };
    element.style.setProperty("--fire-diameter", `${STICK_RADIUS * FIRE_START * 2}px`);
    element.addEventListener("pointerdown", (event) => {
      if (!this.enabled || !this.controls.active() || event.button !== 0) {
        return;
      }
      if (!this.controls.touch.begin(kind, event.pointerId)) {
        return;
      }
      const rect = element.getBoundingClientRect();
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      const dx = Math.max(-ANCHOR_SHIFT, Math.min(ANCHOR_SHIFT, event.clientX - centerX));
      const dy = Math.max(-ANCHOR_SHIFT, Math.min(ANCHOR_SHIFT, event.clientY - centerY));
      this.origins[kind] = { x: centerX + dx, y: centerY + dy };
      base.style.transform = `translate(${dx}px, ${dy}px)`;
      element.setPointerCapture(event.pointerId);
      element.classList.add("held");
      move(event);
    });
    element.addEventListener("pointermove", move);
    for (const name of ["pointerup", "pointercancel", "lostpointercapture"] as const) {
      element.addEventListener(name, (event) => this.controls.touch.end(kind, event.pointerId));
    }
  }

  private syncReleasedSticks(): void {
    for (const kind of ["drive", "aim"] as const) {
      if (this.controls.touch.pointers[kind] !== null) {
        continue;
      }
      const element = this.sticks[kind];
      if (!element.classList.contains("held")) {
        continue;
      }
      element.classList.remove("held", "firing");
      element.querySelector<HTMLElement>(".stick-base")!.style.transform = "";
      element.querySelector<HTMLElement>(".stick-knob")!.style.transform = "";
    }
  }

  update(): void {
    if (!this.enabled) {
      return;
    }
    const playing = this.controls.active();
    this.layer.hidden = !playing;
    const cooldown = this.simulation.human.mineCooldown;
    this.mine.disabled = !playing || cooldown > 0;
    this.mine.style.setProperty("--mine-ready", `${(1 - cooldown / MINE.cooldownSeconds) * 100}%`);
    this.mine.querySelector("small")!.textContent =
      cooldown > 0 ? `${cooldown.toFixed(1)}s` : "MINE";
    this.mine.setAttribute(
      "aria-label",
      cooldown > 0 ? `Mine ready in ${cooldown.toFixed(1)} seconds` : "Drop mine",
    );
  }
}
