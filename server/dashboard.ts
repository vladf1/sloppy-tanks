import { readFileSync } from "node:fs";
import type { ServerResponse } from "node:http";
import os from "node:os";
import { CONTENT_VERSION, PROTOCOL_VERSION } from "../src/net/protocol";
import {
  HISTORY_READINGS,
  READING_INTERVAL_MS,
  type LiveReading,
  type MonitorEvent,
  type ServerMonitor,
} from "./monitor";

declare const __DASHBOARD_PAGE__: string;
/** The server bundle inlines the page; source runs (tests, tsx) read it beside this module. */
const PAGE =
  typeof __DASHBOARD_PAGE__ === "undefined"
    ? readFileSync(new URL("./dashboard.html", import.meta.url), "utf8")
    : __DASHBOARD_PAGE__;

/** Open dashboard streams per process; each costs one small write per second. */
export const MAX_DASHBOARD_VIEWERS = 10;
/** Unread output after which a viewer is dropped; one second's update is a few KB. */
const MAX_VIEWER_BACKLOG_BYTES = 256 * 1024;
/** Room code characters shown: enough to tell rooms apart, far too few to join one. */
const SHOWN_CODE_CHARS = 3;
const MB = 1024 * 1024;
const PAGE_HEADERS = {
  "Content-Type": "text/html; charset=utf-8",
  "Cache-Control": "no-store",
  "Content-Security-Policy":
    // uPlot comes from jsDelivr, pinned by version and subresource integrity in the page.
    "default-src 'none'; script-src 'unsafe-inline' https://cdn.jsdelivr.net; " +
    "style-src 'unsafe-inline' https://cdn.jsdelivr.net; " +
    "connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
  "X-Robots-Tag": "noindex",
};

export const maskRoom = (code: string) =>
  code.slice(0, SHOWN_CODE_CHARS) + "•".repeat(Math.max(0, code.length - SHOWN_CODE_CHARS));
const publicEvent = (event: MonitorEvent) => ({ ...event, room: maskRoom(event.room) });
const frame = (type: string, data: unknown) => `event: ${type}\ndata: ${JSON.stringify(data)}\n\n`;

/**
 * The public, read-only /dashboard page and its Server-Sent Events stream of the monitor's
 * one-second readings. A room code is the key to join a room, so codes are masked here;
 * full codes stay in the loopback-only /stats and the journal. Nothing identifies players.
 */
export class Dashboard {
  private readonly viewers = new Set<ServerResponse>();
  private readonly unsubscribe: () => void;
  private latest?: ReturnType<Dashboard["publicReading"]>;
  private sentEventId = 0;
  constructor(
    private readonly monitor: ServerMonitor,
    private readonly limits: { maxRooms: number },
  ) {
    this.unsubscribe = monitor.subscribe((reading) => this.broadcast(reading));
  }
  page(response: ServerResponse): void {
    response.writeHead(200, PAGE_HEADERS);
    response.end(PAGE);
  }
  /** Starts a stream with the recent history; false when every viewer slot is taken. */
  stream(response: ServerResponse): boolean {
    if (this.viewers.size >= MAX_DASHBOARD_VIEWERS) return false;
    response.writeHead(200, { "Content-Type": "text/event-stream", "Cache-Control": "no-store" });
    this.viewers.add(response);
    response.on("close", () => this.viewers.delete(response));
    response.write(
      frame("hello", {
        server: {
          contentVersion: CONTENT_VERSION,
          protocolVersion: PROTOCOL_VERSION,
          node: process.version,
          startedAtMs: this.monitor.startedMs,
          maxRooms: this.limits.maxRooms,
          cpus: os.availableParallelism(),
          hostMemoryMB: Math.round(os.totalmem() / MB),
          readingIntervalMs: READING_INTERVAL_MS,
          historyReadings: HISTORY_READINGS,
        },
        history: this.monitor.history,
        events: this.monitor.events.map(publicEvent),
        reading: this.latest,
      }),
    );
    return true;
  }
  /** Ends every stream; viewers' pages keep retrying until the server is back. */
  close(): void {
    this.unsubscribe();
    for (const viewer of this.viewers) viewer.end();
    this.viewers.clear();
  }
  private broadcast(reading: LiveReading): void {
    const events = this.monitor.events.filter((event) => event.id > this.sentEventId);
    this.sentEventId = this.monitor.events.at(-1)?.id ?? this.sentEventId;
    this.latest = this.publicReading(reading);
    if (!this.viewers.size) return;
    const message = frame("reading", { ...this.latest, events: events.map(publicEvent) });
    for (const viewer of this.viewers) {
      if (viewer.writableLength > MAX_VIEWER_BACKLOG_BYTES) viewer.destroy();
      else viewer.write(message);
    }
  }
  private publicReading(reading: LiveReading) {
    return {
      ...reading,
      roomList: reading.roomList.map((room) => ({ ...room, room: maskRoom(room.room) })),
      totals: this.monitor.totals(),
      viewers: this.viewers.size,
      hostLoad: Number(os.loadavg()[0].toFixed(2)),
      hostMemoryUsedMB: Math.round((os.totalmem() - os.freemem()) / MB),
    };
  }
}
