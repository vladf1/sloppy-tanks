import { idleCommand, type Tank, type VehicleCommand, type Weapon } from "../game/types";
import { setDriver } from "./multiplayer-simulation";

export const INPUT_LEASE_MS = 250;
export const BOT_TAKEOVER_MS = 5000;
export const MAX_QUEUED_ACTIONS = 8;
export const MAX_INPUT_LAG_TICKS = 30;
const MAX_AIM_COORDINATE = 1024;
const MAX_INPUTS_PER_SECOND = 60;
const PLAYER_WEAPONS = ["standard", "spread", "rocket", "ricochet", "piercing"] as const;
type Action = { type: "mine" } | { type: "ammo"; weapon: Weapon };
type Aim = { x: number; z: number } | { angle: number };
export interface ControlInput {
  controlEpoch: number;
  seq: number;
  observedTick: number;
  moveX: number;
  moveZ: number;
  aim: Aim;
  fire: boolean;
  actions: Action[];
}
function object(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}
function finite(value: unknown, bound: number): value is number {
  return typeof value === "number" && Number.isFinite(value) && Math.abs(value) <= bound;
}
function validInput(value: unknown): value is ControlInput {
  if (!object(value) || !object(value.aim) || !Array.isArray(value.actions)) {
    return false;
  }
  const aim = value.aim;
  return (
    Number.isSafeInteger(value.controlEpoch) &&
    Number.isSafeInteger(value.seq) &&
    Number.isSafeInteger(value.observedTick) &&
    finite(value.moveX, 1) &&
    finite(value.moveZ, 1) &&
    typeof value.fire === "boolean" &&
    ((Object.keys(aim).length === 1 && finite(aim.angle, Math.PI)) ||
      (Object.keys(aim).length === 2 &&
        finite(aim.x, MAX_AIM_COORDINATE) &&
        finite(aim.z, MAX_AIM_COORDINATE))) &&
    value.actions.length <= MAX_QUEUED_ACTIONS &&
    value.actions.every(
      (action) =>
        object(action) &&
        ((action.type === "mine" && Object.keys(action).length === 1) ||
          (action.type === "ammo" &&
            Object.keys(action).length === 2 &&
            PLAYER_WEAPONS.some((weapon) => weapon === action.weapon))),
    )
  );
}

/** One assigned seat, independent of sockets and wall-clock APIs. Room ownership is checked by the host. */
export class PlayerControls {
  controlEpoch = 1;
  ack = { inputSeq: 0, appliedTick: 0 };
  private life: number;
  private alive: boolean;
  private input?: ControlInput;
  private lastSeq = 0;
  private lastReceivedMs: number;
  private rateWindowMs: number;
  private receivedInWindow = 0;
  private actions: { action: Action; receivedMs: number }[] = [];
  private suspended = false;
  constructor(
    readonly tank: Tank,
    nowMs: number,
    private readonly botTakeover = true,
  ) {
    if (!tank.human) {
      throw new Error("Controls require a player-owned tank");
    }
    this.life = tank.life;
    this.alive = tank.alive;
    this.lastReceivedMs = this.rateWindowMs = nowMs;
  }
  /** Returns false without partially applying a malformed/stale message or extending its lease. */
  accept(value: unknown, serverTick: number, nowMs: number): boolean {
    this.synchronizeLife();
    if (this.suspended || !this.tank.alive || this.tank.driver !== "human") {
      return false;
    }
    if (nowMs - this.rateWindowMs >= 1000) {
      this.rateWindowMs = nowMs;
      this.receivedInWindow = 0;
    }
    if (++this.receivedInWindow > MAX_INPUTS_PER_SECOND) {
      return false;
    }
    this.expireActions(nowMs);
    if (
      !validInput(value) ||
      value.controlEpoch !== this.controlEpoch ||
      value.seq <= this.lastSeq ||
      value.observedTick < 0 ||
      value.observedTick > serverTick ||
      serverTick - value.observedTick > MAX_INPUT_LAG_TICKS ||
      this.actions.length + value.actions.length > MAX_QUEUED_ACTIONS
    ) {
      return false;
    }
    // Do not retain caller-owned mutable message objects.
    this.input = { ...value, aim: { ...value.aim }, actions: [] };
    this.actions.push(
      ...value.actions.map((action) => ({ action: { ...action }, receivedMs: nowMs })),
    );
    this.lastSeq = value.seq;
    this.lastReceivedMs = nowMs;
    return true;
  }
  command(tick: number, nowMs: number): VehicleCommand | undefined {
    this.synchronizeLife();
    if (!this.suspended && nowMs - this.lastReceivedMs >= BOT_TAKEOVER_MS) {
      this.suspend();
    }
    if (this.suspended || this.tank.driver === "bot") {
      return undefined;
    }
    const command = { ...idleCommand(), aim: this.tank.aim };
    if (!this.tank.alive) {
      return command;
    }
    this.expireActions(nowMs);
    if (!this.input || nowMs - this.lastReceivedMs >= INPUT_LEASE_MS) {
      this.actions = [];
      return command;
    }
    const input = this.input;
    const origin = this.tank.body.translation();
    Object.assign(command, {
      moveX: input.moveX,
      moveZ: input.moveZ,
      fire: input.fire,
      aim:
        "angle" in input.aim
          ? input.aim.angle
          : Math.atan2(input.aim.x - origin.x, input.aim.z - origin.z),
    });
    if (this.ack.inputSeq !== input.seq) {
      this.ack = { inputSeq: input.seq, appliedTick: tick };
    }
    const next = this.actions.shift()?.action;
    if (next?.type === "mine") {
      command.mine = true;
    }
    if (next?.type === "ammo") {
      command.ammoSelection = next.weapon;
    }
    return command;
  }
  suspend(): void {
    if (this.suspended) {
      return;
    }
    this.suspended = true;
    setDriver(this.tank, this.botTakeover ? "bot" : "idle");
    this.newEpoch();
  }
  resume(nowMs: number): void {
    this.synchronizeLife();
    this.suspended = false;
    setDriver(this.tank, "human");
    this.lastReceivedMs = nowMs;
    this.newEpoch();
  }
  refreshLife(): void {
    this.synchronizeLife();
  }
  private synchronizeLife(): void {
    if (this.life === this.tank.life && this.alive === this.tank.alive) {
      return;
    }
    this.life = this.tank.life;
    this.alive = this.tank.alive;
    this.newEpoch();
  }
  private newEpoch(): void {
    this.controlEpoch++;
    this.input = undefined;
    this.actions = [];
    this.lastSeq = 0;
    this.ack = { inputSeq: 0, appliedTick: 0 };
  }
  private expireActions(nowMs: number): void {
    this.actions = this.actions.filter((item) => nowMs - item.receivedMs < INPUT_LEASE_MS);
  }
}
