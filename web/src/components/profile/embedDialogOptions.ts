import type { EmbedTemplate } from "@/lib/embed/embedShared";
import type { ColorPaletteName } from "@/lib/themes";

export type EmbedTheme = "dark" | "light";
export type EmbedSortBy = "tokens" | "cost";
export type EmbedView = "2d" | "3d";
export type EmbedNumberFormat = "compact" | "full";
export type EmbedRankFormat = "plain" | "percent" | "total";

export interface EmbedDialogOptions {
  color: ColorPaletteName | null;
  compact: boolean;
  costFormat: EmbedNumberFormat;
  graph: boolean;
  rankFormat: EmbedRankFormat;
  sortBy: EmbedSortBy;
  template: EmbedTemplate;
  theme: EmbedTheme;
  today: boolean;
  tokensFormat: EmbedNumberFormat;
  view: EmbedView;
}

export interface EmbedDialogCapabilities {
  showAccent: boolean;
  showGraph: boolean;
  showLayout: boolean;
  showNumberFormats: boolean;
  showRankFormat: boolean;
  showTemplate: boolean;
  showToday: boolean;
}

export interface ProfileEmbedLinks {
  embedUrl: string;
  htmlSnippet: string;
  markdownSnippet: string;
  profileUrl: string;
}

const TOKENS_URL = "https://tokens.ci";

function escapeHtmlAttribute(value: string): string {
  return value.replace(
    /[&<>"']/g,
    (character) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#39;",
      })[character] ?? character,
  );
}

export function buildEmbedPreviewPath(embedUrl: string): string {
  const url = new URL(embedUrl, TOKENS_URL);
  return `${url.pathname}${url.search}`;
}

export function getEmbedDialogCapabilities(
  options: Pick<EmbedDialogOptions, "compact" | "template" | "view">,
): EmbedDialogCapabilities {
  const is3d = options.view === "3d";

  return {
    showAccent: !is3d,
    showGraph:
      !is3d &&
      options.template !== "graph" &&
      options.template !== "vitals" &&
      options.template !== "detailed" &&
      !(options.template === "classic" && options.compact),
    showLayout: is3d || options.template === "classic",
    showNumberFormats: !is3d,
    showRankFormat: !is3d,
    showTemplate: !is3d,
    showToday: !is3d && options.template !== "detailed",
  };
}

export function buildProfileEmbedLinks(
  username: string,
  options: EmbedDialogOptions,
): ProfileEmbedLinks {
  const params = new URLSearchParams();
  const capabilities = getEmbedDialogCapabilities(options);

  if (options.view === "3d") params.set("view", "3d");
  if (options.theme !== "dark") params.set("theme", options.theme);
  if (options.sortBy !== "tokens") params.set("sort", options.sortBy);

  if (options.view === "3d") {
    if (options.compact) params.set("compact", "1");
  } else {
    if (options.template !== "classic") {
      params.set("template", options.template);
    }
    if (options.color) params.set("color", options.color);
    if (options.template === "classic" && options.compact) {
      params.set("compact", "1");
    }
    if (options.graph && capabilities.showGraph) params.set("graph", "1");
    if (options.today && capabilities.showToday) params.set("today", "1");
    if (options.rankFormat !== "plain") {
      params.set("rank", options.rankFormat);
    }
    params.set("tokens", options.tokensFormat);
    params.set("cost", options.costFormat);
  }

  const query = params.toString();
  const encodedUsername = encodeURIComponent(username);
  const escapedUsername = escapeHtmlAttribute(username);
  const baseEmbedUrl = `${TOKENS_URL}/api/embed/${encodedUsername}/svg`;
  const embedUrl = query ? `${baseEmbedUrl}?${query}` : baseEmbedUrl;
  const profileUrl = `${TOKENS_URL}/u/${encodedUsername}`;

  return {
    embedUrl,
    markdownSnippet: `[![Tokens Stats](${embedUrl})](${profileUrl})`,
    htmlSnippet: `<a href="${profileUrl}"><img alt="Tokens Stats for @${escapedUsername}" src="${embedUrl}" /></a>`,
    profileUrl,
  };
}
