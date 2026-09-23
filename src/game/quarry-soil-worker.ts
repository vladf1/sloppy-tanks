import { bakeQuarrySoil } from "./quarry-soil";

/** One horizontal band of the quarry soil; see bakeSoil in quarry-terrain.ts. */
self.onmessage = (event: MessageEvent<{ accum: Float32Array; start: number; end: number }>) => {
  const { accum, start, end } = event.data;
  const pixels = bakeQuarrySoil(accum, start, end);
  (self as unknown as Worker).postMessage(pixels, [pixels.buffer]);
};
