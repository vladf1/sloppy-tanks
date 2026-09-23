import { AMMO_ORDER, hasAmmo } from "../game/ammunition";
import type { RenderState } from "../game/render-state";
import type { Simulation } from "../game/simulation";
import {
  idleCommand,
  type SimEvent,
  type Vec2,
  type VehicleCommand,
  type Weapon,
} from "../game/types";
import { DelayedChannel } from "./delayed-channel";
import { FixedStepClock, HOST_INTERVAL_MS } from "./fixed-step-clock";
import {
  captureRenderState,
  RenderTimeline,
  type HullPolicy,
  type RenderSample,
} from "./render-timeline";

type Action = { mine: true } | { ammoSelection: Weapon };
interface Input {
  command: VehicleCommand;
  aim: Vec2;
  actions: Action[];
  seq: number;
  receivedMs: number;
}
interface Measurement {
  type: "movement" | "shot" | "hit-feedback";
  milliseconds: number;
}
const INPUT_LEASE_MS = 250;
const REMOTE_BUFFER_MS = 100;
const MAX_ACTIONS = 8;
const MAX_MEASUREMENTS = 1000;

/** Dev-only timing experiment. Imported dynamically only for ?latency=... . */
export class LatencyExperiment {
  readonly rttMs: number;
  readonly rate: number;
  readonly policy: HullPolicy;
  readonly jitterMs: number;
  readonly measurements: Measurement[] = [];
  state: RenderState;
  private inputs = new DelayedChannel<Input>();
  private frames = new DelayedChannel<RenderSample>();
  private timeline = new RenderTimeline();
  private clock = new FixedStepClock(0);
  private actions: Action[] = [];
  private receivedActions: { action: Action; at: number }[] = [];
  private input?: Input;
  private seq = 0;
  private ack = 0;
  private startMs = 0;
  private startElapsed = 0;
  private nextInputMs = 0;
  private nextHostMs = 0;
  private lastFrameMs = 0;
  private lastLife = 0;
  private active = false;
  private outEvents: SimEvent[] = [];
  private previousCommand = idleCommand();
  private pendingMove?: { at: number; x: number; z: number; seq: number };
  private pendingFire?: number;
  private eventTimes = new WeakMap<SimEvent, number>();
  private pendingAmmo?: Weapon;
  private randomState = 12345;
  private readonly panel: HTMLPreElement;

  constructor(
    private readonly simulation: Simulation,
    root: HTMLElement,
    params: URLSearchParams,
  ) {
    const bounded = (name: string, fallback: number, max: number) => {
      const raw = params.get(name);
      const n = raw === null ? fallback : Number(raw);
      if (!Number.isFinite(n) || n < 0 || n > max) {
        throw new Error(`Invalid ${name}`);
      }
      return n;
    };
    this.rttMs = bounded("latency", 100, 500);
    this.rate = params.get("inputHz") === "30" ? 30 : 20;
    this.policy =
      params.get("hull") === "latest"
        ? "latest"
        : params.get("hull") === "extrapolate"
          ? "extrapolate"
          : "smooth";
    this.jitterMs = bounded("jitter", 0, 30);
    this.state = captureRenderState(simulation);
    this.timeline.reset({ state: this.state, events: [], ack: 0 });
    this.panel = document.createElement("pre");
    this.panel.id = "latency-diagnostics";
    this.panel.style.cssText =
      "position:fixed;left:12px;top:110px;z-index:10;background:#091522dd;color:white;padding:12px;font:12px monospace;pointer-events:none";
    root.append(this.panel);
  }

