import type { ClientType } from "./types";

// 2D Canvas
export const BOX_WIDTH = 10;
export const BOX_MARGIN = 2;
export const TEXT_HEIGHT = 15;
export const CANVAS_MARGIN = 20;
export const HEADER_HEIGHT = 60;
export const FONT_SIZE = 10;
export const FONT_FAMILY = "'SF Mono', ui-monospace, Menlo, Monaco, 'Cascadia Mono', 'Segoe UI Mono', monospace";

// 3D Isometric (obelisk.js)
export const CUBE_SIZE = 16;
export const MAX_CUBE_HEIGHT = 100;
export const MIN_CUBE_HEIGHT = 3;
export const ISO_CANVAS_WIDTH = 1000;
export const ISO_CANVAS_HEIGHT = 600;

// Labels
export const DAY_LABELS_SHORT = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
export const MONTH_LABELS_SHORT = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

// Source configuration
export const SOURCE_DISPLAY_NAMES: Record<ClientType, string> = {
  opencode: "OpenCode",
  claude: "Claude Code",
  codex: "Codex CLI",
  copilot: "Copilot",
  gemini: "Gemini CLI",
  cursor: "Cursor",
  amp: "Amp",
  codebuff: "Codebuff",
  droid: "Droid",
  openclaw: "OpenClaw",
  hermes: "Hermes Agent",
  pi: "Pi",
  kimi: "Kimi",
  qwen: "Qwen",
  roocode: "Roo Code",
  // Two different products from two different orgs, not one client with an
  // alias: `kilocode` is Kilo-Org's VS Code extension, `kilo` is nicepkg's CLI.
  // Both rendered as "Kilo" with the same mark, so the supported-clients grid
  // showed the same entry twice and neither was identifiable.
  kilocode: "Kilo",
  kilo: "Kilo CLI",
  mux: "Mux",
  kiro: "Kiro",
  crush: "Crush",
  goose: "Goose",
  antigravity: "Antigravity",
  "antigravity-cli": "Antigravity CLI",
  zed: "Zed Agent",
  trae: "Trae",
  warp: "Warp",
  cline: "Cline",
  synthetic: "Synthetic",
  gjc: "Gajae Code",
  "9router": "9Router",
  grok: "Grok Build",
  jcode: "Jcode",
  commandcode: "Command Code",
  micode: "MiMo Code",
  junie: "Junie",
  zcode: "ZCode",
  opencodereview: "OpenCodeReview",
  codebuddy: "CodeBuddy",
  workbuddy: "WorkBuddy",
  "devin-cli": "Devin CLI",
  "devin-desktop": "Devin Desktop",
  fx: "Fx",
};

// Client logos, served from this deployment rather than hotlinked.
//
// These used to point at raw.githubusercontent.com on the `main` branch, which
// silently 404s for any logo added on a feature branch — grok, zcode and
// workbuddy were all broken that way. Copying them into public/ at build time
// ties the asset to the deploy that references it.
const GITHUB_CDN_BASE = "/clients";
export const SOURCE_LOGOS: Record<ClientType, string> = {
  opencode: `${GITHUB_CDN_BASE}/client-opencode.png`,
  claude: `${GITHUB_CDN_BASE}/client-claude.jpg`,
  codex: `${GITHUB_CDN_BASE}/client-openai.jpg`,
  copilot: `${GITHUB_CDN_BASE}/client-copilot.jpg`,
  gemini: `${GITHUB_CDN_BASE}/client-gemini.png`,
  cursor: `${GITHUB_CDN_BASE}/client-cursor.jpg`,
  amp: `${GITHUB_CDN_BASE}/client-amp.png`,
  codebuff: `${GITHUB_CDN_BASE}/client-codebuff.png`,
  droid: `${GITHUB_CDN_BASE}/client-droid.png`,
  openclaw: `${GITHUB_CDN_BASE}/client-openclaw.jpg`,
  hermes: `${GITHUB_CDN_BASE}/client-hermes.png`,
  pi: `${GITHUB_CDN_BASE}/client-pi.png`,
  kimi: `${GITHUB_CDN_BASE}/client-kimi.png`,
  qwen: `${GITHUB_CDN_BASE}/client-qwen.png`,
  roocode: `${GITHUB_CDN_BASE}/client-roocode.png`,
  kilocode: `${GITHUB_CDN_BASE}/client-kilocode.png`,
  // Generic rather than Kilo-Org's mark: nicepkg's CLI is a different project,
  // and borrowing the other one's logo attributes it to the wrong people.
  // Swap in a real asset if one turns up.
  kilo: `${GITHUB_CDN_BASE}/client-generic.svg`,
  mux: `${GITHUB_CDN_BASE}/client-mux.png`,
  kiro: `${GITHUB_CDN_BASE}/client-kiro.jpg`,
  crush: `${GITHUB_CDN_BASE}/client-crush.png`,
  goose: `${GITHUB_CDN_BASE}/client-goose.png`,
  antigravity: `${GITHUB_CDN_BASE}/client-antigravity.png`,
  "antigravity-cli": `${GITHUB_CDN_BASE}/client-antigravity.png`,
  zed: `${GITHUB_CDN_BASE}/client-zed.webp`,
  trae: `${GITHUB_CDN_BASE}/client-trae.png`,
  warp: `${GITHUB_CDN_BASE}/client-warp.png`,
  cline: `${GITHUB_CDN_BASE}/client-cline.png`,
  synthetic: `${GITHUB_CDN_BASE}/client-synthetic.png`,
  gjc: `${GITHUB_CDN_BASE}/client-generic.svg`,
  // 9Router data flows through the gjc-format bridge; reuse the gjc mark
  // until 9Router ships a dedicated asset.
  "9router": `${GITHUB_CDN_BASE}/client-generic.svg`,
  grok: `${GITHUB_CDN_BASE}/client-grok.png`,
  jcode: `${GITHUB_CDN_BASE}/client-jcode.png`,
  commandcode:
    `${GITHUB_CDN_BASE}/client-commandcode.png`,
  micode: `${GITHUB_CDN_BASE}/client-micode.jpg`,
  junie: `${GITHUB_CDN_BASE}/client-junie.png`,
  zcode: `${GITHUB_CDN_BASE}/client-zcode.png`,
  opencodereview: `${GITHUB_CDN_BASE}/client-opencodereview.png`,
  codebuddy:
    `${GITHUB_CDN_BASE}/client-codebuddy.png`,
  workbuddy:
    `${GITHUB_CDN_BASE}/client-workbuddy.png`,
  "devin-cli": `${GITHUB_CDN_BASE}/client-devin.jpg`,
  "devin-desktop": `${GITHUB_CDN_BASE}/client-devin.jpg`,
  fx: `${GITHUB_CDN_BASE}/client-fx.png`,
};

