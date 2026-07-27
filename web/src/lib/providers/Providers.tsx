"use client";

import React from "react";
import { ThemeProvider } from "next-themes";

/**
 * App-wide providers.
 *
 * Only next-themes, for the light/dark toggle (class strategy → `.dark` on
 * <html>, which the palette in globals.css reads).
 *
 * There used to be a react-aria `RouterProvider` here so HeroUI components
 * carrying `href` would navigate client-side. Every UI primitive now comes from
 * @base-ui/react, which does not read it, and links are `next/link` — so it
 * went with the HeroUI dependency.
 */
export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      disableTransitionOnChange
    >
      {children}
    </ThemeProvider>
  );
}
