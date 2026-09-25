const TRACE_URL = "https://cloudflare.com/cdn-cgi/trace";
let traced: Promise<string> | undefined;

/**
 * The colo this isolate executes in. `request.cf.colo` names where a request entered
 * Cloudflare, which can differ from where a Worker or Durable Object actually runs.
 * An isolate never moves, so one trace per isolate is enough.
 */
export function executionColo(): Promise<string> {
  traced ??= fetch(TRACE_URL)
    .then((response) => response.text())
    .then((text) => /^colo=(\w+)$/m.exec(text)?.[1] ?? "unknown")
    .catch(() => {
      traced = undefined;
      return "unknown";
    });
  return traced;
}

/** Keeps the latest samples so a diagnostic read reports recent behavior, not one value. */
export class SampleWindow {
  private readonly samples: number[] = [];
  constructor(private readonly capacity: number) {}
  add(value: number): void {
    this.samples.push(value);
    if (this.samples.length > this.capacity) this.samples.shift();
  }
  summary(): { count: number; median: number; max: number } {
    const sorted = [...this.samples].sort((a, b) => a - b);
    return {
      count: sorted.length,
      median: sorted[Math.floor(sorted.length / 2)] ?? 0,
      max: sorted.at(-1) ?? 0,
    };
  }
}
