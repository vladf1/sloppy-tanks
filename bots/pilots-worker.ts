/**
 * The pilots Worker (`pilots.wrangler.jsonc`): players run by the bot brain
 * (`crates/pilot` as Wasm), one squad of up to eight a region. It has no public address;
 * the bots Worker's control page and API reach it over their `PILOTS` service binding.
 * It deploys on its own (`pnpm run pilots:deploy`) because its engine is game code,
 * stamped with the content version the servers it plays on must run.
 */

import { DurableObject } from "cloudflare:workers";
import { PilotEngine } from "./pilot-engine";
import type { Connect } from "./pilot-seat";
import { PilotSquad, readBrain, type DirectoryServer, type PilotBrain } from "./pilot-squad";
import {
  CONNECT_TIMEOUT_MS,
  DEFAULT_MINUTES,
  MAINTAIN_MS,
  MAX_MINUTES,
  REGIONS,
  WATCHDOG_MS,
  currentColo,
  directoryList,
  environment,
  integer,
  json,
  region,
  roomCode,
  type DirectoryEnv,
  type Environment,
  type Region,
} from "./regions";
// Built by scripts/build-pilot.mjs, which `pnpm run pilots:deploy` runs first.
import pilotModule from "./generated/sloppy_pilot.wasm";

interface Env extends DirectoryEnv {
  SQUAD: DurableObjectNamespace<PilotSwarm>;
}

// Each pilot runs a copy of the arena's physics and the browser's client in the region's
// one Wasm instance; a room has eight seats anyway.
const MAX_PILOTS_PER_REGION = 8;
// Pilots play at about the frame rate of a slow browser tab; their input goes out at 20 Hz.
const PILOT_TICK_MS = 33;

interface PilotRun {
  region: Region;
  pilots: number;
  stopAt: number;
  /** Join this room. */
  room?: string;
  /** With no room given, create one rather than join the fullest open room. */
  host: boolean;
  brain: PilotBrain;
  /** The directory environment to play on, when not the Worker's own. */
  environment?: Environment;
}

function swarm(env: Env, region: Region): DurableObjectStub<PilotSwarm> {
  // The hint only takes effect when the object is first created, so every lookup passes it.
  return env.SQUAD.get(env.SQUAD.idFromName("pilots-" + region), { locationHint: region });
}

/** A start request from the control plane, checked; `bots` is how many pilots. */
export function readRun(body: Record<string, unknown>, now: number): PilotRun {
  return {
    region: region(body.region),
    pilots: integer(body.bots, 1, 1, MAX_PILOTS_PER_REGION, "pilots"),
    stopAt: now + integer(body.minutes, DEFAULT_MINUTES, 1, MAX_MINUTES, "minutes") * 60_000,
    room: roomCode(body.room),
    host: body.host === true,
    brain: readBrain(body),
    environment: environment(body.environment),
  };
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    try {
      if (url.pathname === "/status" && request.method === "GET") {
        const regions = await Promise.all(REGIONS.map((name) => swarm(env, name).status()));
        return json({ regions: regions.filter((item) => item.running) });
      }
      if (request.method !== "POST") {
        return json({ error: "Not found" }, 404);
      }
      const body = (await request.json().catch(() => ({}))) as Record<string, unknown>;
      if (url.pathname === "/start") {
        const run = readRun(body, Date.now());
        return json(await swarm(env, run.region).start(run));
      }
      if (url.pathname === "/stop") {
        const targets = body.region === undefined ? [...REGIONS] : [region(body.region)];
        await Promise.all(targets.map((name) => swarm(env, name).stop()));
        return json({ stopped: targets });
      }
      return json({ error: "Not found" }, 404);
    } catch (error) {
      return json({ error: error instanceof Error ? error.message : String(error) }, 400);
    }
  },
} satisfies ExportedHandler<Env>;

