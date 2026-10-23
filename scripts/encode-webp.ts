import { execFileSync } from "node:child_process";
import type { Canvas } from "@napi-rs/canvas";

// Preserve every RGBA value, including RGB under transparent pixels.
function cwebpLossless(input: Buffer): Buffer {
  return execFileSync("cwebp", ["-quiet", "-lossless", "-exact", "-z", "9", "-o", "-", "--", "-"], {
    input,
    maxBuffer: 32 * 1024 * 1024,
  });
}

export function encodeWebp(canvas: Canvas): Buffer {
  return cwebpLossless(canvas.toBuffer("image/png"));
}

/** Colour artwork, not data: lossy at `quality`, with the sharper RGB to YUV
 * conversion so saturated edges keep their colour. A noisy painted surface packs
 * several times smaller than losslessly, and the difference is gone once mipmapped. */
export function encodeLossyWebp(canvas: Canvas, quality: number): Buffer {
  const args = ["-quiet", "-q", String(quality), "-m", "6", "-sharp_yuv", "-metadata", "none"];
  return execFileSync("cwebp", [...args, "-o", "-", "--", "-"], {
    input: canvas.toBuffer("image/png"),
    maxBuffer: 32 * 1024 * 1024,
  });
}

/** Raw RGBA pixels, unpremultiplied, which a canvas would premultiply on the way out. */
export function encodePixelsWebp(data: Uint8ClampedArray, width: number, height: number): Buffer {
  const header = `P7\nWIDTH ${width}\nHEIGHT ${height}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n`;
  return cwebpLossless(Buffer.concat([Buffer.from(header, "ascii"), Buffer.from(data.buffer)]));
}
