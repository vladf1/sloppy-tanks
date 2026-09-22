export type StickKind = "drive" | "aim";
export type TouchMode = "auto" | "on" | "off";
export const STICK_DEADZONE = 0.12;
export const FIRE_START = 0.7;
export const FIRE_STOP = 0.58;

/** Pointer ownership and normalized input, independent of rendering and display Hz. */
export class TouchInput {
  moveX = 0;
  moveZ = 0;
  aimX = 0;
  aimY = -1;
  aiming = false;
  fire = false;
  changed = () => {};
  readonly pointers: Record<StickKind, number | null> = { drive: null, aim: null };

  begin(kind: StickKind, pointerId: number): boolean {
    if (this.pointers[kind] !== null || Object.values(this.pointers).includes(pointerId)) {
      return false;
    }
    this.pointers[kind] = pointerId;
    return true;
  }

  move(kind: StickKind, pointerId: number, x: number, y: number): void {
    if (this.pointers[kind] !== pointerId) {
      return;
    }
    const distance = Math.hypot(x, y);
    if (kind === "drive") {
      const speed = Math.max(0, (Math.min(1, distance) - STICK_DEADZONE) / (1 - STICK_DEADZONE));
      this.moveX = distance ? (x / distance) * speed : 0;
      this.moveZ = distance ? (y / distance) * speed : 0;
    } else {
      if (distance > STICK_DEADZONE) {
        this.aimX = x / distance;
        this.aimY = y / distance;
        this.aiming = true;
      }
      this.fire = distance >= (this.fire ? FIRE_STOP : FIRE_START);
    }
  }

  end(kind: StickKind, pointerId: number): void {
    if (this.pointers[kind] !== pointerId) {
      return;
    }
    this.pointers[kind] = null;
    if (kind === "drive") {
      this.moveX = this.moveZ = 0;
    } else {
      this.fire = false;
    }
    this.changed();
  }

  clear(): void {
    const held = this.pointers.drive !== null || this.pointers.aim !== null;
    this.moveX = this.moveZ = 0;
    this.fire = false;
    this.aiming = false;
    this.pointers.drive = this.pointers.aim = null;
    if (held) {
      this.changed();
    }
  }
}
