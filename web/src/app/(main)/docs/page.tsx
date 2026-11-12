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
import {
  BREW_INSTALL_COMMAND,
  SOURCE_DISPLAY_NAMES,
  SOURCE_LOGOS,
  SUPPORTED_CLIENTS,
} from "@/lib/constants";
import { PageHeader } from "@/components/layout/PageHeader";
import { ogImageUrl } from "@/lib/og/ogImage";
import { cn } from "@/lib/utils";
import { getTranslator } from "@/lib/i18n/server";
import { localizeHref } from "@/lib/i18n/config";
import { richText } from "@/lib/i18n/rich";

export async function generateMetadata(): Promise<Metadata> {
  const { locale, t } = await getTranslator();
  const description = t("docs.meta.description");

  return {
    title: t("docs.meta.title"),
    description,
    openGraph: {
      title: t("docs.meta.ogTitle"),
      description,
      url: "https://tokens.ci",
      siteName: "Tokens",
      images: [
        {
          url: ogImageUrl({ title: t("docs.title"), subtitle: t("docs.meta.ogImageSubtitle"), locale }),
          width: 1200,
          height: 630,
        },
      ],
    },
    twitter: { card: "summary_large_image" },
  };
}

// Storefront-agnostic App Store link: Apple redirects it to the visitor's own
// region, so it works everywhere without hardcoding /us/ or /cn/.
const APP_STORE_URL = "https://apps.apple.com/app/id6793029466";

/**
 * What the Supported clients grid renders.
 *
 * Scanned clients plus agent platforms whose native provider sessions are
 * discovered by the scanner. Their usage stays attributed to those providers.
 */
const CLIENT_GRID: ReadonlyArray<{ id: string; name: string; logo: string }> = [
  ...SUPPORTED_CLIENTS.map((client) => ({
    id: client,
    name: SOURCE_DISPLAY_NAMES[client],
    logo: SOURCE_LOGOS[client],
  })),
  { id: "orca", name: "Orca", logo: "/clients/client-orca.png" },
  { id: "paseo", name: "Paseo", logo: "/clients/client-paseo.svg" },
].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));

// The notes are catalogue keys rather than text: the lists sit at module scope,
// where there is no translator, and `CommandBlock` resolves them at render.
const MACOS: readonly DocCommand[] = [
  { command: BREW_INSTALL_COMMAND, noteKey: "docs.commands.notes.install" },
  { command: "tokens login", noteKey: "docs.commands.notes.linkGitHub" },
  { command: "brew services start tokens", noteKey: "docs.commands.notes.submitAutomatically" },
];

const LINUX: readonly DocCommand[] = [
  { command: "curl -fsSL https://tokens.ci/install.sh | sh", noteKey: "docs.commands.notes.install" },
  { command: "tokens login", noteKey: "docs.commands.notes.linkGitHub" },
  { command: "tokens serve", noteKey: "docs.commands.notes.submitAutomatically" },
];

const WINDOWS: readonly DocCommand[] = [
  { command: "bunx tokens-cli@latest login", noteKey: "docs.commands.notes.linkGitHub" },
  { command: "bunx tokens-cli@latest submit", noteKey: "docs.commands.notes.submitUsage" },
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
 *  thing and reads as a mistake next to "App Store". */
function AppleMark(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M11.932 6.908c.95 0 2.727-1.291 4.595-1.1.782.032 2.976.316 4.388 2.38-.113.069-2.622 1.528-2.593 4.565.034 3.617 3.166 4.828 3.221 4.85-.029.086-.506 1.723-1.658 3.416-1.002 1.463-2.039 2.919-3.675 2.95-1.606.03-2.125-.955-3.96-.955s-2.409.923-3.931.984c-1.581.06-2.78-1.58-3.79-3.037-2.065-2.98-3.64-8.422-1.527-12.087 1.051-1.824 2.93-2.98 4.969-3.009 1.549-.032 3.011 1.043 3.96 1.043zM16.552 0c.153 1.407-.411 2.817-1.251 3.833-.837 1.013-2.214 1.804-3.555 1.7-.185-1.378.495-2.814 1.27-3.712C13.883.805 15.346.05 16.553 0z" />
    </svg>
  );
}

