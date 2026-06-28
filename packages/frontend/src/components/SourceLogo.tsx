"use client";

import { SOURCE_LOGOS } from "@/lib/constants";
import type { ClientType } from "@/lib/types";

interface SourceLogoProps {
  sourceId: string;
  height?: number;
  className?: string;
}

export function SourceLogo({ sourceId, height = 14, className = "" }: SourceLogoProps) {
  const normalizedId = sourceId.toLowerCase() as ClientType;
  const src = Object.prototype.hasOwnProperty.call(SOURCE_LOGOS, normalizedId)
    ? SOURCE_LOGOS[normalizedId]
    : null;

  if (!src) {
    return <span className={className}>{sourceId}</span>;
  }

  return (
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={src}
      alt={sourceId}
      className={`rounded-sm object-contain ${className}`}
      style={{ height, width: "auto", minWidth: height, maxWidth: height, minHeight: height, maxHeight: height }}
    />
  );
}