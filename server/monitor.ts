import { monitorEventLoopDelay, performance, PerformanceObserver } from "node:perf_hooks";
import type { RoomActivity, RoomSample } from "./room-session";

/** How often the monitor takes a live reading for the dashboard. */
export const READING_INTERVAL_MS = 1000;
/** Readings per /stats sample (ten seconds). */
const READINGS_PER_SAMPLE = 10;
/** Samples between summary log lines while rooms are active (one minute). */
const SAMPLES_PER_SUMMARY = 6;
/** Live readings kept for the dashboard charts (five minutes). */
export const HISTORY_READINGS = 300;
/** Lifecycle events kept for the dashboard log. */
export const RECENT_EVENTS = 100;
/** Event-loop delay sampling interval; the histogram's idle floor is this long. */
const LOOP_DELAY_RESOLUTION_MS = 10;

/** One room's state with its load as rates. */
export type RoomLoad = Omit<
  RoomSample,
  "sentBytes" | "receivedBytes" | "ticks" | "sentMessages" | "receivedMessages"
> & {
  sentKBps: number;
  receivedKBps: number;
};
export interface ServerStats {
  sampledAt: string;
  windowSeconds: number;
  uptimeSeconds: number;
  rooms: number;
  players: number;
  sockets: number;
  sentKBps: number;
  receivedKBps: number;
  wireSentKBps: number;
  wireReceivedKBps: number;
  cpuPercent: number;
  rssMB: number;
  heapUsedMB: number;
  heapTotalMB: number;
  loopDelayP99Ms: number;
  loopDelayMaxMs: number;
  totals: {
    roomsCreated: number;
    joins: number;
    sentMB: number;
    receivedMB: number;
    wireSentMB: number;
    wireReceivedMB: number;
  };
  roomList: RoomLoad[];
}
/** One second of process and room load for the dashboard. */
export interface LiveReading {
  atMs: number;
  rooms: number;
  players: number;
  sockets: number;
  cpuPercent: number;
  /** Share of the second the event loop spent running callbacks rather than waiting. */
  loopBusyPercent: number;
  /** How late the loop's 10 ms probe timer ran, excluding the interval itself. */
  loopLagP50Ms: number;
  loopLagP90Ms: number;
  loopLagP99Ms: number;
  loopLagMaxMs: number;
  /** Garbage-collection pause time in this second. */
  gcMs: number;
  rssMB: number;
  heapUsedMB: number;
  heapTotalMB: number;
  /** Room messages before WebSocket compression, as UTF-16 lengths. */
  sentKBps: number;
  receivedKBps: number;
  /** Bytes through the sockets after compression, including frame and handshake bytes. */
  wireSentKBps: number;
  wireReceivedKBps: number;
  /** Messages per second by type. */
  sentMessages: Record<string, number>;
  receivedMessages: Record<string, number>;
  /** Slowest room timer callback in this second. */
  tickMaxMs: number;
  roomList: RoomLoad[];
}
/** A reading as kept in the chart history, without its room list. */
export type LivePoint = Omit<LiveReading, "roomList">;
export interface MonitorEvent {
  /** Increases by one per event, so a reader can ask for what it has not seen. */
  id: number;
  atMs: number;
  room: string;
  message: string;
}
export interface MonitorSource {
  samples(): RoomSample[];
  sockets(): number;
  /** Bytes written to and read from room sockets since the server started. */
  wireBytes(): { sent: number; received: number };
}
interface RoomWindow {
  sentBytes: number;
  receivedBytes: number;
  ticks: number;
  tickTotalMs: number;
  tickMaxMs: number;
}

const round = (value: number, digits = 1) => Number(value.toFixed(digits));
const kilobytes = (bytes: number, seconds: number) => round(bytes / 1024 / seconds);
const megabytes = (bytes: number) => round(bytes / 1024 / 1024);
const cpuMicros = (usage: NodeJS.CpuUsage) => usage.user + usage.system;
const cpuPercent = (micros: number, seconds: number) => round(micros / 1e4 / seconds);
function addCounts(total: Record<string, number>, counts: Record<string, number>): void {
  for (const [type, count] of Object.entries(counts)) total[type] = (total[type] ?? 0) + count;
}
const perSecond = (counts: Record<string, number>, seconds: number) =>
  Object.fromEntries(Object.entries(counts).map(([type, count]) => [type, round(count / seconds)]));
function duration(ms: number): string {
  const seconds = Math.round(ms / 1000);
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}
function describe(event: RoomActivity): string {
  if (event.type === "created") return "created";
  if (event.type === "joined") return `player joined (${event.players} connected)`;
  if (event.type === "left")
    return `player disconnected${event.code ? ` (code ${event.code})` : ""} (${event.players} connected)`;
  if (event.code === 1000) return `player left (${event.players} connected)`;
  return `server closed a socket: ${event.code} ${event.reason} (${event.players} connected)`;
}

/**
 * Reads room and process load every second for the dashboard, and sums ten readings into
 * each /stats sample and six samples into a once-a-minute summary. Also logs room
 * lifecycle lines. Observation only: nothing here feeds back into rooms or the simulation.
 */
