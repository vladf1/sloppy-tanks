import type { VehicleCommand } from "./types";
export class Controls {
  keys = new Set<string>();
  fire = false;
  mine = false;
  nx = 0;
  ny = 0;
  constructor(
    canvas: HTMLCanvasElement,
    public pause: () => void,
    zoom: (amount: number) => void,
  ) {
    window.addEventListener("keydown", (e) => {
      if (e.code === "Escape") {
        pause();
        return;
      }
      if (["KeyW", "KeyA", "KeyS", "KeyD", "Space"].includes(e.code)) {
        if ((e.target as HTMLElement).tagName === "INPUT") return;
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
      if (e.button === 0) this.fire = true;
      if (e.button === 2) this.mine = true;
      canvas.focus();
    });
    window.addEventListener("pointerup", (e) => {
      if (e.button === 0) this.fire = false;
    });
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());
    canvas.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        zoom(Math.sign(e.deltaY) * 2);
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
  clear() {
    this.keys.clear();
    this.fire = false;
    this.mine = false;
  }
  command(aim: number): VehicleCommand {
    const mine = this.mine;
    this.mine = false;
    return {
      moveX: Number(this.keys.has("KeyD")) - Number(this.keys.has("KeyA")),
      moveZ: Number(this.keys.has("KeyS")) - Number(this.keys.has("KeyW")),
      aim,
      fire: this.fire,
      mine,
    };
  }
}
