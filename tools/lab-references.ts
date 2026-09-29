// Reference images for the render and effects labs: frames of the lab scenes drawn by
// the game's former Three.js r185 renderer, captured before Three.js left the project.
// They are large, so they live in the ignored `artifacts/references/labs/` (served by
// the dev server); a lab without them still draws and reports its renderer.

/** Where the dev server serves the references from. */
const REFERENCE_DIRECTORY = "artifacts/references/labs/";
/** The difference image amplifies each channel's error by this much. */
const DIFF_GAIN = 4;
/** The per-region error grid is this many cells on a side. */
const GRID = 4;

/** RGBA of a canvas's current frame; read in the task that drew it. */
export function pixels(canvas: HTMLCanvasElement): ImageData {
  const copy = document.createElement("canvas");
  copy.width = canvas.width;
  copy.height = canvas.height;
  const context = copy.getContext("2d", { willReadFrequently: true })!;
  context.drawImage(canvas, 0, 0);
  return context.getImageData(0, 0, canvas.width, canvas.height);
}

/** A reference frame as pixels, or undefined when it was not captured here. */
export async function loadReference(name: string): Promise<ImageData | undefined> {
  const response = await fetch(`${import.meta.env.BASE_URL}${REFERENCE_DIRECTORY}${name}.png`);
  if (!response.ok || !response.headers.get("content-type")?.startsWith("image/")) {
    return undefined;
  }
  const bitmap = await createImageBitmap(await response.blob());
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const context = canvas.getContext("2d", { willReadFrequently: true })!;
  context.drawImage(bitmap, 0, 0);
  return context.getImageData(0, 0, bitmap.width, bitmap.height);
}

export interface Comparison {
  /** False when no reference of this size exists; only the renderer's mean is reported. */
  reference: boolean;
  meanAbsDiff?: number;
  maxAbsDiff?: number;
  rustMeanRgb: number[];
  referenceMeanRgb?: number[];
  /** Mean error per region, row by row. */
  cellMeanDiff?: number[];
}

/** Compare a frame with its reference, drawing the amplified difference into `diff`. */
export function compareImages(
  image: ImageData,
  reference: ImageData | undefined,
  diff: HTMLCanvasElement,
): Comparison {
  const count = image.data.length / 4;
  const mean = (data: Uint8ClampedArray) =>
    [0, 1, 2].map((c) => {
      let sum = 0;
      for (let i = c; i < data.length; i += 4) sum += data[i];
      return sum / count;
    });
  const context = diff.getContext("2d")!;
  if (!reference || reference.width !== image.width || reference.height !== image.height) {
    context.clearRect(0, 0, diff.width, diff.height);
    return { reference: false, rustMeanRgb: mean(image.data) };
  }
  const output = new ImageData(image.width, image.height);
  const cells = Array.from({ length: GRID * GRID }, () => ({ sum: 0, count: 0 }));
  let sum = 0;
  let max = 0;
  for (let i = 0; i < image.data.length; i += 4) {
    let pixel = 0;
    for (let c = 0; c < 3; c++) {
      const delta = Math.abs(image.data[i + c] - reference.data[i + c]);
      pixel += delta;
      output.data[i + c] = Math.min(255, delta * DIFF_GAIN);
    }
    output.data[i + 3] = 255;
    pixel /= 3;
    sum += pixel;
    max = Math.max(max, pixel);
    const p = i / 4;
    const x = Math.floor(((p % image.width) / image.width) * GRID);
    const y = Math.floor((Math.floor(p / image.width) / image.height) * GRID);
    cells[y * GRID + x].sum += pixel;
    cells[y * GRID + x].count++;
  }
  context.putImageData(output, 0, 0);
  return {
    reference: true,
    meanAbsDiff: sum / count,
    maxAbsDiff: max,
    rustMeanRgb: mean(image.data),
    referenceMeanRgb: mean(reference.data),
    cellMeanDiff: cells.map((cell) => Math.round((cell.sum / cell.count) * 10) / 10),
  };
}
