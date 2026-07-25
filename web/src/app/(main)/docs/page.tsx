import type { Metadata } from "next";
import { LayoutGridIcon, LockIcon, Share2Icon } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CommandBlock, type DocCommand } from "@/components/docs/CommandBlock";
import { BrandGlyph } from "@/components/profile/ModelIcon";
import { CONTAINER } from "@/components/layout/Container";
import { PageHeader } from "@/components/layout/PageHeader";
import { cn } from "@/lib/utils";

export const metadata: Metadata = {
  title: "Docs - Tokens",
  description:
    "Install the Tokens CLI on macOS, Linux or Windows, and get the iOS app on TestFlight.",
  openGraph: {
    title: "Docs — Tokens",
    description: "Install the Tokens CLI, or get the iOS app.",
    url: "https://tokens.ci",
    siteName: "Tokens",
    images: [
      {
        url: `/api/og?title=Docs&subtitle=Install+the+Tokens+CLI,+or+get+the+iOS+app.`,
        width: 1200,
        height: 630,
      },
    ],
  },
  twitter: { card: "summary_large_image" },
};

const TESTFLIGHT_URL = "https://testflight.apple.com/join/NWmvqqTX";

const MACOS: readonly DocCommand[] = [
  { command: "brew install owo-network/brew/tokens", note: "install" },
  { command: "tokens login", note: "link your GitHub account" },
  { command: "brew services start tokens", note: "submit automatically" },
];

const LINUX: readonly DocCommand[] = [
  { command: "curl -fsSL https://tokens.ci/install.sh | sh", note: "install" },
  { command: "tokens login", note: "link your GitHub account" },
  { command: "tokens serve", note: "submit automatically" },
];

const WINDOWS: readonly DocCommand[] = [
  { command: "bunx tokens-cli@latest login", note: "link your GitHub account" },
  { command: "bunx tokens-cli@latest submit", note: "submit your usage" },
];

/**
 * Platform marks. Apple and Microsoft come from the shared brand set so they
 * follow the text colour; Linux has no entry there, so Tux is drawn inline.
 */
function OsIcon({ name }: { name: "macos" | "linux" | "windows" }) {
  if (name === "linux") {
    return (
      <svg
        viewBox="0 0 24 24"
        width={14}
        height={14}
        fill="currentColor"
        aria-hidden="true"
        className="shrink-0"
        data-icon="inline-start"
      >
        <path d="M12 2c-2.4 0-3.7 1.9-3.7 4.3 0 .9.1 1.7.1 2.4 0 .8-.5 1.5-1.1 2.4C6.4 12.4 5 14.3 5 16.4c0 1 .3 1.8.9 2.4-.3.4-.5.9-.5 1.4 0 1.1 1 1.8 2.4 1.8 1 0 1.8-.3 2.4-.8.5.1 1.1.2 1.8.2s1.3-.1 1.8-.2c.6.5 1.4.8 2.4.8 1.4 0 2.4-.7 2.4-1.8 0-.5-.2-1-.5-1.4.6-.6.9-1.4.9-2.4 0-2.1-1.4-4-2.3-5.3-.6-.9-1.1-1.6-1.1-2.4 0-.7.1-1.5.1-2.4C15.7 3.9 14.4 2 12 2zm-1.5 3.3c.4 0 .8.5.8 1.1s-.4 1.1-.8 1.1-.8-.5-.8-1.1.4-1.1.8-1.1zm3 0c.4 0 .8.5.8 1.1s-.4 1.1-.8 1.1-.8-.5-.8-1.1.4-1.1.8-1.1zM12 8.4c.9 0 1.7.4 1.7.8 0 .2-.2.4-.5.6l-1 .6c-.1.1-.3.1-.4 0l-1-.6c-.3-.2-.5-.4-.5-.6 0-.4.8-.8 1.7-.8z" />
      </svg>
    );
  }
  return (
    <BrandGlyph
      slug={name === "macos" ? "apple" : "microsoft"}
      size={14}
      className="fill-current"
    />
  );
}

