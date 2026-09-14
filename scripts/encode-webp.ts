import { execFileSync } from "node:child_process";
import type { Canvas } from "@napi-rs/canvas";

// Preserve every RGBA value, including RGB under transparent pixels.
export function encodeWebp(canvas: Canvas): Buffer {
  return execFileSync("cwebp", ["-quiet", "-lossless", "-exact", "-z", "9", "-o", "-", "--", "-"], {
    input: canvas.toBuffer("image/png"),
    maxBuffer: 32 * 1024 * 1024,
  });
}
