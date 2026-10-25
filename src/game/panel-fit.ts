/** Battle Setup is laid out for a screen about 900 CSS pixels tall. A shorter laptop
 * screen (1280×800, 1366×768, 1280×720) scales the whole panel down to fit instead of
 * cutting off GO below the fold; phones lay out and scroll their own edition. */

/** The overlay's top padding plus a little room under the panel. */
const SCREEN_MARGIN = 34;
/** Below this the text would be too small to read; the overlay scrolls instead. */
const MIN_SCALE = 0.62;

/** Scale every Battle Setup panel under `root` to the window's height. */
export function fitPanels(root: ParentNode = document): void {
  if (document.documentElement.classList.contains("phone")) {
    return;
  }
  for (const panel of root.querySelectorAll<HTMLElement>(".menu.start")) {
    panel.style.removeProperty("zoom");
    const height = panel.offsetHeight;
    if (!height) {
      continue;
    }
    const scale = Math.min(1, (window.innerHeight - SCREEN_MARGIN) / height);
    if (scale < 0.999) {
      panel.style.setProperty("zoom", String(Math.max(MIN_SCALE, scale)));
    }
  }
}

let watching = false;

/** Refit the panels whenever the window changes size. */
export function keepPanelsFitted(): void {
  if (watching) {
    return;
  }
  watching = true;
  let queued = false;
  window.addEventListener("resize", () => {
    if (queued) {
      return;
    }
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      fitPanels();
    });
  });
}
