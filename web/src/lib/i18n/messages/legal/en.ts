/**
 * Legal copy is the one place where a translation is not a stylistic choice:
 * a clause that reads wrong is a clause that does not hold. Slots carry the
 * inline markup (`{email}`, `{ci}`, `{code}`) so a translator can move the
 * link, and every paragraph is its own key so a change to one does not force
 * the other 60 to be re-reviewed.
 */
const messages = {
  lastUpdated: "Last updated {date}",
  // Shown only above a translated policy; the English page is the binding one.
  // `{english}` is a link to it.
  translationNotice:
    "This translation is provided for convenience. If it differs from the {english}, the English version prevails.",
  englishVersion: "English version",

  terms: {
    title: "Terms of Service",
    description: "The rules for using Tokens, and what we do and do not promise.",
    updated: "25 July 2026",
    metaDescription: "The rules for using Tokens, and what we do and do not promise.",
    sections: {
      agreement: {
        heading: "Agreement",
        body: "These terms are between you and Vincent Yang, the operator of Tokens ({ci}), and cover the website, the public API, the command-line tool and the iOS app. Using any of them means you accept these terms. If you do not accept them, do not use Tokens.",
      },
      whoMayUse: {
        heading: "Who may use it",
        p1: "You must be at least 13 years old, and at least 16 where your jurisdiction sets that threshold. You need a GitHub account, and you must be the person that account belongs to. One person, one account.",
        p2: "You are responsible for what happens under your account and for keeping your API tokens private. An API token can submit usage on your behalf; treat it like a password. If you think one has leaked, revoke it in Settings.",
      },
      whatItIs: {
        heading: "What Tokens is",
        p1: "A public leaderboard for AI coding usage. Our CLI reads the session files your AI coding tools write, totals them on your machine, and uploads only the totals, which we then rank and display.",
        p2: "It is free. There is no paid tier, and no payment is ever taken.",
      },
      publicData: {
        heading: "Your data is public",
        p1: "Publishing your standing is the point of the service. By submitting, you agree that your username, display name, avatar, usage totals, per-client and per-model breakdown, daily history, device names and rank are public — visible to anyone, served through our public API, and rendered into embeddable cards and share images that others may post elsewhere.",
        p2: "You keep whatever rights you have in that data. You grant us a worldwide, non-exclusive, royalty-free licence to host, reproduce and display it for the purpose of operating and promoting Tokens. The licence ends when you delete the data, except for copies already made by third parties and for the ban records described below.",
        p3: "Do not submit usage under an account whose GitHub profile carries content you are not entitled to publish.",
      },
      honestNumbers: {
        heading: "Honest numbers",
        intro: "A leaderboard is worth nothing if its numbers are invented. Usage is self-reported and we cannot cryptographically prove any of it, so this rule carries the whole service. You must not:",
        item1: "Fabricate, inflate or otherwise manipulate submitted usage.",
        item2: "Modify the CLI, or write your own client, in order to report usage you did not incur.",
        item3: "Submit another person's usage as your own, or copy figures from another profile.",
        item4: "Generate usage for the sole purpose of moving up the ranking.",
        item5: "Operate multiple accounts to inflate a single person's standing.",
        outro: "Reading the CLI, forking it, and building your own tools on the open API are all fine and encouraged. The line is reporting usage that did not happen.",
      },
      mustNot: {
        heading: "Other things you must not do",
        item1: "Break the law, or use Tokens to harass, defame or impersonate anyone.",
        item2: "Attack the service — scrape at a rate that degrades it, attempt to overwhelm it, or probe for vulnerabilities without permission.",
        item3: "Try to access another user's account, session or API tokens.",
        item4: "Circumvent a ban, including by creating a replacement account.",
        outro: "If you find a security vulnerability, please report it to {email} before disclosing it. We will not pursue good-faith researchers who do.",
      },
      enforcement: {
        heading: "Enforcement and the Hall of Shame",
        p1: "When a submission looks fabricated we check it against the raw submitted data. If we conclude an account submitted fraudulent usage, we ban it: it can no longer sign in or submit, and none of its data counts toward any ranking.",
        p2: "Banned accounts are listed publicly on the {shameLink} with their username partially masked, and their records are retained as evidence of the ban rather than deleted. We mask the name so that a ban cannot double as publicity.",
        p3: "If you believe a ban was a mistake, email {email} and we will look at it again. Reports of suspicious profiles go to the same address.",
      },
      accuracy: {
        heading: "Accuracy",
        body: "Cost figures are estimates. They are computed from public pricing data (LiteLLM, OpenRouter and models.dev) and the token counts your tools recorded; they are not invoices, they do not account for your plan, discounts or credits, and they will not match what a provider actually bills you. Rankings depend on self-reported data and may be wrong or incomplete. Do not rely on any of it for financial or business decisions.",
      },
      software: {
        heading: "The software",
        body: "Tokens is open source under the MIT licence. Your use of the source code is governed by that licence, not by these terms — including the warranty disclaimer it contains. These terms govern your use of the hosted service at tokens.ci.",
      },
      availability: {
        heading: "Availability and changes",
        p1: "We may change, suspend or discontinue any part of Tokens at any time, and we may change these terms. Material changes will be reflected in the date at the top of this page. Continuing to use Tokens after a change means you accept it.",
        p2: "You may stop using Tokens whenever you like and delete your account from Settings. We may suspend or terminate an account that breaches these terms.",
      },
      noWarranty: {
        heading: "No warranty",
        body: "Tokens is provided “as is” and “as available”, without warranty of any kind, express or implied, including any implied warranty of merchantability, fitness for a particular purpose, title or non-infringement. We do not warrant that the service will be uninterrupted, secure, or free of errors, or that any figure it displays is accurate.",
      },
      liability: {
        heading: "Limitation of liability",
        p1: "To the fullest extent permitted by law, neither the operator nor any contributor is liable for any indirect, incidental, special, consequential, exemplary or punitive damages, or for any loss of profits, revenue, data, goodwill or reputation, arising out of or related to your use of Tokens, on any theory of liability, even if advised of the possibility of such damages. Our total aggregate liability arising out of or related to these terms or the service shall not exceed one hundred United States dollars (US$100).",
        p2: "Some jurisdictions do not allow the exclusion of certain warranties or the limitation of certain damages. Where that is the case, the above applies to the fullest extent the law allows, and nothing here limits liability that cannot lawfully be limited.",
      },
      indemnity: {
        heading: "Indemnity",
        body: "You agree to indemnify and hold harmless the operator and contributors from any claim, demand, loss or expense, including reasonable legal fees, arising out of your use of Tokens, the data you submit, or your breach of these terms.",
      },
      governingLaw: {
        heading: "Governing law",
        body: "These terms are governed by the laws of the State of California, without regard to its conflict-of-laws rules. You and we agree to the exclusive jurisdiction of the state and federal courts located in California for any dispute that is not otherwise resolved, and each side waives any objection to venue there. Nothing in this clause removes any right you have to bring a claim in a small-claims court, or any non-waivable right under the law of your place of residence.",
      },
      everythingElse: {
        heading: "Everything else",
        body: "If any provision of these terms is held unenforceable, the rest remains in force and the unenforceable part is limited to the minimum extent necessary. Our failure to enforce a provision is not a waiver of it. These terms, together with the {privacyLink}, are the entire agreement between you and us regarding the service. You may not assign them; we may assign them in connection with a transfer of the project.",
      },
      contact: { heading: "Contact" },
    },
  },

  privacy: {
    title: "Privacy Policy",
    description: "What we collect, why we collect it, who it goes to, and how to get it removed.",
    updated: "11 November 2026",
    metaDescription: "What Tokens collects, why, who it goes to, and how to get it deleted.",
    sections: {
      shortVersion: {
        heading: "The short version",
        p1: "Tokens counts how many tokens you spend on AI coding tools and ranks that publicly. To do it we need your GitHub identity and the usage totals our CLI computes on your machine.",
        p2: "The CLI reads the session files your AI coding tools already write to disk and adds them up locally. Only the totals are uploaded — token counts, cost figures, model names, client names and timestamps. The contents of those sessions never leave your machine: not your prompts, not the model's replies, not your code, not your file paths, not your repository or directory names. The one optional extra is the names of your configured MCP servers, which are sent only if you opt in.",
        p3: "You can verify this rather than trust it. The CLI is open source, one parser per client, and {dryRun} prints the exact payload without uploading anything.",
      },
      whoWeAre: {
        heading: "Who we are",
        body: "Tokens ({ci}) is an independent open-source project operated by Vincent Yang. For anything in this policy, including privacy requests, write to {email}.",
      },
      whatWeCollect: {
        heading: "What we collect",
        p1Strong: "From GitHub, when you sign in.",
        p1: "We use GitHub OAuth with the {scopes} scopes and store your GitHub numeric ID, username, display name, avatar URL, email address, and the public profile links GitHub exposes (such as a personal site or social handles). We never receive your GitHub password, and we do not request access to your repositories.",
        p2Strong: "From the CLI, when you submit.",
        p2: "Daily totals of tokens and estimated cost, broken down by AI coding client and by model, with the dates they belong to. Each submission is attributed to a device you name, so the same day reported from a laptop and a desktop is not double-counted.",
        p3Strong: "To keep you signed in.",
        p3: "A session record holding a hashed session token, its expiry, and the browser user-agent string of the session. Personal API tokens you create for the CLI, with the name you gave them and when they were last used.",
        p4Strong: "If an account is banned.",
        p4: "The time of the ban and the reason for it.",
        p5Strong: "Your MCP server names, only if you opt in.",
        p5: "This is off by default. If you run {enable}, or pass {flag} to a single submit, each submission includes the names of the MCP servers configured for the clients that submission covers: Claude Code ({claudePath}), Cursor ({cursorPath}), Kiro ({kiroPath}) and OpenCode skills ({opencodePath}). Only the names are read, never the commands, arguments, URLs, credentials or environment values beside them, and no other client's configuration is opened. We use the names only to show the tools you use on your profile; counting usage does not need them.",
      },
      neverCollect: {
        heading: "What we never collect",
        item1: "Prompts, completions, or any conversation content.",
        item2: "Source code, file contents, file paths, repository names or directory names.",
        item3: "Your GitHub repositories, issues, or any private GitHub data.",
        item4: "Payment details. Tokens is free and takes no payments.",
        item5: "MCP server names, unless you opt in, and never the commands, URLs, credentials or settings configured for them.",
      },
      whyWeUse: {
        heading: "Why we use it",
        item1: "To build your public profile and place you on the leaderboard — the purpose of the service.",
        item2: "To keep you signed in and to authenticate CLI submissions.",
        item3: "To detect and act on fabricated submissions, which is what keeps the ranking meaningful.",
        item4: "To answer you when you contact us.",
        outro: "We do not use your data to train machine-learning models, and we do not profile you for advertising.",
      },
      whatIsPublic: {
        heading: "What is public",
        p1: "Tokens is a public leaderboard, so this matters more here than on most sites. Your username, display name, avatar, usage totals, per-client and per-model breakdown, daily history, device names (shown masked, e.g. {example}) and rank are visible to anyone, including people who are not signed in, and are served through our public API, embeddable cards and share images.",
        p2: "Unless you turn on private mode, in Settings or with {command}. A private account is left off the leaderboard, and its profile, usage data, devices and embeddable cards are shown only to you while signed in, and to anyone you give one of your read tokens to.",
        p3: "Your email address is never shown publicly and is not part of any API response.",
        p4: "Because profiles are public, the iOS app needs no sign-in: it simply reads the public profile of whichever GitHub username you enter.",
        p5: "MCP server names you opt in to sharing are shown in full on your profile and in its API response. Private mode hides them from other people along with the rest of the profile, but they are still sent to and stored by Tokens. To stop sending them at all, leave the option off.",
      },
      cookies: {
        heading: "Cookies",
        p1: "One cookie, {cookie}, set only after you sign in. It is HTTP-only, Secure, SameSite=Lax, and expires after 30 days. It exists solely to keep you signed in.",
        p2: "There are no advertising cookies, no analytics cookies, and no third-party trackers on this site. We do not run Google Analytics or any comparable product.",
      },
      sharing: {
        heading: "Who we share it with",
        p1: "We do not sell personal information, and we do not share it for cross-context behavioural advertising. We have never done either.",
        p2: "We rely on a small number of infrastructure providers who process data on our behalf:",
        provider1Name: "V.PS",
        provider1: "— the server the site, the API and the database run on. This is where the records above actually live.",
        provider2Name: "Cloudflare",
        provider2: "— the CDN and network protection in front of that server. It routes traffic and keeps copies of pages that are public anyway; it no longer runs the application or holds a copy of the database, as it did while this site was deployed on Workers. It does still terminate TLS, so like any reverse proxy it can see requests as they pass through.",
        provider3Name: "Neon",
        provider3: "— the managed PostgreSQL that held these records until the move, and which keeps a copy for recovery.",
        provider4Name: "GitHub",
        provider4: "— authentication, and the source of the profile fields and avatar images we display.",
        p3: "We may also disclose information where the law requires it, or where it is necessary to investigate fraud or abuse of the leaderboard.",
      },
      whereStored: {
        heading: "Where it is stored",
        body: "Data is stored in the United States. If you use Tokens from outside the United States, you are sending your information there.",
      },
      retention: {
        heading: "How long we keep it",
        p1: "Profile and usage data is kept while your account exists, because a leaderboard with a gap in its history is not a leaderboard. Sessions expire after 30 days. Expired device-authorisation codes are short-lived and cleared automatically.",
        p2: "Deleting your account removes your profile, submissions, daily breakdown, devices, sessions and API tokens. Records of banned accounts are retained as evidence of the ban, with usernames partially masked.",
      },
      yourChoices: {
        heading: "Your choices",
        intro: "These are built into the product; you do not have to ask us:",
        item1Strong: "Delete your usage data",
        item1: "but keep your account — Settings, or {command} from the CLI.",
        item2Strong: "Delete your account",
        item2: "and everything attached to it — Settings.",
        item3Strong: "Revoke API tokens",
        item3: "at any time — Settings.",
        item4Strong: "Stop submitting",
        item4: "— stop the background service; nothing is sent unless the CLI sends it.",
        item5Strong: "Stop sharing MCP server names",
        item5: "— run {command}. Your next submission sends no names and clears the list stored on your profile.",
      },
      california: {
        heading: "California privacy rights",
        p1: "If you are a California resident, the California Consumer Privacy Act as amended by the CPRA gives you the right to know what personal information we collect and how we use it, to request a copy of it, to request correction of inaccurate information, to request deletion, and to opt out of the sale or sharing of personal information.",
        p2: "On that last point there is nothing to opt out of: {strong}, and we do not process sensitive personal information for purposes requiring a right to limit.",
        p2Strong: "we do not sell or share personal information",
        p3: "The categories we collect map to the CCPA as: identifiers (GitHub ID, username, email, avatar URL); internet or network activity (submission timestamps, session user-agent); and commercial-adjacent information in the form of your self-reported AI tool usage totals. Sources, purposes and recipients are described in the sections above.",
        p4: "Use the controls in Settings, or email {email}. We will verify a request by asking you to confirm control of the GitHub account in question. We will not discriminate against you for exercising any of these rights. You may use an authorised agent; we may ask for proof of their authority.",
      },
      otherStates: {
        heading: "Other US state privacy laws",
        body: "Residents of states with comparable laws — including Virginia, Colorado, Connecticut, Utah and Texas — have similar rights of access, correction, deletion and portability. The same contact address and the same Settings controls serve those requests. Where a state law provides an appeal process for a refused request, you may appeal by replying to our response.",
      },
      children: {
        heading: "Children",
        body: "Tokens is not directed to children. You must be at least 13 to use it, and at least 16 if you are in a jurisdiction that sets that threshold. We do not knowingly collect information from children below those ages. If you believe a child has created an account, write to {email} and we will remove it.",
      },
      security: {
        heading: "Security",
        body: "Session tokens are stored hashed, never in plaintext. Traffic is served over HTTPS. The session cookie is HTTP-only, so page scripts cannot read it. No system is perfectly secure, and we cannot guarantee absolute security, but if we discover a breach affecting your personal information we will notify affected users and any regulator the law requires.",
      },
      changes: {
        heading: "Changes",
        body: "If this policy changes materially we will update the date at the top of this page and, where the change is significant, say so on the site. Continuing to use Tokens after a change means you accept the updated policy.",
      },
      contact: { heading: "Contact" },
    },
  },
};

export default messages;