/** A pilot's socket: Workers open WebSockets through fetch, so a wss: address goes as https:. */
const connectPilot: Connect = async (url, events) => {
  // The signal stays bound to the upgraded socket, so the timeout must end with the handshake.
  const abort = new AbortController();
  const timeout = setTimeout(() => abort.abort(), CONNECT_TIMEOUT_MS);
  const response = await fetch(url.replace(/^ws/, "http"), {
    headers: { Upgrade: "websocket" },
    signal: abort.signal,
  }).finally(() => clearTimeout(timeout));
  const socket = response.webSocket;
  if (!socket) {
    throw new Error(`The room refused the pilot: ${response.status} ${await response.text()}`);
  }
  socket.binaryType = "arraybuffer";
  socket.accept();
  socket.addEventListener("message", (event) => events.message(event.data as string | ArrayBuffer));
  socket.addEventListener("close", (event) => events.close(event.code));
  socket.addEventListener("error", () => events.close(1006));
  return socket;
};

/** Runs one region's pilots. Their sockets keep it awake only while a run is configured. */
export class PilotSwarm extends DurableObject<Env> {
  private run?: PilotRun;
  private squad?: PilotSquad;
  private timer?: ReturnType<typeof setInterval>;
  private colo?: string;
  private coloRequested = false;
  private lastMaintainMs = 0;
  private maintaining = false;
  private lastError?: string;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    void ctx.blockConcurrencyWhile(async () => {
      this.run = await ctx.storage.get<PilotRun>("run");
    });
  }

  async start(run: PilotRun): Promise<ReturnType<PilotSwarm["summary"]>> {
    // Starting an identical run again only moves its deadline.
    if (JSON.stringify({ ...this.run, stopAt: 0 }) !== JSON.stringify({ ...run, stopAt: 0 })) {
      this.release();
    }
    this.run = run;
    await this.ctx.storage.put("run", run);
    await this.ctx.storage.setAlarm(Date.now() + WATCHDOG_MS);
    this.play();
    await this.maintain(Date.now());
    return this.summary();
  }

  async stop(): Promise<void> {
    this.run = undefined;
    this.release();
    await this.ctx.storage.deleteAll();
    await this.ctx.storage.deleteAlarm();
  }

  status(): ReturnType<PilotSwarm["summary"]> {
    if (this.run && Date.now() < this.run.stopAt) {
      this.play();
    }
    return this.summary();
  }

  override async alarm(): Promise<void> {
    const run = this.run;
    if (!run || Date.now() >= run.stopAt) {
      await this.stop();
      return;
    }
    this.play();
    await this.ctx.storage.setAlarm(Math.min(run.stopAt, Date.now() + WATCHDOG_MS));
  }

  private summary() {
    return {
      region: this.run?.region ?? null,
      colo: this.colo ?? null,
      running: !!this.run,
      config: this.run ?? null,
      lastError: this.lastError ?? null,
      squad: this.squad?.summary() ?? null,
    };
  }

  /** Pilots tick every frame and look for their room every few seconds. */
  private play(): void {
    const run = this.run;
    if (!run) {
      return;
    }
    this.squad ??= new PilotSquad(
      {
        region: run.region,
        pilots: run.pilots,
        room: run.room,
        host: run.host,
        brain: run.brain,
      },
      () => new PilotEngine(pilotModule),
      connectPilot,
    );
    this.timer ??= setInterval(() => {
      const now = Date.now();
      if (!this.run || now >= this.run.stopAt) {
        void this.stop();
        return;
      }
      this.squad?.tick();
      if (now - this.lastMaintainMs >= MAINTAIN_MS) {
        void this.maintain(now);
      }
    }, PILOT_TICK_MS);
    if (!this.coloRequested) {
      this.coloRequested = true;
      void currentColo()
        .then((colo) => (this.colo = colo))
        .catch(() => (this.coloRequested = false));
    }
  }

  private async maintain(now: number): Promise<void> {
    if (this.maintaining || !this.squad) {
      return;
    }
    this.maintaining = true;
    this.lastMaintainMs = now;
    try {
      this.squad.maintain(await this.listServers());
      this.lastError = undefined;
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
    } finally {
      this.maintaining = false;
    }
  }

  private async listServers(): Promise<DirectoryServer[]> {
    const response = await fetch(directoryList(this.env, this.run?.environment));
    if (!response.ok) {
      throw new Error(`Server directory ${response.status}: ${await response.text()}`);
    }
    return ((await response.json()) as { servers: DirectoryServer[] }).servers;
  }

  private release(): void {
    clearInterval(this.timer ?? null);
    this.timer = undefined;
    this.squad?.stop();
    this.squad = undefined;
    this.lastError = undefined;
  }
}
