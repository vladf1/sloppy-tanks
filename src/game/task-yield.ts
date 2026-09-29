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

/** How often arena preparation polls a GPU that is still running its warm-up. */
const GPU_POLL_MS = 16;

/** Wait before the next `prepare_step`: a message task while pipelines and textures
 * remain, or a short timer while the GPU compiles and runs the warm-up (`gpuPending`),
 * which can take seconds on a cold shader cache and should not spin the page. */
export function nextPrepareStep(gpuPending: boolean): Promise<void> {
  return gpuPending ? new Promise((resolve) => setTimeout(resolve, GPU_POLL_MS)) : nextTask();
}

/** A hidden tab paints no frames; after this long `afterPaint` stops waiting for one. */
const PAINT_FALLBACK_MS = 100;

/** Resolve just after the browser paints its next frame, so a new choice shows before
 * long synchronous work starts. An animation frame runs right before the paint, and a
 * task it queues runs after it. */
export function afterPaint(): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, PAINT_FALLBACK_MS);
    requestAnimationFrame(() => {
      void nextTask().then(() => {
        clearTimeout(timer);
        resolve();
      });
    });
  });
}
