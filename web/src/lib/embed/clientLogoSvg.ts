/**
 * Inline client logo rendering for the embed SVG cards.
 *
 * Embed responses are standalone SVGs behind a `default-src 'none'` CSP and
 * GitHub's camo proxy, so external `<image>` references never load. Logos are
 * therefore inlined from @lobehub/icons-static-svg (see
 * scripts/generate-embed-logos.ts) as nested `<svg>` fragments.
 */
import {
  EMBED_CLIENT_LOGOS,
  type EmbedClientLogo,
} from "./clientLogos.generated";

export function getEmbedClientLogo(source: string): EmbedClientLogo | null {
  const normalized = source.toLowerCase();
  // cc-mirror variants are Claude Code mirrors; show the Claude mark.
  const key = normalized.startsWith("cc-mirror/") ? "claude" : normalized;
  return EMBED_CLIENT_LOGOS[key] ?? null;
}

export interface ClientLogoSvgOptions {
  x: number;
  y: number;
  size: number;
  /** Fill applied to monochrome icons (color icons keep their brand fills). */
  monoColor: string;
}

/** Render a client's logo as a nested SVG, or null when no icon exists. */
export function clientLogoSvg(
  source: string,
  options: ClientLogoSvgOptions,
): string | null {
  const logo = getEmbedClientLogo(source);
  if (!logo) return null;
  const { x, y, size, monoColor } = options;
  const rootAttrs = logo.mono
    ? logo.rootAttrs.replaceAll("currentColor", monoColor)
    : logo.rootAttrs;
  return `<svg x="${x}" y="${y}" width="${size}" height="${size}" viewBox="${logo.viewBox}"${rootAttrs ? ` ${rootAttrs}` : ""}>${logo.body}</svg>`;
}
