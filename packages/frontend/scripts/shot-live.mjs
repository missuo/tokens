// Screenshot the live leaderboard to inspect the real layout (with data).
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";
const URL = process.env.URL || "https://tokens.ci/leaderboard";
const OUT = "/tmp/lb-shots";
mkdirSync(OUT, { recursive: true });
const browser = await chromium.launch();
for (const [name, width] of [["desktop", 1440], ["narrow", 1024], ["mobile", 420]]) {
  const ctx = await browser.newContext({ colorScheme: "light", viewport: { width, height: 1000 } });
  const page = await ctx.newPage();
  await page.goto(URL, { waitUntil: "load", timeout: 45000 });
  await page.waitForTimeout(2500);
  await page.screenshot({ path: `${OUT}/${name}.png`, fullPage: true });
  // capture the leaderboard table's column widths for the desktop view
  if (name === "desktop") {
    const info = await page.evaluate(() => {
      const ths = [...document.querySelectorAll("table thead th, table th")].map((th) => ({
        text: (th.textContent || "").trim().slice(0, 16),
        w: Math.round(th.getBoundingClientRect().width),
      }));
      const firstRowCells = [...(document.querySelector("table tbody tr")?.children || [])].map((td) => ({
        text: (td.textContent || "").trim().slice(0, 24),
        w: Math.round(td.getBoundingClientRect().width),
      }));
      return { ths, firstRowCells };
    });
    console.log(JSON.stringify(info, null, 2));
  }
  await ctx.close();
}
await browser.close();
console.log("shots in " + OUT);
