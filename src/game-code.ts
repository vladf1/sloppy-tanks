// Attach the second half to this instance's memory and function table. The menu
// keeps drawing and accepting choices while the response streams into the compiler.
const DOWNLOAD_IDLE_MS = 20_000;

export async function installGameCode(url: string, primary: WebAssembly.Exports) {
  const controller = new AbortController();
  const stalled = () =>
    setTimeout(
      () => controller.abort(new Error("The gameplay download stalled.")),
      DOWNLOAD_IDLE_MS,
    );
  let timeout = stalled();
  try {
    const response = await fetch(url, { signal: controller.signal });
    if (!response.ok || !response.body) throw new Error(`HTTP ${response.status} for ${url}`);
    const bytes = response.body.pipeThrough(
      new TransformStream<Uint8Array, Uint8Array>({
        transform(chunk, stream) {
          clearTimeout(timeout);
          timeout = stalled();
          stream.enqueue(chunk);
        },
        // Compiling on a slow device can continue after the last byte arrives.
        flush() {
          clearTimeout(timeout);
        },
      }),
    );
    const wasm = new Response(bytes, { headers: { "Content-Type": "application/wasm" } });
    if (WebAssembly.instantiateStreaming) {
      await WebAssembly.instantiateStreaming(wasm, { primary });
    } else {
      await WebAssembly.instantiate(await wasm.arrayBuffer(), { primary });
    }
  } finally {
    clearTimeout(timeout);
  }
}
