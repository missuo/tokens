import type { NextConfig } from "next";

/**
 * Two build targets from one config.
 *
 * `BUILD_TARGET=node` produces the self-hosted image: a `standalone` server
 * that runs under plain Node beside its own Postgres. Anything else keeps the
 * Cloudflare build exactly as it was, because that deployment is not being
 * retired — it stays deployed against Neon as the failover path, and a config
 * that quietly changed shape underneath it would make the fallback the one
 * thing nobody had tested.
 */
const isNodeTarget = process.env.BUILD_TARGET === "node";

const nextConfig: NextConfig = {
  // Traces the module graph and emits a self-contained server, so the runtime
  // image carries neither node_modules nor sources. Only set for the Node
  // target: OpenNext consumes the ordinary `.next` output and has no use for
  // a second copy of the server.
  ...(isNodeTarget ? { output: "standalone" as const } : {}),

  images: {
    // Workers has no Vercel image optimizer. Every `next/image` source in this
    // app is a local, pre-optimized static asset (svg/webp/png) served straight
    // from Cloudflare's edge, and GitHub avatars go through plain <img>, so
    // opting out costs nothing and avoids a Cloudflare Images bill.
    unoptimized: true,
    remotePatterns: [
      {
        protocol: "https",
        hostname: "avatars.githubusercontent.com",
        pathname: "/**",
      },
      {
        protocol: "https",
        hostname: "github.com",
        pathname: "/**",
      },
    ],
  },

  // Security headers for production
  headers: async () => [
    {
      source: "/:path*",
      headers: [
        {
          key: "X-DNS-Prefetch-Control",
          value: "on",
        },
        {
          key: "X-Frame-Options",
          value: "SAMEORIGIN",
        },
        {
          key: "X-Content-Type-Options",
          value: "nosniff",
        },
        {
          key: "Referrer-Policy",
          value: "strict-origin-when-cross-origin",
        },
      ],
    },
  ],

  // Experimental features
  experimental: {
    // Enable server actions
    serverActions: {
      bodySizeLimit: "2mb",
    },
  },
};

export default nextConfig;

// Makes the Cloudflare bindings (Hyperdrive, R2, Durable Objects) available to
// `next dev`, so local development exercises the same code paths as production.
//
// Imported lazily rather than at the top of the file. The Node image builds
// without the Cloudflare adapter installed at all, and a static import would
// make `next.config.ts` fail to load there — before any of the config above is
// ever read.
if (!isNodeTarget) {
  void import("@opennextjs/cloudflare")
    .then(({ initOpenNextCloudflareForDev }) => initOpenNextCloudflareForDev())
    .catch(() => {
      // Not installed, or not a Cloudflare build. Neither is an error here.
    });
}