/**
 * Code spans inside prose. Every sentence that carries one is a message with a
 * `{...}` slot, so the span lands wherever the translation puts it — Chinese
 * and Japanese do not keep the English word order around it.
 */
const CODE = "font-mono text-[13px]";

const LINK = "underline underline-offset-4 hover:text-foreground";

const BRAND_LINK = "font-medium text-foreground underline underline-offset-4";

function ExternalLink({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noopener noreferrer" className={LINK}>
      {children}
    </a>
  );
}

interface ThirdPartyPerson {
  label: string;
  href: string;
}

/**
 * Apps other people build on Tokens data. Names and handles stay verbatim in
 * every locale; only the byline and description are translated. To be listed,
 * authors open an issue on the repo.
 */
const THIRD_PARTY_APPS: ReadonlyArray<{
  id: string;
  name: string;
  url: string;
  urlLabel: string;
  author: ThirdPartyPerson;
  plugin?: { href: string; author: ThirdPartyPerson };
  bylineKey: string;
  bodyKey: string;
}> = [
  {
    id: "nowdex",
    name: "Nowdex",
    url: "https://nowdex.app",
    urlLabel: "nowdex.app",
    author: { label: "Randy Lu (@randyloop)", href: "https://x.com/randyloop" },
    bylineKey: "docs.thirdPartyApps.byline",
    bodyKey: "docs.thirdPartyApps.nowdex.body",
  },
  {
    id: "metrix",
    name: "Metrix.Bar",
    url: "https://metrix.bar",
    urlLabel: "metrix.bar",
    author: { label: "@miantiao_me", href: "https://x.com/miantiao_me" },
    plugin: {
      href: "https://awesome.metrix.bar/dashboards/tokens",
      author: { label: "Baran Wang (@baranwang)", href: "https://github.com/baranwang" },
    },
    bylineKey: "docs.thirdPartyApps.bylineWithPlugin",
    bodyKey: "docs.thirdPartyApps.metrix.body",
  },
];

