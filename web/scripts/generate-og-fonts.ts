/**
 * Generate src/lib/og/fonts.generated.ts from Google Fonts.
 *
 * The share-card renderer (Satori) cannot use next/font and has no font of its
 * own beyond Noto Sans, so the faces the site uses are inlined as base64. That
 * keeps `/api/og` free of filesystem reads and outbound fetches, which matters
 * because the same route runs under standalone Node and under Workers.
 *
 * Two font groups come out of this:
 *
 * - the Latin faces, `latin` subset only;
 * - one CJK face, shared by Chinese and Japanese, subsetted to the characters
 *   the card actually has to draw: every ideograph and kana in the `og`
 *   catalogue, the most common 800 hanzi, and the kana block.
 *
 * The CJK face is Noto Sans JP for both languages because it carries the full
 * hanzi set *and* kana — a Simplified-Chinese face has no kana at all, and two
 * faces would double the payload for a few glyph-shape differences nobody
 * reads on a 600px thumbnail. A character outside the subset falls back to the
 * Latin face and can render blank; that is the price of keeping the route free
 * of outbound fetches, and 800 hanzi covers ordinary display names.
 *
 * Satori takes TTF/OTF/WOFF (not WOFF2).
 *
 * Run with: bun scripts/generate-og-fonts.ts
 */
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const outFile = join(here, "../src/lib/og/fonts.generated.ts");

const FACES = [
  { family: "Geist", query: "Geist", weight: 500 },
  { family: "Geist", query: "Geist", weight: 600 },
  { family: "JetBrains Mono", query: "JetBrains+Mono", weight: 600 },
] as const;

/** How many of the common hanzi to carry. 800 keeps the css2 URL under 10 KB. */
const CJK_HANZI_LIMIT = 800;

// An old WebKit UA makes the css2 API answer with WOFF rather than WOFF2.
const LEGACY_UA =
  "Mozilla/5.0 (Windows NT 6.1) AppleWebKit/534.30 (KHTML, like Gecko) Safari/534.30";

async function latinWoffUrl(query: string, weight: number): Promise<string> {
  const res = await fetch(
    `https://fonts.googleapis.com/css2?family=${query}:wght@${weight}&display=swap`,
    { headers: { "User-Agent": LEGACY_UA } },
  );
  if (!res.ok) throw new Error(`css2 ${query}@${weight}: ${res.status}`);
  const css = await res.text();
  const block = css.split("/* latin */")[1];
  const url = block?.match(/src:\s*url\(([^)]+)\)\s*format\('woff'\)/)?.[1];
  if (!url) throw new Error(`no latin woff for ${query}@${weight}`);
  return url;
}

/**
 * The characters the CJK face must be able to draw.
 *
 * Ordered as: everything the `og` catalogue contains, then the common hanzi,
 * then the kana block. Catalogue characters come first so that growing the
 * copy can never push a character the card literally needs out of the subset.
 */
async function cjkCharacters(): Promise<string> {
  const dir = join(here, "../src/lib/i18n/messages/og");
  const seen = new Set<string>();

  for (const file of ["zh-CN.ts", "ja.ts"]) {
    const text = await Bun.file(join(dir, file)).text();
    for (const char of text) {
      if (/[\u3000-\u9fff\u3040-\u30ff\uff00-\uffef]/.test(char)) seen.add(char);
    }
  }

  // The hanzi list is checked in beside this script rather than decoded here:
  // Bun's TextDecoder has no `gbk`, and transpiling GB2312 by hand for 800
  // characters is more code than a file. `og-cjk-hanzi.txt` is the first 800 of
  // GB2312 level 1 (the 3755 most common hanzi) in row order — the tail of a
  // display name, not a dictionary.
  const hanzi = (await Bun.file(join(here, "og-cjk-hanzi.txt")).text()).slice(
    0,
    CJK_HANZI_LIMIT,
  );
  for (const char of hanzi) {
    if (/[\u4e00-\u9fff]/.test(char)) seen.add(char);
  }

  for (let code = 0x3040; code <= 0x309f; code++) seen.add(String.fromCodePoint(code));
  for (let code = 0x30a0; code <= 0x30ff; code++) seen.add(String.fromCodePoint(code));

  return [...seen].join("");
}

async function cjkWoff(): Promise<Uint8Array> {
  const text = await cjkCharacters();
  const res = await fetch(
    `https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@600&text=${encodeURIComponent(text)}&display=swap`,
    { headers: { "User-Agent": LEGACY_UA } },
  );
  if (!res.ok) throw new Error(`css2 CJK: ${res.status}`);
  const css = await res.text();
  const url = css.match(/url\(([^)]+)\)/)?.[1];
  if (!url) throw new Error("no woff for CJK subset");
  return new Uint8Array(await (await fetch(url)).arrayBuffer());
}

const entries: string[] = [];
for (const face of FACES) {
  const url = await latinWoffUrl(face.query, face.weight);
  const bytes = Buffer.from(await (await fetch(url)).arrayBuffer());
  console.log(`${face.family} ${face.weight}: ${bytes.length} bytes`);
  entries.push(
    `  { name: ${JSON.stringify(face.family)}, weight: ${face.weight}, data: ${JSON.stringify(bytes.toString("base64"))} },`,
  );
}

const cjk = await cjkWoff();
console.log(`Noto Sans JP (CJK subset): ${cjk.length} bytes`);
entries.push(
  `  { name: ${JSON.stringify("Noto Sans JP")}, weight: 600, data: ${JSON.stringify(Buffer.from(cjk).toString("base64"))} },`,
);

writeFileSync(
  outFile,
  `/**
 * AUTO-GENERATED by scripts/generate-og-fonts.ts — do not edit by hand.
 * Latin WOFF subsets of the site's faces plus one shared CJK subset, inlined
 * for the Open Graph renderer.
 * Regenerate with: bun scripts/generate-og-fonts.ts
 */

export interface OgFontSource {
  name: string;
  weight: 500 | 600;
  /** Base64-encoded WOFF. */
  data: string;
}

export const OG_FONT_SOURCES: OgFontSource[] = [
${entries.join("\n")}
];
`,
);
console.log(`wrote ${outFile}`);
