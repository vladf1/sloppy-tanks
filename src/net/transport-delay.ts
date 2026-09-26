/** Share of messages that model a lost TCP segment when `stall` is set. */
const STALL_CHANCE = 0.02;
const CHANNEL_CAPACITY = 128;

/** Ordered transport delay with bounded storage. Jitter never reorders reliable messages. */
export class DelayedChannel<T> {
  private queue: { at: number; value: T }[] = [];
  constructor(private readonly capacity = CHANNEL_CAPACITY) {}
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

/**
 * Development-only, ordered half-RTT delay. No per-packet timers survive a reconnect.
 * `stall` holds an occasional message for that many milliseconds; later messages queue
 * behind it, as head-of-line blocking does after a TCP retransmission.
 */
export function transportDelay(params: URLSearchParams) {
  const half = Math.max(0, Math.min(300, Number(params.get("latency")) || 0)) / 2;
  const jitter = Math.max(0, Math.min(30, Number(params.get("jitter")) || 0));
  const stall = Math.max(0, Math.min(500, Number(params.get("stall")) || 0));
  type Delivery = { text: string; emit: (text: string) => void };
  const outbound = new DelayedChannel<Delivery>();
  const inbound = new DelayedChannel<Delivery>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const pump = () => {
    timer = undefined;
    for (const channel of [outbound, inbound]) {
      for (const delivery of channel.receive(performance.now())) {
        delivery.emit(delivery.text);
      }
    }
    if (outbound.size || inbound.size) {
      timer = setTimeout(pump, 4);
    }
  };
  const queue = (channel: DelayedChannel<Delivery>, text: string, emit: (text: string) => void) => {
    const stalled = stall && Math.random() < STALL_CHANCE ? stall : 0;
    channel.send({ text, emit }, performance.now(), half + Math.random() * jitter + stalled);
    timer ??= setTimeout(pump, 4);
  };
  return {
    send: (text: string, emit: (text: string) => void) => queue(outbound, text, emit),
    receive: (text: string, emit: (text: string) => void) => queue(inbound, text, emit),
    clear() {
      clearTimeout(timer);
      timer = undefined;
      outbound.clear();
      inbound.clear();
    },
  };
}
