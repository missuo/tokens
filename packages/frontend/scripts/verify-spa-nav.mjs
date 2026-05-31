import { chromium } from "playwright";
const browser = await chromium.launch();
const ctx = await browser.newContext({ colorScheme: "light", viewport: { width: 1440, height: 1000 } });
const page = await ctx.newPage();
await page.goto("https://tokens.ci/leaderboard", { waitUntil: "load", timeout: 45000 });
await page.waitForTimeout(2000);
await page.evaluate(() => { window.__spa = "persisted"; });
let fullLoads = 0;
page.on("load", () => { fullLoads++; });
const row = page.locator("table tbody tr").first();
const hadRow = (await row.count()) > 0;
if (hadRow) {
  await row.click();
  await page.waitForURL(/\/u\//, { timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(1800);
}
const result = {
  hadRow,
  url: page.url(),
  markerSurvived: (await page.evaluate(() => window.__spa)) === "persisted",
  fullPageLoadsAfterClick: fullLoads,
  navPersisted: (await page.locator('nav[aria-label="Main navigation"]').count()) > 0,
};
console.log(JSON.stringify(result, null, 2));
console.log(result.markerSurvived && result.fullPageLoadsAfterClick === 0
  ? "\n✅ SEAMLESS client-side navigation (no full reload, nav persisted)"
  : "\n❌ full page reload occurred");
await browser.close();
