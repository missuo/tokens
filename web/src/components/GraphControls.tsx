"use client";

import type { ViewMode, ColorPaletteName, ClientType, GraphColorPalette } from "@/lib/types";
import { getPaletteNames, colorPalettes } from "@/lib/themes";
import { SOURCE_DISPLAY_NAMES, SOURCE_LOGOS } from "@/lib/constants";
import { formatTokenCount, toggleClientFilter } from "@/lib/utils";

interface GraphControlsProps {
  view: ViewMode;
  onViewChange: (view: ViewMode) => void;
  paletteName: ColorPaletteName;
  onPaletteChange: (palette: ColorPaletteName) => void;
  selectedYear: string;
  availableYears: string[];
  onYearChange: (year: string) => void;
  clientFilter: ClientType[];
  availableClients: ClientType[];
  onClientFilterChange: (clients: ClientType[]) => void;
  palette: GraphColorPalette;
  totalTokens: number;
}

function relativeLuminance(hex: string): number {
  const match = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!match) return 0;

  const channels = [0, 2, 4].map((offset) => {
    const value = Number.parseInt(match[1].slice(offset, offset + 2), 16) / 255;
    return value <= 0.04045
      ? value / 12.92
      : ((value + 0.055) / 1.055) ** 2.4;
  });
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
}

function paletteForeground(background: string): string {
  const backgroundLuminance = relativeLuminance(background);
  const lightContrast = 1.05 / (backgroundLuminance + 0.05);
  const darkLuminance = relativeLuminance("#0b0d14");
  const darkContrast =
    (Math.max(backgroundLuminance, darkLuminance) + 0.05) /
    (Math.min(backgroundLuminance, darkLuminance) + 0.05);
  return lightContrast >= darkContrast ? "#ffffff" : "#0b0d14";
}

export function GraphControls({
  view,
  onViewChange,
  paletteName,
  onPaletteChange,
  selectedYear,
  availableYears,
  onYearChange,
  clientFilter,
  availableClients,
  onClientFilterChange,
  palette,
  totalTokens,
}: GraphControlsProps) {
  const paletteNames = getPaletteNames();
  const activeViewForeground = paletteForeground(palette.grade3);

  const handleClientToggle = (client: ClientType) => {
    onClientFilterChange(toggleClientFilter(client, clientFilter, availableClients));
  };

  return (
    <div className="relative mb-4">
      <div role="group" aria-label="View mode" className="float-right mt-1 ml-4 flex">
        <button
          onClick={() => onViewChange("2d")}
          aria-pressed={view === "2d"}
          aria-label="2D view"
          className="rounded-l-full border px-3 py-1.5 text-xs font-semibold transition"
          style={{
            backgroundColor: view === "2d" ? palette.grade3 : "var(--accent)",
            color: view === "2d" ? activeViewForeground : "var(--foreground)",
            borderColor: view === "2d" ? palette.grade3 : "var(--border)",
          }}
        >
          2D
        </button>
        <button
          onClick={() => onViewChange("3d")}
          aria-pressed={view === "3d"}
          aria-label="3D view"
          className="rounded-r-full border border-l-0 px-3 py-1.5 text-xs font-semibold transition"
          style={{
            backgroundColor: view === "3d" ? palette.grade3 : "var(--accent)",
            color: view === "3d" ? activeViewForeground : "var(--foreground)",
            borderColor: view === "3d" ? palette.grade3 : "var(--border)",
          }}
        >
          3D
        </button>
      </div>

      <div className="float-right mt-1 ml-3">
        <select
          value={paletteName}
          onChange={(e) => onPaletteChange(e.target.value as ColorPaletteName)}
          aria-label="Color palette"
          className="cursor-pointer rounded-lg border border-border bg-accent px-2 py-1.5 text-xs font-medium text-foreground"
        >
          {paletteNames.map((name) => (
            <option key={name} value={name}>
              {colorPalettes[name].name}
            </option>
          ))}
        </select>
      </div>

      <h2 className="mb-3 text-lg font-medium text-foreground">
        <span className="font-mono font-bold text-primary tabular-nums">
          {formatTokenCount(totalTokens)}
        </span>{" "}
        tokens used
        {selectedYear && (
          <>
            {" "}
            in{" "}
            {availableYears.length > 1 ? (
              <select
                value={selectedYear}
                onChange={(e) => onYearChange(e.target.value)}
                aria-label="Select year"
                className="cursor-pointer border-none bg-transparent font-bold text-foreground underline decoration-dotted decoration-2 underline-offset-4"
              >
                {availableYears.map((year) => (
                  <option key={year} value={year} className="bg-card">
                    {year}
                  </option>
                ))}
              </select>
            ) : (
              <span className="font-bold">{selectedYear}</span>
            )}
          </>
        )}
      </h2>

      <div className="clear-both" />

      <div className="mt-3 flex flex-wrap items-center justify-between gap-3 max-[560px]:flex-col max-[560px]:items-stretch max-[560px]:justify-start max-[560px]:gap-2">
        {availableClients.length > 1 && (
          <div role="group" aria-label="Client filters" className="flex max-w-full flex-wrap items-center gap-2 overflow-x-auto pb-1 max-[560px]:flex-nowrap [&::-webkit-scrollbar]:hidden">
            <span className="shrink-0 text-xs font-semibold whitespace-nowrap text-muted-foreground">Filter:</span>
            {availableClients.map((client) => {
              const isSelected = clientFilter.length === 0 || clientFilter.includes(client);
              return (
                <button
                  key={client}
                  onClick={() => handleClientToggle(client)}
                  aria-pressed={isSelected}
                  aria-label={`Filter by ${SOURCE_DISPLAY_NAMES[client] || client}`}
                  className={`flex flex-none items-center gap-1.5 rounded-full border-[1.5px] py-1 pr-3 pl-1.5 text-xs transition hover:scale-105 ${isSelected ? "font-semibold opacity-100" : "font-normal opacity-50"}`}
                  style={{
                    backgroundColor: isSelected ? `${palette.grade3}30` : "transparent",
                    color: "var(--foreground)",
                    borderColor: isSelected ? palette.grade3 : "var(--border)",
                  }}
                >
                  {SOURCE_LOGOS[client] && (
                    // eslint-disable-next-line @next/next/no-img-element
                    <img src={SOURCE_LOGOS[client]} alt={`${SOURCE_DISPLAY_NAMES[client] || client} logo`} className="h-5 w-5 shrink-0 rounded-full object-cover" />
                  )}
                  {SOURCE_DISPLAY_NAMES[client] || client}
                </button>
              );
            })}
            {clientFilter.length > 0 && clientFilter.length < availableClients.length && (
              <button onClick={() => onClientFilterChange([...availableClients])} aria-label="Show all clients" className="rounded-full px-3 py-1 text-xs font-medium text-muted-foreground transition hover:bg-foreground/10">
                Show all
              </button>
            )}
            {clientFilter.length === availableClients.length && (
              <button onClick={() => onClientFilterChange([])} aria-label="Clear all client filters" className="rounded-full px-3 py-1 text-xs font-medium text-muted-foreground transition hover:bg-foreground/10">
                Clear
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
