/** Shared with the lightweight menu without importing the renderer. */
export class GraphicsUnavailableError extends Error {
  constructor(cause: unknown) {
    super(
      "Neither WebGPU nor WebGL2 is available. Use a browser with hardware acceleration enabled.",
      { cause },
    );
    this.name = "GraphicsUnavailableError";
  }
}

export function startupErrorMessage(error: unknown, fallback: string): string {
  // The inline menu and lazy engine are separate builds, so their copies of an
  // application error class do not share constructor identity.
  return error instanceof Error && error.name === "GraphicsUnavailableError"
    ? error.message
    : fallback;
}
