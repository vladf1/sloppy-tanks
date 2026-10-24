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

/** The inline menu and lazy engine are separate builds, so their copies of an
 * application error class do not share constructor identity. */
function graphicsUnavailable(error: unknown): boolean {
  return error instanceof Error && error.name === "GraphicsUnavailableError";
}

export function startupErrorMessage(error: unknown, fallback: string): string {
  return graphicsUnavailable(error) && error instanceof Error ? error.message : fallback;
}

/** What to do next: a missing GPU is a browser setting, anything else is worth a retry. */
export function startupErrorHint(error: unknown): string {
  return graphicsUnavailable(error)
    ? "Turn on hardware acceleration in your browser's settings, then reload."
    : "Check your connection, then try again.";
}

/** A load that failed before any menu could run: one dialog in the menu's style, with
 * the reason and a reload, in place of the page. Text only, never markup. */
export function showFatalError(root: HTMLElement, title: string, message: string): void {
  const overlay = document.createElement("div");
  overlay.id = "startup-overlay";
  overlay.dataset.state = "error";
  const dialog = document.createElement("section");
  dialog.className = "menu compact fatal-error";
  dialog.setAttribute("role", "alertdialog");
  dialog.setAttribute("aria-labelledby", "fatal-title");
  dialog.setAttribute("aria-describedby", "fatal-message");
  const head = document.createElement("header");
  head.className = "dialog-head";
  const eyebrow = document.createElement("span");
  eyebrow.className = "eyebrow";
  eyebrow.textContent = "SLOPPY TANKS";
  const heading = document.createElement("h2");
  heading.id = "fatal-title";
  heading.textContent = title;
  const lede = document.createElement("p");
  lede.id = "fatal-message";
  lede.className = "dialog-lede";
  lede.textContent = message;
  head.append(eyebrow, heading, lede);
  const actions = document.createElement("div");
  actions.className = "menu-actions";
  const reload = document.createElement("button");
  reload.className = "primary";
  reload.type = "button";
  reload.textContent = "RELOAD";
  reload.addEventListener("click", () => location.reload());
  actions.append(reload);
  dialog.append(head, actions);
  overlay.append(dialog);
  root.replaceChildren(overlay);
  reload.focus();
}
