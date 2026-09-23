import { DelayedChannel } from "./delayed-channel";
/** Development-only, ordered half-RTT delay. No per-packet timers survive a reconnect. */
export function transportDelay(params: URLSearchParams) {
  const half = Math.max(0, Math.min(300, Number(params.get("latency")) || 0)) / 2;
  const jitter = Math.max(0, Math.min(30, Number(params.get("jitter")) || 0));
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
    channel.send({ text, emit }, performance.now(), half + Math.random() * jitter);
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
