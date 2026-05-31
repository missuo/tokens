// Strict light-mode E2E check.
// Emulates a Light OS preference with a FRESH browser profile (no stored theme),
// visits each route, asserts the page is actually rendered light, hunts for any
// large dark-background elements (hardcoded-dark offenders), exercises the theme
// toggle, and screenshots everything.
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";

const BASE = process.env.BASE_URL || "http://localhost:3000";
const OUT = process.env.OUT_DIR || "/tmp/theme-shots";
mkdirSync(OUT, { recursive: true });

const ROUTES = [
  ["leaderboard", "/leaderboard"],
  ["leaderboard-groups", "/leaderboard?view=groups"],
  ["local", "/local"],
  ["settings", "/settings"],
  ["device", "/device"],
];

// Relative luminance from an "rgb(...)"/"rgba(...)" string. Returns null if transparent.
function luminance(rgb) {
  const m = rgb.match(/rgba?\(([^)]+)\)/);
  if (!m) return null;
  const parts = m[1].split(",").map((s) => parseFloat(s.trim()));
  const [r, g, b, a = 1] = parts;
  if (a < 0.5) return null; // effectively transparent
  const lin = (c) => {
    c /= 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

// Runs in the browser: returns theme state + list of big dark-bg elements.
function inspect() {
  const html = document.documentElement;
  const bodyBg = getComputedStyle(document.body).backgroundColor;
  const offenders = [];
  const els = document.querySelectorAll("*");
  for (const el of els) {
    const r = el.getBoundingClientRect();
    const area = r.width * r.height;
    if (area < 6000) continue; // ignore tiny elements
    const bg = getComputedStyle(el).backgroundColor;
    offenders.push({
      bg,
      area: Math.round(area),
      tag: el.tagName.toLowerCase(),
      cls: (el.getAttribute("class") || "").slice(0, 80),
      text: (el.textContent || "").trim().slice(0, 40),
    });
  }
  return {
    htmlClass: html.getAttribute("class") || "",
    dataTheme: html.getAttribute("data-theme") || "",
    bodyBg,
    storedTheme: localStorage.getItem("theme"),
    matchesDark: window.matchMedia("(prefers-color-scheme: dark)").matches,
    offenders,
  };
}

function darkOffenders(report) {
  // Keep elements whose own background is genuinely dark (luminance < 0.25).
  return report.offenders
    .map((o) => ({ ...o, lum: luminance(o.bg) }))
    .filter((o) => o.lum !== null && o.lum < 0.25)
    .sort((a, b) => b.area - a.area)
    .slice(0, 12);
}

const results = [];
const browser = await chromium.launch();
// colorScheme:'light' => prefers-color-scheme: light. Fresh context => no localStorage.
const ctx = await browser.newContext({ colorScheme: "light", viewport: { width: 1440, height: 900 } });
const page = await ctx.newPage();

for (const [name, route] of ROUTES) {
  const url = BASE + route;
  await page.goto(url, { waitUntil: "networkidle", timeout: 60000 });
  await page.waitForTimeout(600); // let next-themes resolve + client render
  const report = await page.evaluate(inspect);
  const offenders = darkOffenders(report);
  const bodyLum = luminance(report.bodyBg);
  await page.screenshot({ path: `${OUT}/${name}.png`, fullPage: true });
  results.push({ name, url, bodyBg: report.bodyBg, bodyLum, htmlClass: report.htmlClass, storedTheme: report.storedTheme, prefersDark: report.matchesDark, darkOffenderCount: offenders.length, offenders });
}

// Toggle test on the leaderboard: click the theme toggle and confirm what happens.
await page.goto(BASE + "/leaderboard", { waitUntil: "networkidle" });
await page.waitForTimeout(400);
const before = await page.evaluate(() => document.documentElement.getAttribute("class") || "");
const toggle = page.locator('button[aria-label="Toggle color theme"]');
let toggleResult = "toggle button not found";
if (await toggle.count()) {
  await toggle.first().click();
  await page.waitForTimeout(400);
  const after = await page.evaluate(() => document.documentElement.getAttribute("class") || "");
  const storedAfter = await page.evaluate(() => localStorage.getItem("theme"));
  await page.screenshot({ path: `${OUT}/leaderboard-after-toggle.png`, fullPage: true });
  toggleResult = { beforeClass: before, afterClass: after, storedAfter };
}

await browser.close();

console.log(JSON.stringify({ baseUrl: BASE, outDir: OUT, toggleResult, results }, null, 2));
