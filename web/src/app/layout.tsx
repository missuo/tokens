import type { Metadata, Viewport } from "next";
import { Geist, JetBrains_Mono } from "next/font/google";
import NextTopLoader from "nextjs-toploader";
import { Providers } from "@/lib/providers";
import { Navigation } from "@/components/layout/Navigation";
import { ServiceFooter } from "@/components/layout/ServiceFooter";
import { ThemedToastContainer } from "@/components/layout/ThemedToastContainer";
import "./globals.css";
import "react-toastify/dist/ReactToastify.css";
import { cn } from "@/lib/utils";
import { ogImageUrl } from "@/lib/og/ogImage";
import { getTranslator } from "@/lib/i18n/server";
import { LOCALE_TAGS } from "@/lib/i18n/config";
import { resolveMessages } from "@/lib/i18n/messages";
import { LocaleProvider } from "@/components/layout/LocaleProvider";

// Geist carries interface text and JetBrains Mono carries every figure, so
// numeric columns stay aligned when scanned down the page.
const geist = Geist({
  variable: "--font-sans",
  subsets: ["latin"],
  display: "swap",
});

const jetbrainsMono = JetBrains_Mono({
  variable: "--font-mono",
  subsets: ["latin"],
  display: "swap",
});

/**
 * Metadata is generated rather than exported as a constant because the title
 * and description are now translated, and a constant cannot call `headers()`.
 *
 * `metadataBase` and the icons stay literal: a URL and a PNG do not vary by
 * language.
 *
 * `alternates` is deliberately *not* set here. Every route has its own address,
 * so a canonical declared at the layout would claim the docs page canonically
 * lives at /leaderboard. The board sets its own, which is the page most often
 * linked to from outside.
 */
export async function generateMetadata(): Promise<Metadata> {
  const { t } = await getTranslator();
  const title = t("meta.title");
  const description = t("meta.description");

  return {
    title,
    description,
    metadataBase: new URL("https://tokens.ci"),
    icons: {
      icon: [
        // SVG first so capable browsers get the theme-aware mark; the .ico stays
        // for the ones that do not support SVG favicons.
        { url: "/brand/tokens-favicon.svg", type: "image/svg+xml" },
        { url: "/favicon.ico?v=2", sizes: "48x48", type: "image/x-icon" },
        { url: "/favicon-16x16.png?v=2", sizes: "16x16", type: "image/png" },
        { url: "/favicon-32x32.png?v=2", sizes: "32x32", type: "image/png" },
      ],
      apple: "/brand/tokens-app-icon-180.png",
    },
    manifest: "/site.webmanifest",
    openGraph: {
      title,
      description,
      type: "website",
      url: "https://tokens.ci",
      siteName: "Tokens",
      // The dynamic renderer, not a static file: it draws the current mark, so
      // the share card cannot drift from the brand the way a checked-in PNG did.
      images: [
        {
          url: ogImageUrl(),
          width: 1200,
          height: 630,
          alt: t("meta.ogAlt"),
        },
      ],
    },
    twitter: {
      card: "summary_large_image",
      title,
      description,
      images: [ogImageUrl()],
    },
  };
}

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
};

export default async function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  const { locale, t } = await getTranslator();
  const { lang, dir } = LOCALE_TAGS[locale];

  return (
    <html
      lang={lang}
      dir={dir}
      suppressHydrationWarning
      className={cn(geist.variable, jetbrainsMono.variable)}
    >
      <head>
        {/*
          next-themes writes its no-flash theme script inline into the HTML.
          OpenNext bundles the server with esbuild's keepNames, which rewrites
          function declarations to call an `__name` helper — and that helper
          only exists inside the server bundle, not in the serialized script.
          Without this shim the theme script throws before hydration and takes
          the whole client bundle down with it.
        */}
        <script
          dangerouslySetInnerHTML={{
            __html:
              "if(!globalThis.__name){globalThis.__name=function(f){return f}}",
          }}
        />
      </head>
      <body className="flex min-h-dvh flex-col font-sans">
        <NextTopLoader color="#0073FF" showSpinner={false} />
        {/* Every route's <main> carries id="main-content"; this is the link the
            anchors were always for. Off-screen until focused, so it costs
            nothing visually and is the first stop for a keyboard user. */}
        <a
          href="#main-content"
          className="sr-only rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground focus:not-sr-only focus:absolute focus:left-4 focus:top-4 focus:z-[100]"
        >
          {t("common.skipToContent")}
        </a>
        {/* Outside the theme provider but inside the document: the locale is
            known before the first byte, and every client component below it
            reads one merge instead of threading a prop through each layer. */}
        <LocaleProvider locale={locale} messages={resolveMessages(locale)}>
          <Providers>
            {/* Rendered once here so it persists across route changes (no remount
                flicker) and switching Leaderboard <-> Profile is a seamless
                client-side transition. */}
            <Navigation />
            {/* Grows to fill the viewport so the footer stays at the bottom on
                short pages instead of floating up under the content. */}
            <div className="flex flex-1 flex-col">{children}</div>
            <ServiceFooter />
            <ThemedToastContainer />
          </Providers>
        </LocaleProvider>
      </body>
    </html>
  );
}
