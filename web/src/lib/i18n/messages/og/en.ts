/**
 * Copy drawn into the Open Graph share card.
 *
 * These are the only strings on the site that are rendered by Satori rather
 * than a browser: the card is a PNG, so there is no DOM, no CSS cascade and no
 * font fallback for a script the image does not carry. Keep them short — the
 * tiles are fixed-size and a two-line label pushes the value out of frame.
 */
const messages = {
  site: {
    headline: "The leaderboard for",
    accent: "AI coding usage.",
    subtitle:
      "Track tokens and spend across Claude Code, Codex, Cursor, Gemini and 20+ more.",
  },
  card: {
    activity: "Activity",
    less: "Less",
    more: "More",
    pastYear: "Past year",
    activeDays: "{count, plural, one {# active day} other {# active days}}",
    fallback: "AI coding usage on Tokens",
    rank: "#{rank}",
  },
  labels: {
    tokens: "TOKENS",
    spent: "SPENT",
    rank: "RANK",
    activeDays: "ACTIVE DAYS",
  },
};

export default messages;
