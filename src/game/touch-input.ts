export type StickKind = "drive" | "aim";
/** A finger-owned touch control: the two sticks, the held fire button and, on
 * phones, a finger on the arena, which aims there and fires while it is down. */
export type TouchKind = StickKind | "fire" | "arena";
export type TouchMode = "auto" | "on" | "off";
export const STICK_DEADZONE = 0.12;

/** Pointer ownership and normalized input, independent of rendering and display Hz. */
export class TouchInput {
  moveX = 0;
  moveZ = 0;
  aimX = 0;
  aimY = -1;
  aiming = false;
  changed = () => {};
  readonly pointers: Record<TouchKind, number | null> = {
    drive: null,
    aim: null,
    fire: null,
    arena: null,
  };

  /** Held while the fire button or an arena finger is down. */
  get fire(): boolean {
    return this.pointers.fire !== null || this.pointers.arena !== null;
  }

  begin(kind: TouchKind, pointerId: number): boolean {
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
    } else if (distance > STICK_DEADZONE) {
      this.aimX = x / distance;
      this.aimY = y / distance;
      this.aiming = true;
    }
  }

  end(kind: TouchKind, pointerId: number): void {
    if (this.pointers[kind] !== pointerId) {
      return;
    }
    this.pointers[kind] = null;
    if (kind === "drive") {
      this.moveX = this.moveZ = 0;
    }
    this.changed();
  }

  clear(): void {
    const held = Object.values(this.pointers).some((pointer) => pointer !== null);
    this.moveX = this.moveZ = 0;
    this.aiming = false;
    this.pointers.drive = this.pointers.aim = this.pointers.fire = this.pointers.arena = null;
    if (held) {
      this.changed();
    }
  }
}
