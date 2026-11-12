const messages = {
  meta: {
    title: "Hall of Shame - Tokens",
    description:
      "Accounts banned from the Tokens leaderboard for submitting fraudulent usage data.",
    ogTitle: "Hall of Shame — Tokens",
    ogDescription: "Accounts banned for submitting fraudulent usage data.",
    ogImageTitle: "Hall of Shame",
    ogImageSubtitle: "Accounts banned for submitting fraudulent usage data.",
  },
  title: "Hall of Shame",
  description:
    "The leaderboard only works if the numbers on it are real. Accounts listed here submitted fraudulent usage data and are permanently banned: they cannot sign in or submit again, and none of their data counts toward any ranking. Their records are kept as evidence, and usernames are partially masked so a ban cannot double as advertising.",
  bannedAccount: {
    title: "Account banned",
    description: "This account has been banned and can no longer sign in.",
  },
  list: {
    count: "{count, plural, =0 {No accounts} one {# account} other {# accounts}}",
    mostRecent: "Most recent first",
    banned: "Banned",
    claimed: "claimed {amount}",
    emptyTitle: "No banned accounts",
    emptyDescription: "Keep it that way.",
  },
  report: {
    title: "Spotted something suspicious?",
    // The address is a link inside the sentence, so it travels as a slot.
    body: "Reports from the community are welcome. If a profile's numbers look fabricated, email {email} — every report is checked against the submitted raw data before anyone is banned.",
  },
};

export default messages;
