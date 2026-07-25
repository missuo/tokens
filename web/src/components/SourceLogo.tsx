"use client";

import { SOURCE_LOGOS } from "@/lib/constants";
import type { ClientType } from "@/lib/types";
import { cn } from "@/lib/utils";

interface SourceLogoProps {
  sourceId: string;
  height?: number;
  className?: string;
  /**
   * When the logo sits next to a visible text label of the same source, mark it
   * decorative so assistive tech doesn't announce the source name twice.
   */
  decorative?: boolean;
}

export function SourceLogo({ sourceId, height = 14, className = "", decorative = false }: SourceLogoProps) {
  const normalizedId = sourceId.toLowerCase() as ClientType;
  const src = Object.prototype.hasOwnProperty.call(SOURCE_LOGOS, normalizedId)
    ? SOURCE_LOGOS[normalizedId]
    : null;

  if (!src) {
    return (
      <span className={className} aria-hidden={decorative || undefined}>
        {sourceId}
      </span>
    );
  }

  return (
    // Pinned to a square at every axis, not just width and height: these are
    // third-party marks at whatever aspect ratio their owner shipped, and a
    // stray wide one would push the row it sits in.
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={src}
      alt={decorative ? "" : sourceId}
      style={{
        height,
        minHeight: height,
        maxHeight: height,
        minWidth: height,
        maxWidth: height,
      }}
      className={cn("w-auto rounded-sm object-contain", className)}
    />
  );
}
