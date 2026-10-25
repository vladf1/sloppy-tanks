/** Battle Setup is laid out for a screen about 900 CSS pixels tall. A shorter laptop
 * screen (1280×800, 1366×768, 1280×720) scales the whole panel down to fit instead of
 * cutting off GO below the fold; phones lay out and scroll their own edition, and so does
 * a narrow window's one column, whose tabs have heights of their own: scaling it to each
 * would rescale the whole dialog on every tab switch. */

/** The overlay's padding above and below the panel (`style.css`, 24px each), which its
 * scroll height includes, plus the couple of pixels a zoomed layout rounds up to: a panel
 * given any less room leaves the overlay scrolling. */
const SCREEN_MARGIN = 50;
/** Battle Setup's one-column layout (`style.css`), which scrolls rather than scales. */
const ONE_COLUMN = "(max-width: 600px)";
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
    const room = window.innerHeight - SCREEN_MARGIN;
    const fit = (laidOut: number) => Math.max(MIN_SCALE, Math.min(1, room / laidOut));
    let scale = window.matchMedia(ONE_COLUMN).matches ? 1 : fit(height);
    if (scale < 0.999) {
      setZoom(panel, scale);
      // Text wraps a few pixels taller at a fractional zoom: fit once more to that layout.
      scale = Math.min(scale, fit(panel.offsetHeight));
      setZoom(panel, scale);
    }
    fitBuildLabel(panel);
  }
}

/** Place the build label again after a tab switch, which in a narrow window changes the
 * panel's height; the panel's scale stays, so switching tabs never rescales Battle Setup. */
export function fitBuildLabels(root: ParentNode = document): void {
  for (const panel of root.querySelectorAll<HTMLElement>(".menu.start")) {
    fitBuildLabel(panel);
  }
}

function setZoom(panel: HTMLElement, scale: number): void {
  panel.style.setProperty("zoom", String(scale));
  // The build label counters the zoom to keep its size (it is fixed to the screen).
  panel.style.setProperty("--panel-zoom", String(scale));
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
