// Reads and writes the geometry cache's files (`geometry-store.ts`) off the page's main
// thread, for `geometry-cache.ts`: the bytes it reads move to the page (transferred,
// not copied), and those the page stores move here.

import { readFile, writeFile } from "./geometry-store.ts";

export type WorkerRequest =
  | { op: "read"; name: string }
  | { op: "write"; name: string; version: string; buffer: ArrayBuffer };

export type WorkerReply =
  | { op: "read"; name: string; buffer: ArrayBuffer | null }
  | { op: "write"; name: string; error?: string };

onmessage = async ({ data }: MessageEvent<WorkerRequest>) => {
  if (data.op === "read") {
    const buffer = await readFile(data.name).catch(() => null);
    const reply: WorkerReply = { op: "read", name: data.name, buffer };
    postMessage(reply, { transfer: buffer ? [buffer] : [] });
  } else {
    const reply: WorkerReply = { op: "write", name: data.name };
    await writeFile(data.name, data.version, data.buffer).catch((error) => {
      reply.error = String(error);
    });
    postMessage(reply);
  }
};
