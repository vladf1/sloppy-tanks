import { monitorEventLoopDelay } from "node:perf_hooks";
import type { RoomActivity, RoomSample } from "./room-session";

/** How often room and process load is sampled for /stats. */
export const SAMPLE_INTERVAL_MS = 10_000;
/** Samples between summary log lines while rooms are active (one minute). */
const SAMPLES_PER_SUMMARY = 6;

export interface ServerStats {
  sampledAt: string;
  windowSeconds: number;
  uptimeSeconds: number;
  rooms: number;
  players: number;
  sockets: number;
  sentKBps: number;
  receivedKBps: number;
  cpuPercent: number;
  rssMB: number;
  heapUsedMB: number;
  heapTotalMB: number;
  loopDelayP99Ms: number;
  loopDelayMaxMs: number;
  totals: { roomsCreated: number; joins: number; sentMB: number; receivedMB: number };
  roomList: (Omit<RoomSample, "sentBytes" | "receivedBytes"> & {
    sentKBps: number;
    receivedKBps: number;
  })[];
}
export interface MonitorSource {
  samples(): RoomSample[];
  sockets(): number;
}

const round = (value: number, digits = 1) => Number(value.toFixed(digits));
const kilobytes = (bytes: number, seconds: number) => round(bytes / 1024 / seconds);
const megabytes = (bytes: number) => round(bytes / 1024 / 1024);
function duration(ms: number): string {
  const seconds = Math.round(ms / 1000);
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}

/**
 * Logs room lifecycle lines and samples load every 10 s for /stats and a once-a-minute
 * summary. Observation only: nothing here feeds back into rooms or the simulation.
 */
export class ServerMonitor {
  latest?: ServerStats;
  private readonly startedMs = Date.now();
  private readonly loopDelay = monitorEventLoopDelay({ resolution: 10 });
  private timer?: ReturnType<typeof setInterval>;
  private sampledMs = Date.now();
  private cpu = process.cpuUsage();
  private samplesSinceSummary = 0;
  private activeSinceSummary = false;
  private totals = { roomsCreated: 0, joins: 0, sentBytes: 0, receivedBytes: 0 };
  constructor(
    private readonly source: MonitorSource,
    private readonly log: (line: string) => void = console.log,
  ) {}
  start(): void {
    this.loopDelay.enable();
    this.timer = setInterval(() => this.sample(), SAMPLE_INTERVAL_MS);
    this.timer.unref();
  }
  stop(): void {
    clearInterval(this.timer);
    this.loopDelay.disable();
  }
  activity(room: string, event: RoomActivity): void {
    this.activeSinceSummary = true;
    if (event.type === "created") {
      this.totals.roomsCreated++;
      this.log(`room ${room} created`);
    } else if (event.type === "joined") {
      this.totals.joins++;
      this.log(`room ${room} player joined (${event.players} connected)`);
    } else if (event.type === "left") {
      this.log(
        `room ${room} player disconnected${event.code ? ` (code ${event.code})` : ""} (${event.players} connected)`,
      );
    } else if (event.code === 1000) {
      this.log(`room ${room} player left (${event.players} connected)`);
    } else {
      this.log(
        `room ${room} server closed a socket: ${event.code} ${event.reason} (${event.players} connected)`,
      );
    }
  }
  ended(room: string, reason: string, ageMs: number): void {
    this.activeSinceSummary = true;
    this.log(`room ${room} ended: ${reason} after ${duration(ageMs)}`);
  }
  sample(): ServerStats {
    const now = Date.now(),
      seconds = Math.max(0.001, (now - this.sampledMs) / 1000),
      cpu = process.cpuUsage(this.cpu),
      memory = process.memoryUsage(),
      samples = this.source.samples();
    this.sampledMs = now;
    this.cpu = process.cpuUsage();
    let sent = 0,
      received = 0;
    for (const room of samples) {
      sent += room.sentBytes;
      received += room.receivedBytes;
    }
    this.totals.sentBytes += sent;
    this.totals.receivedBytes += received;
    const stats: ServerStats = {
      sampledAt: new Date(now).toISOString(),
      windowSeconds: round(seconds),
      uptimeSeconds: Math.round((now - this.startedMs) / 1000),
      rooms: samples.length,
      players: samples.reduce((total, room) => total + room.players, 0),
      sockets: this.source.sockets(),
      sentKBps: kilobytes(sent, seconds),
      receivedKBps: kilobytes(received, seconds),
      cpuPercent: round(((cpu.user + cpu.system) / 1000 / (seconds * 1000)) * 100),
      rssMB: megabytes(memory.rss),
      heapUsedMB: megabytes(memory.heapUsed),
      heapTotalMB: megabytes(memory.heapTotal),
      loopDelayP99Ms: round(this.loopDelay.percentile(99) / 1e6),
      loopDelayMaxMs: round(this.loopDelay.max / 1e6),
      totals: {
        roomsCreated: this.totals.roomsCreated,
        joins: this.totals.joins,
        sentMB: megabytes(this.totals.sentBytes),
        receivedMB: megabytes(this.totals.receivedBytes),
      },
      roomList: samples.map(({ sentBytes, receivedBytes, ...room }) => ({
        ...room,
        debtMs: round(room.debtMs),
        tickAvgMs: round(room.tickAvgMs, 2),
        tickMaxMs: round(room.tickMaxMs, 2),
        sentKBps: kilobytes(sentBytes, seconds),
        receivedKBps: kilobytes(receivedBytes, seconds),
      })),
    };
    this.loopDelay.reset();
    this.latest = stats;
    if (stats.rooms) this.activeSinceSummary = true;
    if (++this.samplesSinceSummary >= SAMPLES_PER_SUMMARY) {
      this.samplesSinceSummary = 0;
      // Quiet servers log one final idle summary, then stay silent until players return.
      if (this.activeSinceSummary) this.summarize(stats);
      this.activeSinceSummary = stats.rooms > 0;
    }
    return stats;
  }
  private summarize(stats: ServerStats): void {
    this.log(
      `stats: ${stats.rooms} rooms, ${stats.players} players, ${stats.sockets} sockets | ` +
        `out ${stats.sentKBps} KB/s, in ${stats.receivedKBps} KB/s | cpu ${stats.cpuPercent}% | ` +
        `rss ${stats.rssMB} MB, heap ${stats.heapUsedMB}/${stats.heapTotalMB} MB | ` +
        `loop delay p99 ${stats.loopDelayP99Ms} ms, max ${stats.loopDelayMaxMs} ms`,
    );
    for (const room of stats.roomList)
      this.log(
        `  ${room.room} ${room.mapMode} ${room.phase} ${room.players}/${room.seats} players ` +
          `${duration(room.timeLeft * 1000)} left ${room.scores.join("-")} | ` +
          `tick avg ${room.tickAvgMs} ms, max ${room.tickMaxMs} ms, debt ${room.debtMs} ms | ` +
          `out ${room.sentKBps} KB/s`,
      );
  }
}
