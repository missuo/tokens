import type { Metadata, Viewport } from "next";
import { JetBrains_Mono, Figtree } from "next/font/google";
import NextTopLoader from "nextjs-toploader";
import { ToastContainer } from "react-toastify";
import { Analytics } from "@vercel/analytics/next";
import { Providers } from "@/lib/providers";
import { Navigation } from "@/components/layout/Navigation";
import "./globals.css";
import "react-toastify/dist/ReactToastify.css";

const figtree = Figtree({
  variable: "--font-figtree",
  subsets: ["latin"],
  display: "swap",
});

const jetbrainsMono = JetBrains_Mono({
  variable: "--font-jetbrains",
  subsets: ["latin"],
  display: "swap",
});

export const metadata: Metadata = {
  title: "Tokens - AI Token Usage Tracker & Leaderboard",
  description: "Track, visualize, and compete on AI coding assistant token usage across Claude Code, Cursor, OpenCode, Codex, Gemini, Kimi, and Qwen. The Kardashev Scale for AI Devs.",
  metadataBase: new URL("https://tokens.ci"),
  icons: {
    icon: [
      { url: "/favicon.ico?v=2", sizes: "48x48", type: "image/x-icon" },
      { url: "/favicon-16x16.png?v=2", sizes: "16x16", type: "image/png" },
      { url: "/favicon-32x32.png?v=2", sizes: "32x32", type: "image/png" },
    ],
    apple: "/apple-icon.png?v=2",
  },
  manifest: "/site.webmanifest",
  openGraph: {
    title: "Tokens - AI Token Usage Tracker & Leaderboard",
    description: "Track, visualize, and compete on AI coding assistant token usage across Claude Code, Cursor, OpenCode, Codex, Gemini, Kimi, and Qwen. The Kardashev Scale for AI Devs.",
    type: "website",
    url: "https://tokens.ci",
    siteName: "Tokens",
    images: [
      {
        url: "https://tokens.ci/og.png",
        width: 1200,
        height: 630,
        alt: "Tokens - AI Token Usage Tracker",
      },
    ],
  },
  twitter: {
    card: "summary_large_image",
    title: "Tokens - AI Token Usage Tracker & Leaderboard",
    description: "Track, visualize, and compete on AI coding assistant token usage across Claude Code, Cursor, OpenCode, Codex, Gemini, Kimi, and Qwen.",
    images: ["https://tokens.ci/og.png"],
  },
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html
      lang="en"
      suppressHydrationWarning
      className={`${figtree.variable} ${jetbrainsMono.variable}`}
    >
      <body className="font-sans">
        <NextTopLoader color="#0073FF" showSpinner={false} />
        <Providers>
          {/* Rendered once here so it persists across route changes (no remount
              flicker) and switching Leaderboard <-> Profile is a seamless
              client-side transition. */}
          <Navigation />
          {children}
        </Providers>
        <ToastContainer position="top-right" theme="dark" />
        <Analytics />
      </body>
    </html>
  );
}
