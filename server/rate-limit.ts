/** Most keys (client IPs) tracked at once, so hostile IP churn cannot grow memory unbounded. */
export const MAX_RATE_LIMIT_KEYS = 10_000;

/**
 * Fixed-window counter: at most `limit` calls per key in each `periodMs` window. When
 * every tracked key is still inside its window, new keys are refused until one expires:
 * under a flood of more distinct IPs than the cap, failing closed beats unbounded memory.
 */
export class RateLimit {
  private windows = new Map<string, { startMs: number; count: number }>();
  /** Start of the oldest window left after the last sweep; nothing expires before then. */
  private oldestStartMs = Infinity;
  constructor(
    private readonly limit: number,
    private readonly periodMs = 60_000,
    private readonly maxKeys = MAX_RATE_LIMIT_KEYS,
  ) {}
  allow(key: string, nowMs: number): boolean {
    let window = this.windows.get(key);
    if (!window) {
      if (this.windows.size >= this.maxKeys && !this.sweep(nowMs)) return false;
      window = { startMs: nowMs, count: 0 };
      this.windows.set(key, window);
      this.oldestStartMs = Math.min(this.oldestStartMs, nowMs);
    } else if (nowMs - window.startMs >= this.periodMs) {
      window.startMs = nowMs;
      window.count = 0;
    }
    return ++window.count <= this.limit;
  }
  /** Drops expired windows; true when that made room. Scans at most once per expiry. */
  private sweep(nowMs: number): boolean {
    if (nowMs - this.oldestStartMs < this.periodMs) return false;
    this.oldestStartMs = Infinity;
    for (const [key, window] of this.windows) {
      if (nowMs - window.startMs >= this.periodMs) this.windows.delete(key);
      else this.oldestStartMs = Math.min(this.oldestStartMs, window.startMs);
    }
    return this.windows.size < this.maxKeys;
  }
}
