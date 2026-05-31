"use client";

import React from "react";
import { ThemeProvider } from "next-themes";
import { RouterProvider } from "@heroui/react";
import { useRouter } from "nextjs-toploader/app";

/**
 * App-wide providers.
 *
 * HeroUI v3 needs no styling provider — styles come from `@heroui/styles`
 * imported in globals.css. We add:
 *  - next-themes for the light/dark toggle (class strategy → `.dark` on <html>,
 *    which HeroUI reads via its `.dark` / [data-theme="dark"] selectors).
 *  - react-aria's RouterProvider so HeroUI components with `href` use Next.js
 *    client-side navigation (and trigger the top-loader).
 */
export function Providers({ children }: { children: React.ReactNode }) {
  const router = useRouter();

  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      disableTransitionOnChange
    >
      <RouterProvider navigate={(href) => router.push(href)}>
        {children}
      </RouterProvider>
    </ThemeProvider>
  );
}
