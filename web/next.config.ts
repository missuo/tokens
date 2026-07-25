import type { NextConfig } from "next";
import { initOpenNextCloudflareForDev } from "@opennextjs/cloudflare";

const nextConfig: NextConfig = {
  compiler: {
    styledComponents: true,
  },

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
initOpenNextCloudflareForDev();
