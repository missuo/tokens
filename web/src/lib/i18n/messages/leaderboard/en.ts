/**
 * Every rendered string on the board, the contribution graph it embeds, and the
 * panels hanging off it.
 *
 * Split into `statsPanel` / `breakdownPanel` / `tokenGraph*` groups because
 * those components are also mounted by the local viewer, so they are the first
 * candidates to move into `common` once a second page needs them translated.
 */
const messages = {
  "title": "Leaderboard",
  "description": "AI coding token usage, reported by the Tokens CLI.",
  "totals": "Totals",
  "userHandle": "@{username}",
  "yourPosition": "Your position",
  "stats": {
    "tokens": "Tokens",
    "cost": "Cost",
    "developers": "Developers",
    "yourRank": "Your rank",
    "yourRankValue": "#{rank}"
  },
  "period": {
    "label": "Period",
    "allTime": "All time",
    "today": "Today",
    "week": "Week",
    "month": "Month",
    "lastMonth": "Last month"
  },
  "sortBy": "Sort by",
  "sort": {
    "tokens": "Tokens",
    "cost": "Cost"
  },
  "search": {
    "placeholder": "Search…",
    "label": "Search developers"
  },
  "formatToggle": {
    "exact": "Show exact numbers",
    "abbreviate": "Abbreviate numbers",
    "exactAria": "{label}: show exact numbers",
    "abbreviateAria": "{label}: abbreviate numbers"
  },
  "table": {
    "rank": "#",
    "developer": "Developer",
    "usage": "Usage"
  },
  "empty": {
    "noResults": "No developers found",
    "nothing": "Nothing recorded",
    "noMatch": "No developer matches \"{query}\" for this period.",
    "none": "No usage was submitted for this period."
  },
  "pagination": {
    "label": "Pagination",
    "previous": "Previous",
    "next": "Next",
    "page": "{current} of {total}"
  }
};

export default messages;
