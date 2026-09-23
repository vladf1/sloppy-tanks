/**
 * Start a round through the real Battle Setup menu, as a player does. Until GO
 * completes, the startup overlay covers the canvas and swallows pointer and wheel
 * input; `window.sloppy.start()` alone restarts the simulation but leaves the menu up.
 * Waits on match state rather than rendered time, so it also works in fixtures that
 * freeze the game's own animation loop.
 * @param {import("playwright").Page} page
 */
export async function startRound(page) {
  await page.locator("#start").click();
  await page.waitForFunction(
    () =>
      !document.querySelector("#startup-overlay") && window.sloppy?.sim.match.phase === "playing",
  );
}