export class ServerMonitor {
  latest?: ServerStats;
  /** Recent readings, oldest first. */
  readonly history: LivePoint[] = [];
  /** Recent lifecycle events, oldest first. */
  readonly events: MonitorEvent[] = [];
  readonly startedMs = Date.now();
  private readonly listeners = new Set<(reading: LiveReading) => void>();
  // Delay histograms cannot be merged, so the one-second reading and the ten-second
  // sample each keep their own.
  private readonly readingDelay = monitorEventLoopDelay({ resolution: LOOP_DELAY_RESOLUTION_MS });
  private readonly sampleDelay = monitorEventLoopDelay({ resolution: LOOP_DELAY_RESOLUTION_MS });
  private readonly gc = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) this.gcMs += entry.duration;
  });
  private gcMs = 0;
  private timer?: ReturnType<typeof setInterval>;
  private readings = 0;
  private readMs = Date.now();
  private readCpu = process.cpuUsage();
  private readLoop = performance.eventLoopUtilization();
  private readWire = { sent: 0, received: 0 };
  private sampledMs = Date.now();
  private sampleCpu = process.cpuUsage();
  /** Load per room and in total since the last sample, summed from readings. */
  private window = new Map<string, RoomWindow>();
  private windowBytes = { sent: 0, received: 0, wireSent: 0, wireReceived: 0 };
  private samplesSinceSummary = 0;
  private activeSinceSummary = false;
  private counts = {
    roomsCreated: 0,
    joins: 0,
    sentBytes: 0,
    receivedBytes: 0,
    wireSentBytes: 0,
    wireReceivedBytes: 0,
  };
  constructor(
    private readonly source: MonitorSource,
    private readonly log: (line: string) => void = console.log,
  ) {}
  start(): void {
    this.readingDelay.enable();
    this.sampleDelay.enable();
    this.gc.observe({ entryTypes: ["gc"] });
    this.timer = setInterval(
      () => (++this.readings % READINGS_PER_SAMPLE ? this.read() : this.sample()),
      READING_INTERVAL_MS,
    );
    this.timer.unref();
    // The dashboard log always has at least one line, even on a server nobody has joined.
    this.addEvent("", "server started");
  }
  stop(): void {
    clearInterval(this.timer);
    this.readingDelay.disable();
    this.sampleDelay.disable();
    this.gc.disconnect();
    this.listeners.clear();
  }
  /** Calls `listener` with every reading until the returned function is called. */
  subscribe(listener: (reading: LiveReading) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  activity(room: string, event: RoomActivity): void {
    if (event.type === "created") this.counts.roomsCreated++;
    else if (event.type === "joined") this.counts.joins++;
    this.record(room, describe(event));
  }
  ended(room: string, reason: string, ageMs: number): void {
    this.record(room, `ended: ${reason} after ${duration(ageMs)}`);
  }
  totals(): ServerStats["totals"] {
    return {
      roomsCreated: this.counts.roomsCreated,
      joins: this.counts.joins,
      sentMB: megabytes(this.counts.sentBytes),
      receivedMB: megabytes(this.counts.receivedBytes),
      wireSentMB: megabytes(this.counts.wireSentBytes),
      wireReceivedMB: megabytes(this.counts.wireReceivedBytes),
    };
  }
  /** Takes the load since the previous reading; the timer calls it every second. */
  read(): LiveReading {
    const now = Date.now(),
      seconds = Math.max(0.001, (now - this.readMs) / 1000),
      cpu = process.cpuUsage(),
      loop = performance.eventLoopUtilization(),
      memory = process.memoryUsage(),
      samples = this.source.samples(),
      wire = this.source.wireBytes(),
      wireSent = wire.sent - this.readWire.sent,
      wireReceived = wire.received - this.readWire.received,
      sentMessages: Record<string, number> = {},
      receivedMessages: Record<string, number> = {};
    let sent = 0,
      received = 0,
      tickMaxMs = 0;
    for (const room of samples) {
      sent += room.sentBytes;
      received += room.receivedBytes;
      addCounts(sentMessages, room.sentMessages);
      addCounts(receivedMessages, room.receivedMessages);
      tickMaxMs = Math.max(tickMaxMs, room.tickMaxMs);
      const window = this.window.get(room.room) ?? {
        sentBytes: 0,
        receivedBytes: 0,
        ticks: 0,
        tickTotalMs: 0,
        tickMaxMs: 0,
      };
      window.sentBytes += room.sentBytes;
      window.receivedBytes += room.receivedBytes;
      window.ticks += room.ticks;
      window.tickTotalMs += room.tickAvgMs * room.ticks;
      window.tickMaxMs = Math.max(window.tickMaxMs, room.tickMaxMs);
      this.window.set(room.room, window);
    }
    this.windowBytes.sent += sent;
    this.windowBytes.received += received;
    this.windowBytes.wireSent += wireSent;
    this.windowBytes.wireReceived += wireReceived;
    this.counts.sentBytes += sent;
    this.counts.receivedBytes += received;
    this.counts.wireSentBytes += wireSent;
    this.counts.wireReceivedBytes += wireReceived;
    // An idle histogram has no samples and reports zero rather than the interval.
    const lag = (nanoseconds: number) =>
      this.readingDelay.count
        ? round(Math.max(0, nanoseconds / 1e6 - LOOP_DELAY_RESOLUTION_MS))
        : 0;
    const reading: LiveReading = {
      atMs: now,
      rooms: samples.length,
      players: samples.reduce((total, room) => total + room.players, 0),
      sockets: this.source.sockets(),
      cpuPercent: cpuPercent(cpuMicros(cpu) - cpuMicros(this.readCpu), seconds),
      loopBusyPercent: round(
        performance.eventLoopUtilization(loop, this.readLoop).utilization * 100,
      ),
      loopLagP50Ms: lag(this.readingDelay.percentile(50)),
      loopLagP90Ms: lag(this.readingDelay.percentile(90)),
      loopLagP99Ms: lag(this.readingDelay.percentile(99)),
      loopLagMaxMs: lag(this.readingDelay.max),
      gcMs: round(this.gcMs),
      rssMB: megabytes(memory.rss),
      heapUsedMB: megabytes(memory.heapUsed),
      heapTotalMB: megabytes(memory.heapTotal),
      sentKBps: kilobytes(sent, seconds),
      receivedKBps: kilobytes(received, seconds),
      wireSentKBps: kilobytes(wireSent, seconds),
      wireReceivedKBps: kilobytes(wireReceived, seconds),
      sentMessages: perSecond(sentMessages, seconds),
      receivedMessages: perSecond(receivedMessages, seconds),
      tickMaxMs: round(tickMaxMs, 2),
      // Message counts stay server-wide; the dashboard charts them by type.
      roomList: samples.map(
        ({
          sentBytes,
          receivedBytes,
          ticks: _,
          sentMessages: _s,
          receivedMessages: _r,
          ...room
        }) => ({
          ...room,
          debtMs: round(room.debtMs),
          tickAvgMs: round(room.tickAvgMs, 2),
          tickMaxMs: round(room.tickMaxMs, 2),
          sentKBps: kilobytes(sentBytes, seconds),
          receivedKBps: kilobytes(receivedBytes, seconds),
        }),
      ),
    };
    this.readMs = now;
    this.readWire = wire;
    this.readCpu = cpu;
    this.readLoop = loop;
    this.gcMs = 0;
    this.readingDelay.reset();
    const { roomList: _, ...point } = reading;
    this.history.push(point);
    if (this.history.length > HISTORY_READINGS) this.history.shift();
    for (const listener of this.listeners) listener(reading);
    return reading;
  }
  /** Takes a reading and returns the /stats figures since the previous sample. */
  sample(): ServerStats {
    const reading = this.read(),
      seconds = Math.max(0.001, (reading.atMs - this.sampledMs) / 1000);
    const stats: ServerStats = {
      sampledAt: new Date(reading.atMs).toISOString(),
      windowSeconds: round(seconds),
      uptimeSeconds: Math.round((reading.atMs - this.startedMs) / 1000),
      rooms: reading.rooms,
      players: reading.players,
      sockets: reading.sockets,
      sentKBps: kilobytes(this.windowBytes.sent, seconds),
      receivedKBps: kilobytes(this.windowBytes.received, seconds),
      wireSentKBps: kilobytes(this.windowBytes.wireSent, seconds),
      wireReceivedKBps: kilobytes(this.windowBytes.wireReceived, seconds),
      cpuPercent: cpuPercent(cpuMicros(this.readCpu) - cpuMicros(this.sampleCpu), seconds),
      rssMB: reading.rssMB,
      heapUsedMB: reading.heapUsedMB,
      heapTotalMB: reading.heapTotalMB,
      loopDelayP99Ms: round(this.sampleDelay.percentile(99) / 1e6),
      loopDelayMaxMs: round(this.sampleDelay.max / 1e6),
      totals: this.totals(),
      roomList: reading.roomList.map((room) => {
        const window = this.window.get(room.room)!;
        return {
          ...room,
          tickAvgMs: round(window.ticks ? window.tickTotalMs / window.ticks : 0, 2),
          tickMaxMs: round(window.tickMaxMs, 2),
          sentKBps: kilobytes(window.sentBytes, seconds),
          receivedKBps: kilobytes(window.receivedBytes, seconds),
        };
      }),
    };
    this.sampledMs = reading.atMs;
    this.sampleCpu = this.readCpu;
    this.sampleDelay.reset();
    this.window.clear();
    this.windowBytes = { sent: 0, received: 0, wireSent: 0, wireReceived: 0 };
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
  private record(room: string, message: string): void {
    this.activeSinceSummary = true;
    this.addEvent(room, message);
    this.log(`room ${room} ${message}`);
  }
  private addEvent(room: string, message: string): void {
    this.events.push({ id: (this.events.at(-1)?.id ?? 0) + 1, atMs: Date.now(), room, message });
    if (this.events.length > RECENT_EVENTS) this.events.shift();
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