function Section({
  id,
  title,
  description,
  children,
}: {
  id: string;
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <section id={id} className="scroll-mt-20">
      <h2 className="text-lg font-semibold tracking-tight">{title}</h2>
      {description && (
        <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
          {description}
        </p>
      )}
      <div className="mt-4">{children}</div>
    </section>
  );
}

function Feature({
  icon: Icon,
  title,
  children,
}: {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex gap-3">
      <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
      <div className="flex flex-col gap-1">
        <span className="text-sm font-medium">{title}</span>
        <span className="text-sm leading-relaxed text-muted-foreground">
          {children}
        </span>
      </div>
    </div>
  );
}

/** Apple's mark. lucide's `AppleIcon` is the fruit, which is not the same
 *  thing and reads as a mistake next to "TestFlight". */
function AppleMark(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M11.932 6.908c.95 0 2.727-1.291 4.595-1.1.782.032 2.976.316 4.388 2.38-.113.069-2.622 1.528-2.593 4.565.034 3.617 3.166 4.828 3.221 4.85-.029.086-.506 1.723-1.658 3.416-1.002 1.463-2.039 2.919-3.675 2.95-1.606.03-2.125-.955-3.96-.955s-2.409.923-3.931.984c-1.581.06-2.78-1.58-3.79-3.037-2.065-2.98-3.64-8.422-1.527-12.087 1.051-1.824 2.93-2.98 4.969-3.009 1.549-.032 3.011 1.043 3.96 1.043zM16.552 0c.153 1.407-.411 2.817-1.251 3.833-.837 1.013-2.214 1.804-3.555 1.7-.185-1.378.495-2.814 1.27-3.712C13.883.805 15.346.05 16.553 0z" />
    </svg>
  );
}

