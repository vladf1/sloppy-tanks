import assert from "node:assert/strict";

/** Check the shipped CSS and reachable controls, including narrow phone layouts. */
export async function checkMultiplayerMenu(page) {
  const viewport = page.viewportSize();
  try {
    for (const [width, height] of [
      [1200, 800],
      [390, 844],
      [320, 568],
      [844, 390],
    ]) {
      await page.setViewportSize({ width, height });
      const layout = await page.evaluate(() => {
        const menu = document.querySelector(".network-menu");
        const bounds = menu.getBoundingClientRect();
        return {
          rosterDisplay: getComputedStyle(document.querySelector("#network-roster")).display,
          hiddenControlsVisible: [...menu.querySelectorAll("[hidden]")].some(
            (node) => getComputedStyle(node).display !== "none",
          ),
          left: bounds.left,
          right: bounds.right,
          clientWidth: menu.clientWidth,
          scrollWidth: menu.scrollWidth,
        };
      });
      assert.equal(layout.rosterDisplay, "grid", "Multiplayer stylesheet must be applied");
      assert.equal(layout.hiddenControlsVisible, false, "Inactive controls must stay hidden");
      assert.ok(layout.left >= 0 && layout.right <= width, `Menu fits ${width}px viewport`);
      assert.ok(layout.scrollWidth <= layout.clientWidth + 1, `No horizontal scroll at ${width}px`);
    }
  } finally {
    if (viewport) await page.setViewportSize(viewport);
  }
}
