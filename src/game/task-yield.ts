type YieldingScheduler = { yield?: () => Promise<void> };
type SchedulerHost = { scheduler?: YieldingScheduler };

let active = 0;
let restore = () => {};

/** Run `work` with a message-task `scheduler.yield()` where the browser has none.
 *
 * Three r185 yields several times per object while compiling shaders. Without
 * scheduler.yield (Safari) it waits for requestAnimationFrame each time, which
 * turned the menu's warm-up into ~20 s and stalls entirely in a background tab.
 * A message task yields to input and painting without waiting for a frame. The
 * shim exists only while work runs, so nothing else sees a partial Scheduler. */
export async function withTaskYield<T>(work: () => Promise<T>): Promise<T> {
  const host = globalThis as SchedulerHost;
  if (active === 0) {
    if (host.scheduler?.yield) {
      return work();
    }
    const channel = new MessageChannel();
    const waiting: (() => void)[] = [];
    channel.port1.onmessage = () => waiting.shift()?.();
    const taskYield = () =>
      new Promise<void>((resolve) => {
        waiting.push(resolve);
        channel.port2.postMessage(null);
      });
    const existing = host.scheduler;
    if (existing) {
      existing.yield = taskYield;
    } else {
      host.scheduler = { yield: taskYield };
    }
    restore = () => {
      if (existing) {
        delete existing.yield;
      } else {
        delete host.scheduler;
      }
      channel.port1.close();
    };
  }
  active++;
  try {
    return await work();
  } finally {
    if (--active === 0) {
      restore();
    }
  }
}

/** Resolve in a new message task, so the browser can paint first. Unlike a
 * timer, a message task is not throttled while the tab is in the background. */
export function nextTask(): Promise<void> {
  const { port1, port2 } = new MessageChannel();
  return new Promise((resolve) => {
    port1.onmessage = () => {
      port1.close();
      resolve();
    };
    port2.postMessage(null);
  });
}
