"use client";

import React from "react";

export interface TabItem<T extends string> {
  id: T;
  label: string;
}

export interface TabBarProps<T extends string> {
  tabs: TabItem<T>[];
  activeTab: T;
  onTabChange: (tab: T) => void;
  /** `md` = page-level segmented control, `sm` = inline (e.g. a sort toggle). */
  size?: "sm" | "md";
  /** Stretch to fill the container and scroll horizontally when it overflows. */
  fluid?: boolean;
  "aria-label"?: string;
}

export function TabBar<T extends string>({
  tabs,
  activeTab,
  onTabChange,
  size = "md",
  fluid = false,
  "aria-label": ariaLabel = "Content tabs",
}: TabBarProps<T>) {
  const handleKeyDown = (e: React.KeyboardEvent, currentIndex: number) => {
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      onTabChange(tabs[(currentIndex + 1) % tabs.length].id);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      onTabChange(tabs[(currentIndex - 1 + tabs.length) % tabs.length].id);
    } else if (e.key === "Home") {
      e.preventDefault();
      onTabChange(tabs[0].id);
    } else if (e.key === "End") {
      e.preventDefault();
      onTabChange(tabs[tabs.length - 1].id);
    }
  };

  const pad = size === "sm" ? "px-3 py-1.5 text-xs" : "px-3.5 py-1.5 text-sm";

  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      className={`no-scrollbar flex items-center gap-1 overflow-x-auto rounded-lg border border-line bg-surface-secondary p-1 ${
        fluid ? "w-full" : "w-full sm:w-fit"
      }`}
    >
      {tabs.map((tab, index) => {
        const isActive = activeTab === tab.id;
        return (
          <button
            key={tab.id}
            role="tab"
            aria-selected={isActive}
            tabIndex={isActive ? 0 : -1}
            onClick={() => onTabChange(tab.id)}
            onKeyDown={(e) => handleKeyDown(e, index)}
            className={`flex flex-1 items-center justify-center rounded-md font-medium whitespace-nowrap transition-colors outline-none focus-visible:ring-2 focus-visible:ring-accent sm:flex-none ${pad} ${
              isActive
                ? "bg-surface text-foreground shadow-sm ring-1 ring-line"
                : "bg-transparent text-muted hover:text-foreground"
            }`}
          >
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}
