/** A finger-owned touch control: the drive stick, or a finger on the arena, which aims
 * there and fires while it is down. */
export type TouchKind = "drive" | "arena";
export type TouchMode = "auto" | "on" | "off";
export const STICK_DEADZONE = 0.12;

/** Pointer ownership and normalized input, independent of rendering and display Hz. */
export class TouchInput {
  moveX = 0;
  moveZ = 0;
  changed = () => {};
  readonly pointers: Record<TouchKind, number | null> = {
    drive: null,
    arena: null,
  };

  /** Held while an arena finger is down. */
  get fire(): boolean {
    return this.pointers.arena !== null;
  }

  begin(kind: TouchKind, pointerId: number): boolean {
    if (this.pointers[kind] !== null || Object.values(this.pointers).includes(pointerId)) {
      return false;
    }
    this.pointers[kind] = pointerId;
    return true;
  }

  /** The drive stick's push in stick radii (x right, y down), from its own finger. */
  moveStick(pointerId: number, x: number, y: number): void {
    if (this.pointers.drive !== pointerId) {
      return;
    }
    const distance = Math.hypot(x, y);
    const speed = Math.max(0, (Math.min(1, distance) - STICK_DEADZONE) / (1 - STICK_DEADZONE));
    this.moveX = distance ? (x / distance) * speed : 0;
    this.moveZ = distance ? (y / distance) * speed : 0;
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
    this.pointers.drive = this.pointers.arena = null;
    if (held) {
      this.changed();
    }
  }
}
