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

/** Set only the game seed; mocking global Math.random also duplicates Three.js UUIDs. */
export async function seedGame(page, seed) {
  if (!Number.isSafeInteger(seed) || seed < 0) throw new Error("Invalid fixture seed");
  // Startup is bundled inline in HTML in both Vite modes.
  await page.route(
    (url) => url.pathname.endsWith("/") || url.pathname.endsWith(".html"),
    async (route) => {
      const url = new URL(route.request().url());
      if (["127.0.0.1", "localhost"].includes(url.hostname)) {
        // Chromium treats a fulfilled document as non-local; allow this fixture's Vite socket.
        await page.context().grantPermissions(["local-network-access"], { origin: url.origin });
      }
      const response = await route.fetch();
      const source = await response.text();
      const seeded = source.replace(
        /Math\.floor\(Math\.random\(\)\s*\*\s*(?:1e6|1000000)\)/,
        String(seed),
      );
      if (source === seeded) throw new Error("Game seed initialization changed; update seedGame");
      await route.fulfill({ response, body: seeded });
    },
  );
}
