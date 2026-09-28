import { angleDelta } from "./math";
import type { VehicleCommand } from "./types";
import { FIRST_PERSON } from "./view-settings";

/** The player's view from inside the turret. It is presentation state: each
 * tick it becomes the same `VehicleCommand` aim and movement the overhead
 * pointer produces, so the simulation never knows which view is in use. */
export class FirstPersonLook {
  enabled = false;
  /** View and turret heading in the `Tank.aim` convention: facing (sin, cos) on X/Z. */
  yaw = 0;

  /** Entering starts from the turret's current aim so the view never snaps. */
  toggle(aim: number): void {
    this.enabled = !this.enabled;
    if (this.enabled) {
      this.yaw = angleDelta(0, aim);
    }
  }

  /** Positive mouse pixels or aim-stick X turn to the right, which lowers yaw. */
  turn(pixels: number, stickX: number, dt: number): void {
    const turn =
      pixels * FIRST_PERSON.mouseRadiansPerPixel +
      stickX * FIRST_PERSON.touchTurnRadiansPerSecond * dt;
    this.yaw = angleDelta(0, this.yaw - turn);
  }

  /** Forward input drives where the turret looks; strafing input heads to its sides. */
  steer(command: VehicleCommand): VehicleCommand {
    if (!this.enabled) {
      return command;
    }
    return { ...command, ...viewRelativeMove(command.moveX, command.moveZ, this.yaw) };
  }

  /** Clockwise screen angle of a world bearing, with the view's heading straight up. */
  screenAngle(bearing: number): number {
    return angleDelta(bearing, this.yaw);
  }
}

/** Rotate screen-style movement (negative Z is forward, positive X is right)
 * into the world for a view facing `yaw`. The overhead camera faces yaw = π,
 * where this is the identity. */
export function viewRelativeMove(
  moveX: number,
  moveZ: number,
  yaw: number,
): { moveX: number; moveZ: number } {
  const sin = Math.sin(yaw);
  const cos = Math.cos(yaw);
  return {
    moveX: -moveX * cos - moveZ * sin,
    moveZ: moveX * sin - moveZ * cos,
  };
}