  reset(nowMs: number): void {
    this.inputs.clear();
    this.frames.clear();
    this.actions = [];
    this.receivedActions = [];
    this.input = undefined;
    this.seq = this.ack = 0;
    this.outEvents = [];
    this.pendingMove = undefined;
    this.pendingFire = undefined;
    this.pendingAmmo = undefined;
    this.eventTimes = new WeakMap();
    this.previousCommand = idleCommand();
    this.startMs = this.nextInputMs = this.lastFrameMs = nowMs;
    this.nextHostMs = nowMs + HOST_INTERVAL_MS;
    this.startElapsed = this.simulation.elapsed;
    this.clock = new FixedStepClock(nowMs);
    this.lastLife = this.simulation.human.life;
    this.state = captureRenderState(this.simulation);
    this.timeline.reset({ state: this.state, events: [], ack: 0 });
    this.active = true;
  }
  pause(): void {
    this.active = false;
    this.outEvents = [];
  }
  stall(milliseconds: number): void {
    this.nextHostMs += Math.max(0, Math.min(350, milliseconds));
  }
  events(): SimEvent[] {
    const events = this.outEvents;
    this.outEvents = [];
    return events;
  }
  private delay(): number {
    this.randomState = (1664525 * this.randomState + 1013904223) >>> 0;
    return this.rttMs / 2 + (this.randomState / 0x100000000) * this.jitterMs;
  }
  private measure(type: Measurement["type"], milliseconds: number): void {
    this.measurements.push({ type, milliseconds });
    if (this.measurements.length > MAX_MEASUREMENTS) {
      this.measurements.shift();
    }
  }