export default function DocsPage() {
  return (
    <main
      className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")}
      id="main-content"
    >
      <div className="mx-auto w-full max-w-[860px]">
      <PageHeader
        title="Docs"
        description="Get your AI coding usage onto the leaderboard, from the terminal or from your phone."
      />

        <div className="flex flex-col gap-12">
        <Section
          id="cli"
          title="Install the CLI"
          description="The CLI scans the AI coding clients already installed on your machine, totals the usage locally, and submits only the totals."
        >
          <Tabs defaultValue="macos">
            <TabsList>
              {/* Icons are checked in rather than hotlinked; currentColor keeps
                  the monochrome marks legible in both themes. */}
              <TabsTrigger value="macos">
                <OsIcon name="macos" />
                macOS
              </TabsTrigger>
              <TabsTrigger value="linux">
                <OsIcon name="linux" />
                Linux
              </TabsTrigger>
              <TabsTrigger value="windows">
                <OsIcon name="windows" />
                Windows
              </TabsTrigger>
            </TabsList>

            <TabsContent value="macos" className="mt-4 flex flex-col gap-3">
              <CommandBlock commands={MACOS} />
              <p className="text-sm leading-relaxed text-muted-foreground">
                <code className="font-mono text-[13px]">brew services</code>{" "}
                keeps a background agent running, so your usage stays current
                without you thinking about it.
              </p>
            </TabsContent>

            <TabsContent value="linux" className="mt-4 flex flex-col gap-3">
              <CommandBlock commands={LINUX} />
              <p className="text-sm leading-relaxed text-muted-foreground">
                <code className="font-mono text-[13px]">tokens serve</code> runs
                the submitter in the foreground; pair it with a systemd unit to
                keep it alive across reboots.
              </p>
            </TabsContent>

            <TabsContent value="windows" className="mt-4 flex flex-col gap-3">
              <CommandBlock commands={WINDOWS} />
              <p className="text-sm leading-relaxed text-muted-foreground">
                Runs straight from npm, so nothing is installed globally. Use a
                Scheduled Task to submit on a timer.
              </p>
            </TabsContent>
          </Tabs>
        </Section>

        <Section
          id="ios"
          title="iOS app"
          description="Your rank and usage on your phone, without opening a browser."
        >
          <Card>
            {/* Stacked on phones. Forced side by side, the title is the only
                flexible item in the row, so at 390px it was squeezed to 53px
                and wrapped onto two lines while the badge and button kept
                their width. */}
            <CardHeader className="flex flex-col items-start gap-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
              <div className="flex flex-wrap items-center gap-2">
                <CardTitle className="text-base">Tokens for iOS</CardTitle>
                <Badge variant="secondary">TestFlight beta</Badge>
              </div>
              {/* Base UI composes via `render`, not Radix's `asChild`.
                  Styled the way Apple's own install buttons are — black with
                  the mark — so it reads as "this goes to Apple". It inverts in
                  dark mode because a black button on a black card disappears. */}
              <Button
                className="w-full shrink-0 border border-transparent bg-black text-white hover:bg-black/85 sm:w-auto dark:bg-white dark:text-black dark:hover:bg-white/90"
                render={
                  <a href={TESTFLIGHT_URL} target="_blank" rel="noopener noreferrer" />
                }
              >
                <AppleMark data-icon="inline-start" />
                Join the TestFlight
              </Button>
            </CardHeader>
            <CardContent className="flex flex-col gap-5">
              <p className="text-sm leading-relaxed text-muted-foreground">
                The app is built around Liquid Glass, so it picks up the depth
                and translucency of iOS itself rather than looking like a web
                page in a shell.
              </p>

              <div className="flex flex-col gap-4">
                <Feature icon={Share2Icon} title="Share cards">
                  Turn a day, a month or an all-time total into a card worth
                  posting, rendered on device.
                </Feature>
                <Feature icon={LayoutGridIcon} title="Home screen widgets">
                  Today&apos;s tokens and your standing, refreshed in the
                  background.
                </Feature>
                <Feature icon={LockIcon} title="Lock screen widgets">
                  Daily usage, running total and current rank, readable at a
                  glance without unlocking.
                </Feature>
              </div>

              <Separator />

              <ol className="flex list-decimal flex-col gap-2 pl-5 text-sm leading-relaxed text-muted-foreground">
                <li>Install Apple&apos;s TestFlight app from the App Store.</li>
                <li>
                  Open the invitation link above on the same device and tap
                  Accept.
                </li>
                <li>Install Tokens from TestFlight, then sign in with GitHub.</li>
                <li>
                  Long-press your Home or Lock screen to add the widgets.
                </li>
              </ol>
            </CardContent>
          </Card>
        </Section>

        <Section
          id="usage"
          title="Everyday use"
          description="Five commands cover the whole workflow."
        >
          <CommandBlock
            commands={[
              { command: "tokens login", note: "authenticate" },
              { command: "tokens submit", note: "send usage now" },
              { command: "tokens serve", note: "keep submitting in the background" },
              { command: "tokens status", note: "what has been submitted" },
              { command: "tokens help", note: "everything else" },
            ]}
          />
        </Section>

        <Section
          id="verified"
          title="The verified badge"
          description="A small check next to a name on the leaderboard. It says the account is a real, findable person — nothing more."
        >
          <div className="flex flex-col gap-4">
            <div className="rounded-lg border p-4">
              <h3 className="text-sm font-medium">How to get it</h3>
              <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
                Add at least <strong className="font-medium text-foreground">two social
                links</strong> to your GitHub profile — the &ldquo;Social accounts&rdquo;
                fields in{" "}
                <a
                  href="https://github.com/settings/profile"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="underline underline-offset-4 hover:text-foreground"
                >
                  GitHub profile settings
                </a>
                . Any two count: a personal site, X, LinkedIn, Mastodon, YouTube.
                That is the whole rule.
              </p>
            </div>

            <div className="rounded-lg border p-4">
              <h3 className="text-sm font-medium">When it appears</h3>
              <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
                Links are re-read once a day, at 03:20 UTC. Adding them now means
                the badge appears on the next run rather than immediately —
                signing out and back in does not speed it up. Dropping below two
                links removes it on the same schedule.
              </p>
            </div>

            <div className="rounded-lg border p-4">
              <h3 className="text-sm font-medium">Why two links</h3>
              <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
                A leaderboard attracts throwaway accounts. Filling in two social
                fields is trivial for someone who already exists online and
                tedious to fake at scale, which is all the badge claims. It is
                not an identity check, and it has no effect on ranking —
                inflated numbers are handled separately, by the submission
                checks and the{" "}
                <a href="/shame" className="underline underline-offset-4 hover:text-foreground">
                  Hall of Shame
                </a>
                .
              </p>
            </div>
          </div>
        </Section>

        <Section
          id="clients"
          title="Supported clients"
          description="The CLI scans whatever is already on your machine — nothing to configure per client."
        >
          <div className="flex flex-col gap-4">
            <p className="text-sm leading-relaxed text-muted-foreground">
              Claude Code, Codex, Cursor, Copilot, Gemini, OpenCode, Kimi, Qwen,
              Amp, Droid, Antigravity, Zed, Kiro, Trae, Warp, Cline, Grok, Junie
              and more. New clients land through upstream parser updates.
            </p>
            <div className="rounded-lg border p-4">
              <h3 className="text-sm font-medium">Orca</h3>
              <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
                Orca runs Codex with its own isolated runtime home rather than
                the shell&apos;s, so a plain scan would miss those sessions
                entirely. The CLI discovers Orca&apos;s runtime directory on
                macOS automatically and counts what it finds under Codex — it is
                the same Codex usage, just stored elsewhere. Nothing to enable.
              </p>
            </div>
          </div>
        </Section>

        <Section
          id="architecture"
          title="Architecture"
          description="What runs where. The repository is public so this can be checked rather than taken on trust."
        >
          <div className="flex flex-col gap-4">
            <div className="rounded-lg border p-4">
              <h3 className="text-sm font-medium">The site and the API</h3>
              <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
                Next.js, deployed to Cloudflare Workers through OpenNext — one
                Worker serves both the pages and the API, with no origin server
                behind it. Static assets and the share cards are cached at the
                edge, so most requests are answered without running any code at
                all.
              </p>
            </div>

            <div className="rounded-lg border p-4">
              <h3 className="text-sm font-medium">The database</h3>
              <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
                Neon (Postgres) in{" "}
                <code className="font-mono text-[13px]">us-west-2</code>, reached
                through Cloudflare Hyperdrive, which keeps warm pooled
                connections beside the database so a page issuing several
                queries does not pay a fresh handshake for each. The Worker is
                pinned to the same region: a request crosses the ocean once, and
                every query after that is a local hop. Schema changes go through
                Drizzle migrations applied at build time.
              </p>
            </div>

            <div className="rounded-lg border p-4">
              <h3 className="text-sm font-medium">The CLI</h3>
              <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
                Rust, distributed as a prebuilt binary per platform through npm.
                It reads the session files your clients already write, totals
                them on your machine, and sends only the totals — token counts,
                model names, timestamps. Prompts, completions and file contents
                never leave the machine.{" "}
                <code className="font-mono text-[13px]">tokens submit --dry-run</code>{" "}
                prints exactly what would be uploaded.
              </p>
            </div>

            <div className="rounded-lg border p-4">
              <h3 className="text-sm font-medium">Caching and scheduled work</h3>
              <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
                Rendered pages live in R2, with Durable Objects tracking which
                tags a submission invalidates — so your own numbers update the
                moment you submit rather than on a timer. The daily badge
                refresh runs as a Worker cron trigger, in-process, with no
                external scheduler holding a key.
              </p>
            </div>
          </div>
        </Section>

        </div>
      </div>
    </main>
  );
}
