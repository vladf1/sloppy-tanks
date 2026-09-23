/** Ordered transport delay with bounded storage. Jitter never reorders reliable messages. */
export class DelayedChannel<T> {
  private queue: { at: number; value: T }[] = [];
  constructor(private readonly capacity = 256) {}
  get size(): number {
    return this.queue.length;
  }
  send(value: T, nowMs: number, delayMs: number): void {
    if (this.queue.length >= this.capacity) {
      throw new Error("Delayed channel capacity exceeded");
    }
    if (!Number.isFinite(nowMs) || !Number.isFinite(delayMs) || delayMs < 0) {
      throw new Error("Invalid transport delay");
    }
    this.queue.push({ at: Math.max(nowMs + delayMs, this.queue.at(-1)?.at ?? -Infinity), value });
  }
  receive(nowMs: number): T[] {
    let count = 0;
    while (count < this.queue.length && this.queue[count].at <= nowMs) {
      count++;
    }
    return this.queue.splice(0, count).map((item) => item.value);
  }
  clear(): void {
    this.queue.length = 0;
  }
}
