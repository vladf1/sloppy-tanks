/** Battle Setup is laid out for a screen about 900 CSS pixels tall. A shorter laptop
 * screen (1280×800, 1366×768, 1280×720) scales the whole panel down to fit instead of
 * cutting off GO below the fold; phones lay out and scroll their own edition. */

/** The overlay's top padding plus a little room under the panel. */
const SCREEN_MARGIN = 34;
/** Below this the text would be too small to read; the overlay scrolls instead. */
const MIN_SCALE = 0.62;

/** Clear space kept between the build label and the panel, in CSS pixels. */
const BUILD_LABEL_GAP = 8;

/** Scale every Battle Setup panel under `root` to the window's height. */
export function fitPanels(root: ParentNode = document): void {
  if (document.documentElement.classList.contains("phone")) {
    return;
  }
  for (const panel of root.querySelectorAll<HTMLElement>(".menu.start")) {
    panel.style.removeProperty("zoom");
    panel.style.removeProperty("--panel-zoom");
    const height = panel.offsetHeight;
    if (!height) {
      continue;
    }
    const scale = Math.min(1, (window.innerHeight - SCREEN_MARGIN) / height);
    if (scale < 0.999) {
      const zoom = String(Math.max(MIN_SCALE, scale));
      panel.style.setProperty("zoom", zoom);
      // The build label counters the zoom to keep its size (it is fixed to the screen).
      panel.style.setProperty("--panel-zoom", zoom);
    }
    fitBuildLabel(panel);
  }
}

/** The build label sits in the screen's bottom-right corner, outside the panel; where the
 * panel leaves that corner no room (a short or narrow window), the label is hidden. */
function fitBuildLabel(panel: HTMLElement): void {
  const label = panel.querySelector<HTMLElement>(".menu-build");
  if (!label) {
    return;
  }
  // A crowded label keeps its box (it is only invisible), so it can be measured either way.
  label.removeAttribute("data-crowded");
  const room = label.getBoundingClientRect();
  const dialog = panel.getBoundingClientRect();
  const clear =
    room.left - BUILD_LABEL_GAP >= dialog.right || room.top - BUILD_LABEL_GAP >= dialog.bottom;
  label.toggleAttribute("data-crowded", !clear);
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
