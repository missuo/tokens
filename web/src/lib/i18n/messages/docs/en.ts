/**
 * The docs page: install the CLI, the iOS app, everyday commands, private mode,
 * the verified badge, supported clients, architecture and sponsors.
 *
 * Commands, file paths, env var names, URLs and product names are inside the
 * messages but never translated — only the prose around them changes. Anything
 * whose sentence carries markup takes a named slot (`{brewServices}`,
 * `{settings}`, `{shame}`) so a translator can move the link, rather than
 * splitting the sentence around an element.
 *
 * Sections are keyed by what they are about, not by the `id` they scroll to, so
 * renaming an anchor never touches the catalogue.
 */
const messages = {
  meta: {
    title: "Docs - Tokens",
    description:
      "Install the Tokens CLI on macOS, Linux or Windows, and get the iOS app on the App Store.",
    ogTitle: "Docs — Tokens",
    ogImageSubtitle: "Install the Tokens CLI, or get the iOS app.",
  },
  title: "Docs",
  description:
    "Get your AI coding usage onto the leaderboard, from the terminal or from your phone.",

  // The two accessible strings in the snippet rows. `{command}` stays verbatim
  // in every locale because it is what gets pasted into a shell.
  commands: {
    copy: "Copy command: {command}",
    copied: "Copied {command}",
    // One line each, shown beside the command. Kept together so the whole set
    // can be revised without hunting through the page.
    notes: {
      install: "install",
      linkGitHub: "link your GitHub account",
      submitAutomatically: "submit automatically",
      submitUsage: "submit your usage",
      authenticate: "authenticate",
      submitNow: "send usage now",
      submitInBackground: "keep submitting in the background",
      submittedSoFar: "what has been submitted",
      everythingElse: "everything else",
      goPrivate: "go private and print a read token",
    },
  },

  install: {
    title: "Install the CLI",
    description:
      "The CLI scans the AI coding clients already installed on your machine, totals the usage locally, and submits only the totals.",
    macosNote:
      "{brewServices} keeps a background agent running, so your usage stays current without you thinking about it.",
    linuxNote:
      "{tokensServe} runs the submitter in the foreground; pair it with a systemd unit to keep it alive across reboots.",
    windowsNote:
      "Runs straight from npm, so nothing is installed globally. Use a Scheduled Task to submit on a timer.",
  },

  iosApp: {
    title: "iOS app",
    description: "Your rank and usage on your phone, without opening a browser.",
    cardTitle: "Tokens for iOS",
    download: "Download on the App Store",
    liquidGlass:
      "The app is built around Liquid Glass, so it picks up the depth and translucency of iOS itself rather than looking like a web page in a shell.",
    features: {
      shareCards: {
        title: "Share cards",
        body: "Turn a day, a month or an all-time total into a card worth posting, rendered on device.",
      },
      homeWidgets: {
        title: "Home screen widgets",
        body: "Today's tokens and your standing, refreshed in the background.",
      },
      lockWidgets: {
        title: "Lock screen widgets",
        body: "Daily usage, running total and current rank, readable at a glance without unlocking.",
      },
    },
    steps: {
      install: "Open the App Store link above on your iPhone and install Tokens.",
      // Two sentences that only make sense together: what to type, and why the
      // username has to be your own.
      enterUsername:
        "Open the app, then enter a GitHub username. There is no sign-in: every profile on Tokens is public, so the app just reads the one you name. Enter your own username or the widgets will show someone else's usage.",
      addWidgets: "Long-press your Home or Lock screen to add the widgets.",
    },
  },

  // Apps someone else builds on the public profile API that this project
  // vouches for. Product names, platforms and the author's name stay verbatim.
  recognizedClients: {
    title: "Recognized clients",
    description: "Apps built by other people that Tokens endorses for viewing your usage.",
    badge: "Recognized client",
    nowdex: {
      body: "Nowdex tracks how much of your Codex, Claude, Cursor and other plan quotas is left, on Mac, iPhone and iPad, with Home Screen widgets and a menu bar item. It shows Tokens usage too — tokens, cost and messages — alongside those quotas, and Tokens recognizes it as a client for viewing that usage. Thanks to {author} for building it. Nowdex is his app, with its own support and privacy policy; it reads the public profile API and never needs your Tokens login.",
    },
  },

  everyday: {
    title: "Everyday use",
    description: "Five commands cover the whole workflow.",
  },

  privateMode: {
    title: "Private mode",
    description:
      "Track your usage without publishing it. A private account is left off the leaderboard, and its profile, devices, badges and embeds answer as if it did not exist.",
    toggle: {
      title: "Turning it on and off",
      body: "Log in with {flag}, or run it again on a machine that is already logged in, to switch the account to private. The Private mode switch in {settings} does the same and is the only way back to public: a submit token in CI can hide your profile but never expose it. You still see your own profile while signed in.",
    },
    readTokens: {
      title: "Read tokens",
      body: "A read token ({token}) unlocks a private profile for whoever holds it — the iOS app, a script, a badge you still want to show. Send it as {header}, or append {query} to a badge or embed URL. {loginCommand} prints the first one; create and revoke more in Settings. They can only read — never submit or change anything — and are shown once, then stored only as a hash.",
    },
    deviceNames: {
      title: "Device names",
      body: "Public or private, other people only ever see your device names masked — the first and last two characters, like {example} — since a device name is often a hostname. You see them in full on your own profile and in Settings.",
    },
  },

  verifiedBadge: {
    title: "The verified badge",
    description:
      "A small check next to a name on the leaderboard. It says the account is a real, findable person — nothing more.",
    howToGet: {
      title: "How to get it",
      // `links` and `settings` keep their own keys: each is a phrase the slot
      // carries as text, so it has to be translated rather than passed through.
      body: "Add at least {links} to your GitHub profile — the “Social accounts” fields in {settings}. Any two count: a personal site, X, LinkedIn, Mastodon, YouTube. That is the whole rule.",
      links: "two social links",
      settings: "GitHub profile settings",
    },
    timing: {
      title: "When it appears",
      body: "Links are re-read once a day, at 03:20 UTC. Adding them now means the badge appears on the next run rather than immediately — signing out and back in does not speed it up. Dropping below two links removes it on the same schedule.",
    },
    why: {
      title: "Why two links",
      body: "A leaderboard attracts throwaway accounts. Filling in two social fields is trivial for someone who already exists online and tedious to fake at scale, which is all the badge claims. It is not an identity check, and it has no effect on ranking — inflated numbers are handled separately, by the submission checks and the {shame}.",
    },
  },

  clients: {
    title: "Supported clients",
    description:
      "The CLI scans whatever is already on your machine — nothing to configure per client.",
    // `{count}` is the length of the grid below; the number is never written out.
    count:
      "All {count} of these are detected automatically — if it is installed and has written sessions, it is counted.",
    paseo:
      "Paseo usage is counted under the underlying supported provider, such as Claude Code or Codex CLI. Tokens also discovers absolute Claude and Codex data-directory overrides in Paseo's provider configuration. Run Tokens on the Paseo daemon host to submit those sessions.",
  },

  architecture: {
    title: "Architecture",
    description:
      "What runs where. The repository is public so this can be checked rather than taken on trust.",
    site: {
      title: "The site and the API",
      body: "Next.js, deployed to Cloudflare Workers through OpenNext — one Worker serves both the pages and the API, with no origin server behind it. Static assets, the share cards, and the pages a signed-out reader sees are cached at the edge, so most requests are answered without running any code at all.",
    },
    database: {
      title: "The database",
      body: "{neon} (Postgres), reached through Cloudflare Hyperdrive, which keeps warm pooled connections beside the database so a page issuing several queries does not pay a fresh handshake for each. The Worker is pinned to the same region: a request crosses the ocean once, and every query after that is a local hop. Schema changes go through Drizzle migrations applied at build time.",
    },
    cli: {
      title: "The CLI",
      body: "Rust, distributed as a prebuilt binary per platform through npm. It reads the session files your clients already write, totals them on your machine, and sends only the totals — token counts, model names, timestamps. Prompts, completions and file contents never leave the machine. {dryRun} prints exactly what would be uploaded.",
    },
    caching: {
      title: "Caching and scheduled work",
      body: "Rendered pages live in R2, with Durable Objects tracking which tags a submission invalidates — so your own numbers update the moment you submit rather than on a timer. The daily badge refresh runs as a Worker cron trigger, in-process, with no external scheduler holding a key.",
    },
  },

  sponsors: {
    title: "Sponsors",
    description:
      "Two companies pay for infrastructure this project does not. Neither asked for anything in return, and neither has any say in what the leaderboard reports.",
    vps: {
      // The mark sits before the title, so the message opens with the slot.
      title: "{vps} — the server",
      p1: "V.PS, run by {xtom}, provide the Tokyo server this site runs on.",
      p2: "The routing is the point of it rather than the hardware. A large share of the people reading this leaderboard are in mainland China, where the path out matters more than anything waiting at the far end — and no amount of caching fixes a slow path to the cache. In their own words: every plan takes the best route each carrier offers, so connections from mainland China stay fast around the clock.",
      p3: "If you want the same thing, they sell it at {vpsHosting}.",
    },
    neon: {
      title: "{neon} — the database",
      p1: "Neon provide the serverless Postgres that holds every submission on this site.",
      p2: "What that pays for is the decision not to approximate. The leaderboard is computed from the actual daily rows on every read, not from a nightly rollup that would be cheaper and slightly wrong; a submission changes your position within the minute; and every figure on a profile can be traced back to a row rather than to a counter someone remembered to increment. Tokens is free to use and free to self-host, so none of that is funded by the people using it.",
    },
  },
};

export default messages;
