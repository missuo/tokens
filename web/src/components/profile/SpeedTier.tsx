"use client";

import * as Tooltip from "@radix-ui/react-tooltip";
import { ZapIcon } from "lucide-react";
import type { SpeedTierRow } from "@/lib/speedUsage";
import { cn } from "@/lib/utils";

const SPEED_LABELS = {
  standard: "Standard",
  fast: "Fast",
  ultrafast: "Ultrafast",
  unknown: "Unknown",
} as const;

const SOURCE_LABELS = {
  reported: "Reported",
  selected: "Selected",
} as const;

const FAST_ZAP = "size-3 fill-current text-amber-500 dark:text-amber-400";

/** Every tier, unknown included, takes the ultrafast width so labels line up. */
function SpeedTierIcon({ speed }: { speed: SpeedTierRow["speed"] }) {
  return (
    <span className="relative inline-flex h-3 w-4 flex-none items-center justify-center" aria-hidden="true">
      {speed === "standard" && <ZapIcon className="size-3 text-muted-foreground" />}
      {speed === "fast" && <ZapIcon className={FAST_ZAP} />}
      {speed === "ultrafast" && (
        <>
          <ZapIcon className={cn(FAST_ZAP, "absolute left-0 opacity-50")} />
          <ZapIcon className={cn(FAST_ZAP, "absolute right-0")} />
        </>
      )}
    </span>
  );
}

export function SpeedTierLabel({ tier, className }: { tier: SpeedTierRow; className?: string }) {
  const label = (
    <span className={cn("inline-flex min-w-0 cursor-default items-center gap-1.5", className)}>
      <SpeedTierIcon speed={tier.speed} />
      {SPEED_LABELS[tier.speed]}
    </span>
  );
  if (tier.sources.length === 0) return label;

  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>{label}</Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content
          side="top"
          sideOffset={6}
          collisionPadding={10}
          className="z-[1000] select-none rounded-lg bg-[#17181B] px-3 py-2 text-xs font-medium text-[#e5e5e5] shadow-[0_8px_30px_rgba(0,0,0,0.4),0_0_0_1px_rgba(255,255,255,0.06)]"
        >
          {SPEED_LABELS[tier.speed]} · {tier.sources.map((source) => SOURCE_LABELS[source]).join(", ")}
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}
