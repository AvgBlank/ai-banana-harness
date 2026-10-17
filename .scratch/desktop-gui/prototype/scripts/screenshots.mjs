// Renders every Review-dock state in both themes from the built single file (dist/index.html).
// Usage: pnpm build && pnpm shots [widthxheight]
import { chromium } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const [w, h] = (process.argv[2] ?? "1440x900").split("x").map(Number);
const out = resolve(root, "screenshots", process.argv[2] ?? "1440x900");
mkdirSync(out, { recursive: true });

// Uses the installed Google Chrome so no browser download is needed (set BH_CHANNEL= to use Playwright's own).
const browser = await chromium.launch(process.env.BH_CHANNEL === "" ? {} : { channel: process.env.BH_CHANNEL ?? "chrome" });
const errors = [];
for (const theme of ["dark", "light"]) {
  const page = await browser.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: 1 });
  page.on("pageerror", (e) => errors.push(`${theme}: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && errors.push(`${theme}: ${m.text()}`));
  await page.goto(pathToFileURL(resolve(root, "dist/index.html")).href);
  const dock = page.getByLabel("Prototype controls");
  await dock.getByRole("button", { name: theme, exact: true }).click();
  const labels = await dock.locator("div.max-h-\\[46vh\\] button").allInnerTexts();
  for (const label of labels) {
    await dock.getByRole("button", { name: label, exact: true }).click();
    await page.waitForTimeout(label.includes("streaming") || label.includes("Quit") ? 1300 : 350);
    await dock.evaluate((el) => (el.style.visibility = "hidden"));
    const slug = label.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
    await page.screenshot({ path: resolve(out, `${theme}-${slug}.png`) });
    await dock.evaluate((el) => (el.style.visibility = "visible"));
    await page.keyboard.press("Escape");
  }
  await page.close();
}
await browser.close();
if (errors.length) {
  console.error("Console/page errors:\n" + errors.join("\n"));
  process.exit(1);
}
console.log(`screenshots written to ${out}`);
