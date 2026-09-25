/** Keys kept before expired windows are swept, so hostile IP churn cannot grow memory unbounded. */
const SWEEP_AT_KEYS = 10_000;

/** Fixed-window counter: at most `limit` calls per key in each `periodMs` window. */
export class RateLimit {
  private windows = new Map<string, { startMs: number; count: number }>();
  constructor(
    private readonly limit: number,
    private readonly periodMs = 60_000,
  ) {}
  allow(key: string, nowMs: number): boolean {
    if (this.windows.size >= SWEEP_AT_KEYS) {
      for (const [stale, window] of this.windows)
        if (nowMs - window.startMs >= this.periodMs) this.windows.delete(stale);
    }
    let window = this.windows.get(key);
    if (!window || nowMs - window.startMs >= this.periodMs) {
      window = { startMs: nowMs, count: 0 };
      this.windows.set(key, window);
    }
    return ++window.count <= this.limit;
  }
}
