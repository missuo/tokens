"use client";

import * as Tooltip from "@radix-ui/react-tooltip";
import { cn } from "@/lib/utils";

const VERIFIED_TOOLTIP =
  "Verified — earned by adding two or more social links (website included) to your GitHub profile. Status refreshes every 24 hours.";

export interface VerifiedBadgeProps {
  size?: number;
  className?: string;
}

/**
 * Blue check shown next to avatars of users with enough linked GitHub
 * social accounts. Hover reveals how to earn it. The tooltip renders in a
 * portal so it never gets clipped by overflow-hidden cards or tables.
 */
export function VerifiedBadge({ size = 14, className }: VerifiedBadgeProps) {
  const glyph = Math.max(8, Math.round(size * 0.62));
  return (
    <Tooltip.Provider delayDuration={150}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          {/* #1d9bf0 is deliberate rather than themed: a blue check is a
              borrowed convention, and it only reads as one at that blue. The
              ring punches the badge out of whatever sits behind it — an avatar,
              a table row — so it stays legible on both. */}
          <span
            className={cn(
              "inline-flex flex-none cursor-default items-center justify-center rounded-full bg-[#1d9bf0] ring-2 ring-card",
              className
            )}
            style={{ width: size, height: size }}
            role="img"
            aria-label={VERIFIED_TOOLTIP}
          >
            <svg
              aria-hidden="true"
              width={glyph}
              height={glyph}
              viewBox="0 0 20 20"
              fill="none"
            >
              <path
                d="M4.5 10.5l3.5 3.5 7.5-8"
                stroke="#fff"
                strokeWidth="2.6"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          </span>
        </Tooltip.Trigger>
        <Tooltip.Portal>
          {/* Inverted in both themes, which is why it carries its own colours
              instead of the popover tokens: a light tooltip on the light theme
              would have nothing to separate it from the page. The previous
              value was #111B2C, blue enough to stand out against the rest of
              the palette; this is the same darkness, neutral. */}
          <Tooltip.Content
            side="top"
            sideOffset={6}
            collisionPadding={10}
            className="z-[1000] max-w-[260px] select-none rounded-lg bg-[#17181B] px-3 py-2 text-center text-xs font-medium leading-relaxed text-[#e5e5e5] shadow-[0_8px_30px_rgba(0,0,0,0.4),0_0_0_1px_rgba(255,255,255,0.06)]"
          >
            {VERIFIED_TOOLTIP}
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
}
