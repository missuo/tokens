/**
 * The profile page, its contribution graph, its usage chart and the embed
 * dialog.
 *
 * Two conventions run through this namespace. Anything that names a quantity
 * carries the unit inside the message, so "3 tokens" is one plural and a
 * translator can move the word; a bare formatted figure (a table cell, an axis
 * value) stays a number and is formatted by the caller. And `{slots}` exist for
 * markup: a sentence holding a link or a `<code>` is assembled with `richText`
 * rather than split into fragments around the element.
 */
const messages = {
  /** `<title>` and every share card. Keyword-shaped on purpose. */
  meta: {
    title: "@{handle} - Token Usage | Tokens",
    shareTitleNamed: "{displayName} (@{handle}) on Tokens",
    shareTitlePlain: "@{handle} on Tokens",
    statsWithRank:
      "{tokens} tokens · {cost} spent · #{rank} on the leaderboard — AI coding usage on Tokens.",
    statsNoRank: "{tokens} tokens · {cost} spent — AI coding usage on Tokens.",
    description: "AI coding token usage for @{handle} on Tokens.",
  },

  banned: {
    stamp: "Banned",
    accountBanned: "Account banned",
    accountBannedOn: "Account banned on {date}",
    description:
      "This account has been permanently banned for submitting fraudulent usage data. All of its statistics have been removed from public view and are excluded from every ranking.",
    reasonHeading: "Ban reason",
    backToShame: "View the Hall of Shame →",
  },

  privacy: {
    title: "Your profile is private",
    description:
      "Only you can see this page. Everyone else gets {notFound}, and you are left out of the leaderboard. Apps and badges can read it with a read token from {settings}.",
    settings: "Settings",
  },

  resubmit: {
    title: "Fresh detail is available",
    description: "Re-submit with {command} to add daily model breakdowns.",
  },

  activity: {
    empty: "No activity recorded for this period.",
  },

  period: {
    label: "Period",
    allTime: "All time",
    month: "Month",
    week: "Week",
  },

  actions: {
    embed: "Embed",
    share: "Share",
    copied: "Copied",
    shareCopied: "Profile link copied",
    shareFailed: "Could not copy the link",
    github: "GitHub",
  },

  stats: {
    totals: "Totals",
    tokens: "Tokens",
    cost: "Cost",
    activeDays: "Active days",
    submissions: "Submissions",
    rank: "Rank #{rank}",
    importedHistory: "Includes imported history",
    joined: "Joined {date}",
    updated: "Updated {date}",
  },

  /** A quantity plus its unit, for the places that name both. */
  units: {
    token: "{count, plural, one {# token} other {# tokens}}",
    message: "{count, plural, one {# message} other {# messages}}",
    session: "{count, plural, one {# session} other {# sessions}}",
  },

  /** Names of the token categories, wherever a split is shown. */
  tokens: {
    input: "Input",
    output: "Output",
    cacheRead: "Cache read",
    cacheWrite: "Cache write",
    reasoning: "Reasoning",
  },

  /** The floating label on one segment of a composition bar. */
  composition: {
    segmentTitle: "{label}: {value}",
  },

  usageDetail: "Usage detail",

  sections: {
    contributions: "Contributions",
    usage: "Usage",
    tokenBreakdown: "Token breakdown",
    habits: "Habits",
    models: "Models",
    clients: "Clients",
    mcpServers: "MCP servers",
  },

  models: {
    empty: "No model usage recorded for this period.",
    fallbackAria: "Models used",
    caption: "Model usage",
    columnModel: "Model",
    columnTokens: "Tokens",
    columnCost: "Cost",
    columnShare: "Share",
    tierLine: "{tokens} · {share}",
  },

  clients: {
    none: "None recorded.",
  },

  today: {
    heading: "Today",
    usageAria: "{heading} usage",
    messages: "Messages",
    empty: "Nothing recorded for this day yet.",
    clientMessages: "{count, plural, one {# msg} other {# msg}}",
  },

  breakdown: {
    heading: "Token mix",
    description: "Distribution across input, output, cache, and reasoning tokens.",
    barAria: "Token distribution: {list}",
  },

  habits: {
    heading: "Coding patterns",
    range: "Latest 12 months",
    topWeekday: "Most productive day",
    topWeekdayMeta: "{tokens} · {share}% of the total",
    biggestDay: "Biggest day",
    biggestDayMeta: "{date} · {cost}",
    byWeekday: "Tokens by weekday",
    barTitle: "{weekday}: {tokens}",
  },

  devices: {
    heading: "Devices · all-time",
    caption: "Usage by device",
    columnDevice: "Device",
    columnTokens: "Tokens",
    columnCost: "Cost",
    columnActiveDays: "Active days",
    lastSubmitted: "Last submitted {time}",
    cloudSessions: "{count, plural, one {# session} other {# sessions}} · ",
  },

  /** Speed-tier vocabulary, shared by the day card, the model table and the graph. */
  speed: {
    standard: "Standard",
    fast: "Fast",
    ultrafast: "Ultrafast",
    unknown: "Unknown",
    reported: "Reported",
    selected: "Selected",
    tooltip: "{label} · {sources}",
  },

  social: {
    aria: "Social links",
    opensInNewTab: "{provider} (opens in new tab)",
    website: "Website",
    twitter: "X (Twitter)",
    linkedin: "LinkedIn",
    instagram: "Instagram",
    facebook: "Facebook",
    mastodon: "Mastodon",
    bluesky: "Bluesky",
    youtube: "YouTube",
    twitch: "Twitch",
    reddit: "Reddit",
    npm: "npm",
  },

  graph: {
    heading: "Contributions",
    description: "Daily token activity across the available history.",
    rangeLabel: "Contribution date range",
    recentYear: "Recent year",
    viewLabel: "Contribution graph view",
    activeDays: "{count, plural, one {# active day} other {# active days}}",
    noActivity: "No activity yet",
    cellTitle: "{date}: {count, plural, one {# token} other {# tokens}}",
    accessibleDetail:
      "Highest activity: {highest}. {days, plural, one {# active day used tokens with no recorded cost.} other {# active days used tokens with no recorded cost.}}",
    noActivityDetail: "No active contribution days are available.",
    gridLabel: "Daily token contributions",
    isometricLabel: "Isometric daily token contributions",
    color: "Color",
    colorLabel: "Contribution graph color",
    legend: "Contribution intensity, low to high",
    low: "Low",
    high: "High",
    empty: "No contribution data is available.",
    instructions:
      "Use arrow keys to inspect adjacent days, Home and End to jump to the range boundaries, Enter or Space to select the detailed day breakdown, and Escape to close the floating tooltip.",
  },

  day: {
    eyebrow: "Day breakdown",
    close: "Close day breakdown",
    totalTokens: "Total tokens",
    cost: "Cost",
    messages: "Messages",
    categories: "Token categories",
    clients: "Clients",
    clientsAndModels: "Clients and models",
    clientListAria: "Client and model details",
    noDetail: "No client or model detail was recorded for this day.",
    more: "+{count} more",
    clickForDetail: "Click for detail",
  },

  palette: {
    green: "Green",
    halloween: "Halloween",
    teal: "Teal",
    blue: "Blue",
    pink: "Pink",
    purple: "Purple",
    orange: "Orange",
    monochrome: "Monochrome",
    ylgnbu: "YlGnBu",
  },

  usage: {
    description: "Model activity, grouped by coding provider.",
    heading: "Usage over time",
    metricLabel: "Usage metric",
    tokens: "Tokens",
    cost: "Cost",
    /** Lower case, for the middle of a sentence. */
    metricTokens: "tokens",
    metricCost: "cost",
    display: "Display",
    displayLabel: "Usage display",
    average: "{days}d average",
    daily: "Daily",
    newestFirstTitle: "Show newest activity on the left",
    newestFirst: "Newest first",
    provider: "Provider",
    providerLabel: "Usage provider",
    all: "All",
    rangeTotal: "Range total {metric}",
    chartAria: "Interactive {metric} chart",
    dateRangeAria: "Chart date range",
    empty: "No usage data yet.",
    instructions:
      "Use Left Arrow and Right Arrow to inspect adjacent days. Use Home and End to jump to the first or last day. Press Escape to close the inspection.",
    modelsLegend: "Usage models",
    more: "+{count} more",
    pinnedAria: "Usage on {date}",
    chartTitle: "{mode} {metric} usage by model and provider",
    chartDescription: "{title} from {start} to {end}. Raw range total: {total}.",
    chartDescriptionNewestFirst:
      "{title} from {start} to {end}, displayed newest first. Raw range total: {total}.",
    noStartDate: "no start date",
    noEndDate: "no end date",
    announcement: "{date}, {mode}: {total}. {rows}",
    announcementMore:
      "{date}, {mode}: {total}. {rows}, plus {count, plural, one {# more model} other {# more models}}",
    costByProvider: "Cost by provider",
    breakdownTotal: "Total {metric}",
    moreModels: "+{count, plural, one {# more model} other {# more models}}",
    moreProviders:
      "+{count, plural, one {# more provider} other {# more providers}}",
    unattributed: "Unattributed",
    unattributedUsage: "Unattributed usage",
    unknownModel: "Unknown model",
    synthetic: "Synthetic",
    providerRemainder: "{provider} remainder",
    other: "Other",
    otherCount: "Other ({count})",
  },

  embed: {
    title: "Embed @{username}",
    description: "Customize the live card, then copy it into GitHub or any HTML page.",
    close: "Close embed dialog",
    preview: "Live preview",
    previewAlt: "Tokens README embed preview for {name}",
    controlsAria: "Embed customization",
    settings: "Card settings",
    template: "Template",
    view: "View",
    theme: "Theme",
    dark: "Dark",
    light: "Light",
    accent: "Accent color",
    defaultAccent: "Default accent color",
    defaultSwatch: "Default",
    accentFor: "{name} accent color",
    ranking: "Ranking",
    rankFormat: "Rank format",
    rankPlain: "Plain",
    rankPercent: "Percent",
    rankTotal: "Total",
    numberFormat: "Number format",
    layout: "Layout",
    full: "Full",
    compact: "Compact",
    graph: "Contribution graph",
    off: "Off",
    on: "On",
    today: "Today's usage",
    tokenFormat: "Token format",
    costFormat: "Cost format",
    markdownSnippet: "Markdown snippet",
    imageUrl: "Image URL",
    html: "HTML",
    copyMarkdown: "Copy markdown",
    viewProfile: "View profile",
    copied: "{label} copied",
    copyFailed: "Failed to copy {label}",
    labelImageUrl: "Image URL",
    labelHtml: "HTML snippet",
    labelMarkdown: "Markdown snippet",
  },

  embedTemplate: {
    classic: "Overview",
    minimal: "Token focus",
    terminal: "Readout",
    graph: "Contributions",
    orbit: "Rank focus",
    vitals: "Activity summary",
    blueprint: "Detailed stats",
    receipt: "Compact list",
    pulse: "Usage pulse",
    detailed: "Today breakdown",
  },
};

export default messages;
