import assert from "node:assert/strict";

/** Check the shipped CSS and reachable controls, at supported desktop sizes. Works for
 * Battle Setup's multiplayer tab and for the in-room network menu. */
export async function checkMultiplayerMenu(page) {
  const viewport = page.viewportSize();
  try {
    for (const [width, height] of [
      [1200, 800],
      [1440, 900],
    ]) {
      await page.setViewportSize({ width, height });
      const layout = await page.evaluate(() => {
        const menu = document.querySelector(".network-menu, .menu.start");
        const bounds = menu.getBoundingClientRect();
        const shown = (node) => {
          const style = getComputedStyle(node);
          return style.display !== "none" && style.visibility !== "hidden";
        };
        return {
          rosterDisplay: getComputedStyle(document.querySelector("#network-roster, #room-list"))
            .display,
          hiddenControlsVisible: [...menu.querySelectorAll("[hidden]")].some(shown),
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

/** Battle Setup's multiplayer tab, opened with a real pointer click. */
export async function openMultiplayerTab(page) {
  const tab = page.locator("#tab-multiplayer");
  await tab.waitFor();
  const bounds = await tab.boundingBox();
  assert.ok(bounds);
  await page.mouse.click(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  await page.locator("#multiplayer-panel:not([hidden])").waitFor();
  await waitForRoomBrowser(page);
}

/** The lazily loaded room list fills in a saved or random name when it is ready. */
export async function waitForRoomBrowser(page) {
  await page.waitForFunction(() => document.querySelector("#player-name")?.value);
}

/** Pick a map for a new room from Battle Setup's multiplayer tab. */
export async function chooseRoomMap(page, map) {
  await page.locator(`input[name="roomMap"][value="${map}"]`).check();
}
