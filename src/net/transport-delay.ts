import { DelayedChannel } from "./delayed-channel";

/** Share of messages that model a lost TCP segment when `stall` is set. */
const STALL_CHANCE = 0.02;

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
  const outbound = new DelayedChannel<Delivery>(128);
  const inbound = new DelayedChannel<Delivery>(128);
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
