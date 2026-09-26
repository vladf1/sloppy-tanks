// Run the self-checking fixtures in tests/*.browser.html and require their PASS verdict:
// solo reinforcement rendering, map switches and water reflections, and suspension
// with a turret that follows the hull's tilt.
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { gameUrl, launchGame } from "./browser-helpers.mjs";

const out = "artifacts/performance/fixtures";
mkdirSync(out, { recursive: true });
const { browser, context, errors } = await launchGame({
  viewport: { width: 1280, height: 800 },
  consoleErrors: true,
});
const fixtures = [
  ["reinforcements", []],
  ["maps", ["#checks", "#water-checks"]],
  ["suspension", ["#checks"]],
];
const verdicts = {};
/** Read the verdict inside the wait, so a dev-server reload cannot swap the page in between. */
async function verdict(page) {
  const text = await page.waitForFunction(
    () => {
      const text = document.querySelector("#result")?.textContent ?? "";
      return /^(PASS|FAIL)/.test(text) && text;
    },
    undefined,
    { timeout: 120000 },
  );
  return text.jsonValue();
}
try {
  for (const [name, buttons] of fixtures) {
    const page = await context.newPage();
    await page.goto(new URL(`tests/${name}.browser.html`, gameUrl).href);
    verdicts[name] = [];
    if (!buttons.length) verdicts[name].push(await verdict(page));
    for (const button of buttons) {
      // Wait for the module script to wire its buttons before the first check.
      await page.waitForFunction(
        (selector) => typeof document.querySelector(selector)?.onclick === "function",
        button,
      );
      await page.locator(button).click();
      verdicts[name].push(await verdict(page));
    }
    await page.screenshot({ path: `${out}/${name}.png` });
    await page.close();
    for (const text of verdicts[name]) {
      console.log(`${name}: ${text.split("\n")[0]}`);
      assert.match(text, /^PASS/, `${name}: ${text}`);
    }
  }
  assert.deepEqual(errors, []);
} finally {
  writeFileSync(`${out}/results.json`, JSON.stringify({ verdicts, errors }, null, 2));
  await browser.close();
}
