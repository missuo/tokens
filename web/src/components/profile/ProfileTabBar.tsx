"use client";

import { useRef } from "react";
import type { KeyboardEvent } from "react";
import { cn } from "@/lib/utils";

export type ProfileTab = "activity" | "models";

export interface ProfileTabBarProps {
  activeTab: ProfileTab;
  onTabChange: (tab: ProfileTab) => void;
  className?: string;
}

const tabs: ReadonlyArray<{ id: ProfileTab; label: string }> = [
  { id: "activity", label: "Usage" },
  { id: "models", label: "Models" },
];

export function ProfileTabBar({
  activeTab,
  onTabChange,
  className,
}: ProfileTabBarProps) {
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const selectAndFocus = (index: number) => {
    onTabChange(tabs[index].id);
    tabRefs.current[index]?.focus();
  };

  const handleKeyDown = (
    event: KeyboardEvent<HTMLButtonElement>,
    currentIndex: number,
  ) => {
    let nextIndex: number | null = null;

    switch (event.key) {
      case "ArrowRight":
      case "ArrowDown":
        nextIndex = (currentIndex + 1) % tabs.length;
        break;
      case "ArrowLeft":
      case "ArrowUp":
        nextIndex = (currentIndex - 1 + tabs.length) % tabs.length;
        break;
      case "Home":
        nextIndex = 0;
        break;
      case "End":
        nextIndex = tabs.length - 1;
        break;
      default:
        return;
    }

    event.preventDefault();
    selectAndFocus(nextIndex);
  };

  return (
    <div
      className={cn(
        "grid w-full grid-cols-2 gap-1 rounded-lg border bg-card p-1",
        className
      )}
      role="tablist"
      aria-label="Profile sections"
      aria-orientation="horizontal"
    >
      {tabs.map((tab, index) => {
        const isActive = tab.id === activeTab;

        return (
          <button
            key={tab.id}
            ref={(node) => {
              tabRefs.current[index] = node;
            }}
            id={`tab-${tab.id}`}
            type="button"
            role="tab"
            aria-selected={isActive}
            aria-controls={isActive ? `tabpanel-${tab.id}` : undefined}
            tabIndex={isActive ? 0 : -1}
            onClick={() => onTabChange(tab.id)}
            onKeyDown={(event) => handleKeyDown(event, index)}
            className={cn(
              // 44px on touch, 32px where there is a pointer — the same rule
              // the stylesheet had, expressed as a variant.
              "inline-flex min-h-8 min-w-0 items-center justify-center overflow-hidden text-ellipsis whitespace-nowrap rounded-lg border px-2 py-1.5 text-[0.8125rem] leading-none transition-colors duration-150 pointer-coarse:min-h-11 motion-reduce:transition-none",
              "focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring",
              isActive
                ? // Was --service-border-strong, a blue #35405A in dark. The
                  // border only has to read as stronger than the card edge, so
                  // it is derived from the palette instead.
                  "border-muted-foreground/30 bg-primary/10 font-semibold text-foreground"
                : "border-transparent font-medium text-muted-foreground hover:bg-muted hover:text-foreground"
            )}
          >
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}
