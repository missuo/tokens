/**
 * Parses search directives from a leaderboard search string.
 *
 * Supported directives:
 * - `client:<value>` — filter users who have submitted data from this client
 * - `model:<value>` — filter users who have used this model
 *
 * Directives are case-insensitive and can appear anywhere in the search string.
 * Multiple directives of the same type are OR-ed (user matches if ANY applies).
 * Remaining non-directive text is treated as a username/displayName search term.
 *
 * Examples:
 *   "client:opencode junhoyeo"  → { clients: ["opencode"], models: [], text: "junhoyeo" }
 *   "model:claude-sonnet-4"    → { clients: [], models: ["claude-sonnet-4"], text: "" }
 *   "client:claude client:amp" → { clients: ["claude", "amp"], models: [], text: "" }
 */

export interface ParsedSearchDirectives {
  /** Free-text portion (username/displayName search). Trimmed. */
  text: string;
  /** Client IDs extracted from `client:` directives. Lowercased. */
  clients: string[];
  /** Model IDs extracted from `model:` directives. Lowercased. */
  models: string[];
}

const DIRECTIVE_REGEX = /\b(client|model):([\w.:\-/]+)/gi;

export function escapeLikePattern(value: string): string {
  return value.replace(/[%_\\]/g, "\\$&");
}

export function parseSearchDirectives(raw: string): ParsedSearchDirectives {
  const clients: string[] = [];
  const models: string[] = [];

  const text = raw
    .replace(DIRECTIVE_REGEX, (_, directive: string, value: string) => {
      const lowerDirective = directive.toLowerCase();
      const lowerValue = value.toLowerCase().replace(/[.,;)]+$/, "");

      if (lowerDirective === "client" && lowerValue) {
        clients.push(lowerValue);
      } else if (lowerDirective === "model" && lowerValue) {
        models.push(lowerValue);
      }

      return "";
    })
    .replace(/\s+/g, " ")
    .trim();

  return { text, clients, models };
}

export function hasDirectives(parsed: ParsedSearchDirectives): boolean {
  return parsed.clients.length > 0 || parsed.models.length > 0;
}

/**
 * Whether a row survives the plain-text part of a search box.
 *
 * Lives here rather than beside the queries because the board is now filtered
 * on the client, over rows it already holds — and `getLeaderboard` pulls in the
 * database, so importing it from a component would drag the driver into the
 * browser bundle. Both sides call this one function so a search cannot mean
 * something different depending on where it ran.
 */
export function matchesLeaderboardSearch(
  user: { username: string; displayName: string | null },
  textSearch: string
): boolean {
  if (!textSearch) {
    return true;
  }

  const lowerSearch = textSearch.toLowerCase();
  if (user.username.toLowerCase().includes(lowerSearch)) {
    return true;
  }
  if (user.displayName && user.displayName.toLowerCase().includes(lowerSearch)) {
    return true;
  }
  return false;
}