export const SOURCE_COLORS: Record<ClientType, string> = {
  opencode: "#00A8E8",
  claude: "#f97316",
  codex: "#10B981",
  copilot: "#24292F",
  gemini: "#8b5cf6",
  cursor: "#22c55e",
  amp: "#EC4899",
  codebuff: "#7C3AED",
  droid: "#1F1D1C",
  openclaw: "#EF4444",
  hermes: "#FFD700",
  pi: "#6366F1",
  kimi: "#8B5CF6",
  qwen: "#1A73E8",
  roocode: "#10B981",
  kilocode: "#F59E0B",
  kilo: "#F59E0B",
  mux: "#171717",
  kiro: "#00A67D",
  crush: "#DC2626",
  goose: "#64B4DC",
  antigravity: "#6366F1",
  "antigravity-cli": "#6366F1",
  zed: "#084CCF",
  trae: "#00BFA5",
  warp: "#01A4A4",
  cline: "#5B8DEF",
  synthetic: "#4ADE80",
  gjc: "#FF6B6B",
  "9router": "#0EA5E9",
  grok: "#171717",
  jcode: "#F59E0B",
  commandcode: "#A855F7",
  micode: "#FF6900",
  junie: "#7B61FF",
  zcode: "#3B5BDB",
  opencodereview: "#FF6A00",
  codebuddy: "#00A4FF",
  workbuddy: "#2563EB",
  "devin-cli": "#334155",
  "devin-desktop": "#334155",
  fx: "#0070F3",
};

/**
 * Every client the CLI scans, in display order.
 *
 * Mirrors `define_clients!` in `cli/tokens-core/src/clients.rs` — 40 entries,
 * excluding the two filter-only aliases (`synthetic`, `9router`) that have no
 * scan path of their own. Kept as an explicit list rather than derived from
 * SOURCE_DISPLAY_NAMES, which also carries those aliases and legacy keys.
 */
export const SUPPORTED_CLIENTS: readonly ClientType[] = [
  "amp",
  "antigravity",
  "antigravity-cli",
  "claude",
  "cline",
  "codebuddy",
  "codebuff",
  "codex",
  "commandcode",
  "copilot",
  "crush",
  "cursor",
  "devin-cli",
  "devin-desktop",
  "droid",
  "fx",
  "gjc",
  "gemini",
  "goose",
  "grok",
  "hermes",
  "jcode",
  "junie",
  "kilo",
  "kilocode",
  "kimi",
  "kiro",
  "micode",
  "mux",
  "openclaw",
  "opencode",
  "opencodereview",
  "pi",
  "qwen",
  "roocode",
  "trae",
  "warp",
  "workbuddy",
  "zcode",
  "zed",
] as const;

// Install command. Shared so the docs page and the local viewer cannot drift —
// the plain `brew install tokens` the viewer used to print installs a different
// package. Matches install.sh's macOS hint.
export const BREW_INSTALL_COMMAND = "brew install owo-network/brew/tokens";

// Derived values
export const CELL_SIZE = BOX_WIDTH + BOX_MARGIN;