  advance(nowMs: number, command: VehicleCommand, aim: Vec2, autoplay: boolean): void {
    if (!this.active) {
      this.reset(nowMs);
    }
    if (
      (command.moveX || command.moveZ) &&
      !this.previousCommand.moveX &&
      !this.previousCommand.moveZ
    ) {
      this.pendingMove = {
        at: nowMs,
        x: this.state.viewer.position.x,
        z: this.state.viewer.position.z,
        seq: this.seq + 1,
      };
    }
    if (command.fire && !this.previousCommand.fire) {
      this.pendingFire = nowMs;
    }
    this.previousCommand = command;
    if (command.mine && this.actions.length < MAX_ACTIONS) {
      this.actions.push({ mine: true });
    }
    if (command.ammoSelection && this.actions.length < MAX_ACTIONS) {
      let selected = command.ammoSelection;
      if (typeof selected !== "string") {
        const from = AMMO_ORDER.indexOf(
          (this.pendingAmmo ?? this.state.viewer.selectedAmmo) as (typeof AMMO_ORDER)[number],
        );
        for (let offset = 1; offset <= AMMO_ORDER.length; offset++) {
          const weapon =
            AMMO_ORDER[(from + selected * offset + AMMO_ORDER.length) % AMMO_ORDER.length];
          if (hasAmmo(this.state.viewer, weapon)) {
            selected = weapon;
            break;
          }
        }
      }
      if (typeof selected === "string") {
        this.pendingAmmo = selected;
        this.actions.push({ ammoSelection: selected });
      }
    }
    if (nowMs >= this.nextInputMs) {
      this.inputs.send(
        {
          command: { ...command, mine: false, ammoSelection: undefined },
          aim: { x: aim.x, z: aim.z },
          actions: this.actions,
          seq: ++this.seq,
          receivedMs: 0,
        },
        nowMs,
        this.delay(),
      );
      this.actions = [];
      this.nextInputMs = Math.max(this.nextInputMs + 1000 / this.rate, nowMs);
    }
    for (const input of this.inputs.receive(nowMs)) {
      input.receivedMs = nowMs;
      this.input = input;
      for (const action of input.actions) {
        if (this.receivedActions.length < MAX_ACTIONS) {
          this.receivedActions.push({ action, at: nowMs });
        }
      }
    }
    if (nowMs >= this.nextHostMs) {
      const events: SimEvent[] = [];
      const ok = this.clock.advance(nowMs, () => {
        let control = idleCommand();
        if (this.input && nowMs - this.input.receivedMs < INPUT_LEASE_MS) {
          // The current authoritative origin is used every tick, not the displayed hull.
          const origin = this.simulation.human.alive
            ? this.simulation.human.body.translation()
            : this.simulation.human.previous;
          control = {
            ...this.input.command,
            aim: Math.atan2(this.input.aim.x - origin.x, this.input.aim.z - origin.z),
          };
          this.ack = this.input.seq;
        }
        while (this.receivedActions.length && nowMs - this.receivedActions[0].at > INPUT_LEASE_MS) {
          this.receivedActions.shift();
        }
        const action = this.receivedActions.shift();
        if (action) {
          Object.assign(control, action.action);
        }
        this.simulation.step(control, autoplay);
        for (const event of this.simulation.events.splice(0)) {
          // Event sources can contain Rapier references in today's local simulation.
          // Read just SimEvent data, never clone an explosion's entire source object.
          const clean: SimEvent = {
            type: event.type,
            x: event.x,
            z: event.z,
            id: event.id,
            owner: event.owner,
            team: event.team,
            weapon: event.weapon,
            size: event.size,
            label: event.label,
            color: event.color,
            height: event.height,
            deathStyle: event.deathStyle,
            coverKind: event.coverKind,
            material: event.material,
            force: event.force,
            from: event.from && { x: event.from.x, y: event.from.y, z: event.from.z },
            damageSource: event.damageSource && {
              cause: event.damageSource.cause,
              origin: { ...event.damageSource.origin },
            },
          };
          events.push(clean);
          if (
            event.type === "hurt" &&
            event.owner === this.simulation.human.id &&
            event.team !== this.simulation.human.team
          ) {
            this.eventTimes.set(clean, nowMs);
          }
        }
        if (this.lastLife !== this.simulation.human.life) {
          this.lastLife = this.simulation.human.life;
          this.inputs.clear();
          this.actions = [];
          this.receivedActions = [];
          this.input = undefined;
          this.pendingMove = undefined;
          this.pendingFire = undefined;
        }
      });
      if (!ok) {
        this.simulation.match.phase = "paused";
        this.pause();
        this.panel.textContent =
          "Latency experiment stopped: host tick debt exceeded 250 ms. Resume for a fresh baseline.";
        return;
      }
      this.frames.send(
        { state: captureRenderState(this.simulation), events, ack: this.ack },
        nowMs,
        this.delay(),
      );
      this.nextHostMs = Math.max(this.nextHostMs + HOST_INTERVAL_MS, nowMs);
    }
    for (const sample of this.frames.receive(nowMs)) {
      this.timeline.push(sample);
    }
    const localTime = this.startElapsed + (nowMs - this.startMs - this.rttMs / 2) / 1000;
    const displayed = this.timeline.read(
      localTime - REMOTE_BUFFER_MS / 1000,
      localTime,
      Math.max(0, (nowMs - this.lastFrameMs) / 1000),
      this.policy,
    );
    this.state = displayed.state;
    const viewer = this.state.viewer;
    // This is cosmetic aim only; the server still receives a delayed ground point.
    Object.assign(viewer, {
      aim: Math.atan2(aim.x - viewer.position.x, aim.z - viewer.position.z),
    });
    this.outEvents.push(...displayed.events);
    if (
      this.pendingMove &&
      this.timeline.latestAck >= this.pendingMove.seq &&
      Math.hypot(viewer.position.x - this.pendingMove.x, viewer.position.z - this.pendingMove.z) >
        0.02
    ) {
      this.measure("movement", nowMs - this.pendingMove.at);
      this.pendingMove = undefined;
    }
    for (const event of displayed.events) {
      const generatedAt = this.eventTimes.get(event);
      if (generatedAt !== undefined) {
        this.measure("hit-feedback", nowMs - generatedAt);
      }
      if (event.type === "shot" && event.id === viewer.id && this.pendingFire !== undefined) {
        this.measure("shot", nowMs - this.pendingFire);
        this.pendingFire = undefined;
      }
    }
    this.lastFrameMs = nowMs;
    this.panel.textContent =
      `LATENCY LAB · ${this.rttMs} ms added RTT + 0–${this.jitterMs} ms jitter/direction\n${this.rate} Hz input · 20 Hz snapshots · ${this.policy}\nRemote display buffer: ${REMOTE_BUFFER_MS} ms\n` +
      this.measurements
        .slice(-5)
        .map((measurement) => `${measurement.type}: ${measurement.milliseconds.toFixed(0)} ms`)
        .join("\n");
  }
}