export default async function DocsPage() {
  const { locale, t } = await getTranslator();

  return (
    <main
      className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")}
      id="main-content"
    >
      <div className="mx-auto w-full max-w-[860px]">
      <PageHeader
        title={t("docs.title")}
        description={t("docs.description")}
      />

        <div className="flex flex-col gap-12">
        <Section
          id="cli"
          title={t("docs.install.title")}
          description={t("docs.install.description")}
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
                {richText(t("docs.install.macosNote"), {
                  brewServices: <code className={CODE}>brew services</code>,
                })}
              </p>
            </TabsContent>

            <TabsContent value="linux" className="mt-4 flex flex-col gap-3">
              <CommandBlock commands={LINUX} />
              <p className="text-sm leading-relaxed text-muted-foreground">
                {richText(t("docs.install.linuxNote"), {
                  tokensServe: <code className={CODE}>tokens serve</code>,
                })}
              </p>
            </TabsContent>

            <TabsContent value="windows" className="mt-4 flex flex-col gap-3">
              <CommandBlock commands={WINDOWS} />
              <p className="text-sm leading-relaxed text-muted-foreground">
                {t("docs.install.windowsNote")}
              </p>
            </TabsContent>
          </Tabs>
        </Section>

        <Section
          id="ios"
          title={t("docs.iosApp.title")}
          description={t("docs.iosApp.description")}
        >
          <Card>
            {/* Stacked on phones. Forced side by side, the title is the only
                flexible item in the row, so at 390px it was squeezed to 53px
                and wrapped onto two lines while the badge and button kept
                their width. */}
            <CardHeader className="flex flex-col items-start gap-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
              <div className="flex flex-wrap items-center gap-2">
                <CardTitle className="text-base">{t("docs.iosApp.cardTitle")}</CardTitle>
                <Badge variant="secondary">App Store</Badge>
              </div>
              {/* Base UI composes via `render`, not Radix's `asChild`.
                  Styled the way Apple's own install buttons are — black with
                  the mark — so it reads as "this goes to Apple". It inverts in
                  dark mode because a black button on a black card disappears. */}
              <Button
                className="w-full shrink-0 border border-transparent bg-black text-white hover:bg-black/85 sm:w-auto dark:bg-white dark:text-black dark:hover:bg-white/90"
                render={
                  <a href={APP_STORE_URL} target="_blank" rel="noopener noreferrer" />
                }
              >
                <AppleMark data-icon="inline-start" />
                {t("docs.iosApp.download")}
              </Button>
            </CardHeader>
            <CardContent className="flex flex-col gap-5">
              <p className="text-sm leading-relaxed text-muted-foreground">
                {t("docs.iosApp.liquidGlass")}
              </p>

              <div className="flex flex-col gap-4">
                <Feature icon={Share2Icon} title={t("docs.iosApp.features.shareCards.title")}>
                  {t("docs.iosApp.features.shareCards.body")}
                </Feature>
                <Feature icon={LayoutGridIcon} title={t("docs.iosApp.features.homeWidgets.title")}>
                  {t("docs.iosApp.features.homeWidgets.body")}
                </Feature>
                <Feature icon={LockIcon} title={t("docs.iosApp.features.lockWidgets.title")}>
                  {t("docs.iosApp.features.lockWidgets.body")}
                </Feature>
              </div>

              <Separator />

              <ol className="flex list-decimal flex-col gap-2 pl-5 text-sm leading-relaxed text-muted-foreground">
                <li>{t("docs.iosApp.steps.install")}</li>
                <li>{t("docs.iosApp.steps.enterUsername")}</li>
                <li>{t("docs.iosApp.steps.addWidgets")}</li>
              </ol>
            </CardContent>
          </Card>
        </Section>

        {/* Built and run by other people on Tokens data. Listed, not
            operated: each has its own support and privacy policy. */}
        <Section
          id="elsewhere"
          title={t("docs.thirdPartyApps.title")}
          description={t("docs.thirdPartyApps.description")}
        >
          <div className="flex flex-col gap-4">
            {THIRD_PARTY_APPS.map((app) => (
              <Card key={app.id}>
                <CardHeader className="flex flex-col items-start gap-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
                  <div className="flex flex-col gap-1">
                    <CardTitle className="text-base">{app.name}</CardTitle>
                    <p className="text-xs text-muted-foreground">
                      {richText(t(app.bylineKey), {
                        author: <ExternalLink href={app.author.href}>{app.author.label}</ExternalLink>,
                        ...(app.plugin
                          ? {
                              plugin: (
                                <ExternalLink href={app.plugin.href}>
                                  {t("docs.thirdPartyApps.tokensPlugin")}
                                </ExternalLink>
                              ),
                              pluginAuthor: (
                                <ExternalLink href={app.plugin.author.href}>
                                  {app.plugin.author.label}
                                </ExternalLink>
                              ),
                            }
                          : {}),
                      })}
                    </p>
                  </div>
                  <Button
                    variant="outline"
                    className="w-full shrink-0 sm:w-auto"
                    render={<a href={app.url} target="_blank" rel="noopener noreferrer" />}
                  >
                    {app.urlLabel}
                  </Button>
                </CardHeader>
                <CardContent className="text-sm leading-relaxed text-muted-foreground">
                  {t(app.bodyKey)}
                </CardContent>
              </Card>
            ))}
            <p className="text-sm text-muted-foreground">
              {richText(t("docs.thirdPartyApps.listYours"), {
                repo: (
                  <ExternalLink href="https://github.com/missuo/tokens/issues/new">
                    github.com/missuo/tokens
                  </ExternalLink>
                ),
              })}
            </p>
          </div>
        </Section>

        <Section
          id="usage"
          title={t("docs.everyday.title")}
          description={t("docs.everyday.description")}
        >
          <CommandBlock
            commands={[
              { command: "tokens login", noteKey: "docs.commands.notes.authenticate" },
              { command: "tokens submit", noteKey: "docs.commands.notes.submitNow" },
              { command: "tokens serve", noteKey: "docs.commands.notes.submitInBackground" },
              { command: "tokens status", noteKey: "docs.commands.notes.submittedSoFar" },
              { command: "tokens help", noteKey: "docs.commands.notes.everythingElse" },
            ]}
          />
        </Section>

        <Section
          id="private"
          title={t("docs.privateMode.title")}
          description={t("docs.privateMode.description")}
        >
          <div className="flex flex-col gap-4">
            <CommandBlock
              commands={[
                {
                  command: "tokens login --private",
                  noteKey: "docs.commands.notes.goPrivate",
                },
              ]}
            />

            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.privateMode.toggle.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {richText(t("docs.privateMode.toggle.body"), {
                  flag: <code className={CODE}>--private</code>,
                  settings: (
                    <a href={localizeHref("/settings", locale)} className={LINK}>
                      {t("nav.settings")}
                    </a>
                  ),
                })}
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.privateMode.readTokens.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {richText(t("docs.privateMode.readTokens.body"), {
                  token: <code className={CODE}>tkr_…</code>,
                  header: <code className={CODE}>Authorization: Bearer tkr_…</code>,
                  query: <code className={CODE}>?token=tkr_…</code>,
                  loginCommand: <code className={CODE}>tokens login --private</code>,
                })}
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.privateMode.deviceNames.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {richText(t("docs.privateMode.deviceNames.body"), {
                  example: <code className={CODE}>jo****ro</code>,
                })}
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.privateMode.mcpNames.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {richText(t("docs.privateMode.mcpNames.body"), {
                  enable: <code className={CODE}>tokens mcp-names enable</code>,
                  flag: <code className={CODE}>tokens submit --include-mcp</code>,
                  disable: <code className={CODE}>tokens mcp-names disable</code>,
                  status: <code className={CODE}>tokens mcp-names status</code>,
                })}
              </CardContent>
            </Card>
          </div>
        </Section>

        <Section
          id="verified"
          title={t("docs.verifiedBadge.title")}
          description={t("docs.verifiedBadge.description")}
        >
          <div className="flex flex-col gap-4">
            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.verifiedBadge.howToGet.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {richText(t("docs.verifiedBadge.howToGet.body"), {
                  links: (
                    <strong className="font-medium text-foreground">
                      {t("docs.verifiedBadge.howToGet.links")}
                    </strong>
                  ),
                  settings: (
                    <a
                      href="https://github.com/settings/profile"
                      target="_blank"
                      rel="noopener noreferrer"
                      className={LINK}
                    >
                      {t("docs.verifiedBadge.howToGet.settings")}
                    </a>
                  ),
                })}
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.verifiedBadge.timing.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {t("docs.verifiedBadge.timing.body")}
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.verifiedBadge.why.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {richText(t("docs.verifiedBadge.why.body"), {
                  shame: (
                    <a href={localizeHref("/shame", locale)} className={LINK}>
                      {t("nav.hallOfShame")}
                    </a>
                  ),
                })}
              </CardContent>
            </Card>
          </div>
        </Section>

        <Section
          id="clients"
          title={t("docs.clients.title")}
          description={t("docs.clients.description")}
        >
          <div className="flex flex-col gap-4">
            <p className="text-sm leading-relaxed text-muted-foreground">
              {t("docs.clients.count", { count: CLIENT_GRID.length })}
            </p>

            <p className="text-sm leading-relaxed text-muted-foreground">
              {t("docs.clients.paseo")}
            </p>

            {/* A plain responsive grid rather than a table: these are names,
                not tabular data, and at 39 entries a table would force either
                a very long single column or horizontal scrolling on a phone. */}
            <ul className="grid grid-cols-2 gap-x-4 gap-y-2.5 sm:grid-cols-3 lg:grid-cols-4">
              {CLIENT_GRID.map(({ id, name, logo }) => (
                <li key={id} className="flex min-w-0 items-center gap-2.5">
                  {/* Plain <img>: these are small third-party marks in a mix of
                      png/jpg/webp/svg, already served from our own origin, so
                      the optimizer would add requests without adding much. */}
                  <img
                    src={logo}
                    alt=""
                    width={20}
                    height={20}
                    loading="lazy"
                    className="size-5 shrink-0 rounded-[4px] object-contain"
                  />
                  <span className="truncate text-sm text-muted-foreground">
                    {name}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        </Section>

        <Section
          id="architecture"
          title={t("docs.architecture.title")}
          description={t("docs.architecture.description")}
        >
          <div className="flex flex-col gap-4">
            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.architecture.site.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {t("docs.architecture.site.body")}
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.architecture.database.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {richText(t("docs.architecture.database.body"), {
                  neon: (
                    <a
                      href="https://neon.com"
                      target="_blank"
                      rel="noopener noreferrer"
                      className={BRAND_LINK}
                    >
                      Neon
                    </a>
                  ),
                })}
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.architecture.cli.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {richText(t("docs.architecture.cli.body"), {
                  dryRun: <code className={CODE}>tokens submit --dry-run --json</code>,
                })}
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-sm">{t("docs.architecture.caching.title")}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {t("docs.architecture.caching.body")}
              </CardContent>
            </Card>
          </div>
        </Section>

        {/* Its own section rather than a paragraph tacked onto whichever
            architecture card happened to mention the vendor. Sponsorship is not
            an implementation detail, and burying it under "Architecture" made
            it read as one. */}
        <Section
          id="sponsors"
          title={t("docs.sponsors.title")}
          description={t("docs.sponsors.description")}
        >
          <div className="flex flex-col gap-4">
            <Card size="sm">
              <CardHeader>
                <CardTitle className="flex items-center gap-2 text-sm">
                  {/* Both marks are checked in rather than hotlinked, so a
                      brand site reorganising cannot break this page. V.PS
                      publishes its mark white-on-dark with the corner radius
                      baked in, so it carries its own background. */}
                  {/* eslint-disable-next-line @next/next/no-img-element */}
                  <img
                    src="/icons/vps.svg"
                    alt=""
                    width={18}
                    height={18}
                    className="size-[18px] shrink-0"
                  />
                  {richText(t("docs.sponsors.vps.title"), {
                    vps: (
                      <a
                        href="https://v.ps"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="underline underline-offset-4"
                      >
                        V.PS
                      </a>
                    ),
                  })}
                </CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {richText(t("docs.sponsors.vps.p1"), {
                  xtom: (
                    <a
                      href="https://xtom.com"
                      target="_blank"
                      rel="noopener noreferrer"
                      className={BRAND_LINK}
                    >
                      xTom
                    </a>
                  ),
                })}
                <br />
                <br />
                {t("docs.sponsors.vps.p2")}
                <br />
                <br />
                {richText(t("docs.sponsors.vps.p3"), {
                  vpsHosting: (
                    <a
                      href="https://vps.hosting/?affid=2147"
                      target="_blank"
                      rel="noopener noreferrer"
                      className={BRAND_LINK}
                    >
                      vps.hosting
                    </a>
                  ),
                })}
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader>
                <CardTitle className="flex items-center gap-2 text-sm">
                  {/* eslint-disable-next-line @next/next/no-img-element */}
                  <img
                    src="/icons/neon.svg"
                    alt=""
                    width={18}
                    height={18}
                    className="size-[18px] shrink-0"
                  />
                  {richText(t("docs.sponsors.neon.title"), {
                    neon: (
                      <a
                        href="https://neon.com"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="underline underline-offset-4"
                      >
                        Neon
                      </a>
                    ),
                  })}
                </CardTitle>
              </CardHeader>
              <CardContent className="text-sm leading-relaxed text-muted-foreground">
                {t("docs.sponsors.neon.p1")}
                <br />
                <br />
                {t("docs.sponsors.neon.p2")}
              </CardContent>
            </Card>
          </div>
        </Section>

        </div>
      </div>
    </main>
  );
}
