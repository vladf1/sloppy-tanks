/** Shared with the lightweight menu without importing the renderer. */
export class WebGPUUnavailableError extends Error {
  constructor(cause: unknown) {
    super("WebGPU is unavailable. Use a browser with WebGPU and hardware acceleration enabled.", {
      cause,
    });
    this.name = "WebGPUUnavailableError";
  }
}

export function startupErrorMessage(error: unknown, fallback: string): string {
  // The inline menu and lazy engine are separate builds, so their copies of an
  // application error class do not share constructor identity.
  return error instanceof Error && error.name === "WebGPUUnavailableError"
    ? error.message
    : fallback;
}
