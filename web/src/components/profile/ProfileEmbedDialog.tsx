"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { CopyIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { toast } from "react-toastify";
import {
  EMBED_TEMPLATES,
  resolvePalette,
  type EmbedTemplate,
} from "@/lib/embed/embedShared";
import { getPaletteNames, type ColorPaletteName } from "@/lib/themes";
import {
  buildEmbedPreviewPath,
  buildProfileEmbedLinks,
  getEmbedDialogCapabilities,
  type EmbedNumberFormat,
  type EmbedRankFormat,
  type EmbedSortBy,
  type EmbedTheme,
  type EmbedView,
} from "./embedDialogOptions";
import { tw } from "@/lib/tw";
import { cn } from "@/lib/utils";

function titleCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

const EMBED_TEMPLATE_LABELS: Record<EmbedTemplate, string> = {
  classic: "Overview",
  minimal: "Token focus",
  terminal: "Readout",
  graph: "Contributions",
  orbit: "Rank focus",
  vitals: "Activity summary",
  blueprint: "Detailed stats",
  receipt: "Compact list",
  pulse: "Usage pulse",
  detailed: "Today breakdown",
};

interface ProfileEmbedDialogProps {
  open: boolean;
  username: string;
  displayName?: string | null;
  onClose: () => void;
}

export function ProfileEmbedDialog({
  open,
  username,
  displayName,
  onClose,
}: ProfileEmbedDialogProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const [theme, setTheme] = useState<EmbedTheme>("dark");
  const [sortBy, setSortBy] = useState<EmbedSortBy>("tokens");
  const [layoutCompact, setLayoutCompact] = useState(false);
  const [threeDCompact, setThreeDCompact] = useState(false);
  const [view, setView] = useState<EmbedView>("2d");
  const [template, setTemplate] = useState<EmbedTemplate>("classic");
  const [color, setColor] = useState<ColorPaletteName | null>(null);
  const [tokensFormat, setTokensFormat] =
    useState<EmbedNumberFormat>("compact");
  const [costFormat, setCostFormat] = useState<EmbedNumberFormat>("compact");
  const [rankFormat, setRankFormat] = useState<EmbedRankFormat>("plain");
  const [graph, setGraph] = useState(false);
  const [today, setToday] = useState(false);
  const compact = view === "3d" ? threeDCompact : layoutCompact;
  const setCompactForView = (nextCompact: boolean) => {
    if (view === "3d") {
      setThreeDCompact(nextCompact);
    } else {
      setLayoutCompact(nextCompact);
    }
  };

  const capabilities = getEmbedDialogCapabilities({
    compact,
    template,
    view,
  });

  useEffect(() => {
    if (!open) return;

    const previousOverflow = document.body.style.overflow;
    const previousFocus = document.activeElement as HTMLElement | null;
    const focusDialog = window.requestAnimationFrame(() => {
      dialogRef.current?.focus();
    });
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }

      if (event.key !== "Tab" || !dialogRef.current) {
        return;
      }

      const focusableElements = Array.from(
        dialogRef.current.querySelectorAll<HTMLElement>(
          'a[href], button:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((element) => element.getClientRects().length > 0);

      if (focusableElements.length === 0) {
        event.preventDefault();
        dialogRef.current.focus();
        return;
      }

      const firstElement = focusableElements[0];
      const lastElement = focusableElements[focusableElements.length - 1];
      const activeElement = document.activeElement;

      if (
        event.shiftKey &&
        (activeElement === firstElement ||
          activeElement === dialogRef.current ||
          !dialogRef.current.contains(activeElement))
      ) {
        event.preventDefault();
        lastElement.focus();
      } else if (
        !event.shiftKey &&
        (activeElement === lastElement ||
          !dialogRef.current.contains(activeElement))
      ) {
        event.preventDefault();
        firstElement.focus();
      }
    };

    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.cancelAnimationFrame(focusDialog);
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", handleKeyDown);
      previousFocus?.focus();
    };
  }, [open, onClose]);

  const { embedUrl, markdownSnippet, htmlSnippet, profileUrl } = useMemo(
    () =>
      buildProfileEmbedLinks(username, {
        color,
        compact,
        costFormat,
        graph,
        rankFormat,
        sortBy,
        template,
        theme,
        today,
        tokensFormat,
        view,
      }),
    [
      color,
      compact,
      costFormat,
      graph,
      rankFormat,
      sortBy,
      template,
      theme,
      today,
      tokensFormat,
      username,
      view,
    ],
  );
  const previewUrl = useMemo(() => buildEmbedPreviewPath(embedUrl), [embedUrl]);

  const copyToClipboard = async (value: string, label: string) => {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(`${label} copied`);
    } catch {
      toast.error(`Failed to copy ${label.toLowerCase()}`);
    }
  };

  if (!open || typeof document === "undefined") {
    return null;
  }

  return createPortal(
    <Overlay
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <Dialog
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="profile-embed-dialog-title"
        aria-describedby="profile-embed-dialog-description"
        tabIndex={-1}
      >
        <DialogHeader>
          <HeaderCopy>
            <DialogTitle id="profile-embed-dialog-title">
              Embed @{username}
            </DialogTitle>
            <DialogDescription id="profile-embed-dialog-description">
              Customize the live card, then copy it into GitHub or any HTML
              page.
            </DialogDescription>
          </HeaderCopy>

          <CloseButton
            type="button"
            onClick={onClose}
            aria-label="Close embed dialog"
          >
            <CloseIcon />
          </CloseButton>
        </DialogHeader>

        <DialogBody>
          <PreviewPanel>
            <PreviewSurface>
              <PreviewLabel>Live preview</PreviewLabel>
              <PreviewFrame $threeD={view === "3d"}>
                <PreviewImage
                  src={previewUrl}
                  alt={`Tokens README embed preview for ${displayName || username}`}
                />
              </PreviewFrame>
            </PreviewSurface>
          </PreviewPanel>

          <ControlsPanel aria-label="Embed customization">
            <ControlsHeader>
              <ControlsTitle>Card settings</ControlsTitle>
            </ControlsHeader>

            {capabilities.showTemplate && (
              <OptionGroup>
                <OptionLabel id="embed-template-label">Template</OptionLabel>
                <SelectWrap>
                  <SelectControl
                    name="profile-embed-template"
                    aria-labelledby="embed-template-label"
                    value={template}
                    onChange={(event) =>
                      setTemplate(event.currentTarget.value as EmbedTemplate)
                    }
                  >
                    {EMBED_TEMPLATES.map((tpl) => (
                      <option key={tpl} value={tpl}>
                        {EMBED_TEMPLATE_LABELS[tpl]}
                      </option>
                    ))}
                  </SelectControl>
                  <SelectIcon aria-hidden="true" />
                </SelectWrap>
              </OptionGroup>
            )}

            <OptionGroup>
              <OptionLabel id="embed-view-label">View</OptionLabel>
              <SegmentedControl role="group" aria-labelledby="embed-view-label">
                <SegmentButton
                  type="button"
                  $active={view === "2d"}
                  aria-pressed={view === "2d"}
                  onClick={() => setView("2d")}
                >
                  2D
                </SegmentButton>
                <SegmentButton
                  type="button"
                  $active={view === "3d"}
                  aria-pressed={view === "3d"}
                  onClick={() => setView("3d")}
                >
                  3D
                </SegmentButton>
              </SegmentedControl>
            </OptionGroup>

            <OptionGroup>
              <OptionLabel id="embed-theme-label">Theme</OptionLabel>
              <SegmentedControl
                role="group"
                aria-labelledby="embed-theme-label"
              >
                <SegmentButton
                  type="button"
                  $active={theme === "dark"}
                  aria-pressed={theme === "dark"}
                  onClick={() => setTheme("dark")}
                >
                  Dark
                </SegmentButton>
                <SegmentButton
                  type="button"
                  $active={theme === "light"}
                  aria-pressed={theme === "light"}
                  onClick={() => setTheme("light")}
                >
                  Light
                </SegmentButton>
              </SegmentedControl>
            </OptionGroup>

            {capabilities.showAccent && (
              <OptionGroup $stacked>
                <OptionLabel id="embed-accent-label">Accent color</OptionLabel>
                <SwatchRow role="group" aria-labelledby="embed-accent-label">
                  <Swatch
                    type="button"
                    $active={color === null}
                    $color={resolvePalette(theme, null).brand}
                    aria-pressed={color === null}
                    aria-label="Default accent color"
                    title="Default"
                    onClick={() => setColor(null)}
                  />
                  {getPaletteNames().map((name) => (
                    <Swatch
                      key={name}
                      type="button"
                      $active={color === name}
                      $color={resolvePalette(theme, name).brand}
                      aria-pressed={color === name}
                      aria-label={`${name} accent color`}
                      title={titleCase(name)}
                      onClick={() => setColor(name)}
                    />
                  ))}
                </SwatchRow>
              </OptionGroup>
            )}

            <OptionGroup>
              <OptionLabel id="embed-ranking-label">Ranking</OptionLabel>
              <SegmentedControl
                role="group"
                aria-labelledby="embed-ranking-label"
              >
                <SegmentButton
                  type="button"
                  $active={sortBy === "tokens"}
                  aria-pressed={sortBy === "tokens"}
                  onClick={() => setSortBy("tokens")}
                >
                  Tokens
                </SegmentButton>
                <SegmentButton
                  type="button"
                  $active={sortBy === "cost"}
                  aria-pressed={sortBy === "cost"}
                  onClick={() => setSortBy("cost")}
                >
                  Cost
                </SegmentButton>
              </SegmentedControl>
            </OptionGroup>

            {capabilities.showRankFormat && (
              <OptionGroup>
                <OptionLabel id="embed-rank-format-label">
                  Rank format
                </OptionLabel>
                <SegmentedControl
                  role="group"
                  aria-labelledby="embed-rank-format-label"
                >
                  {(["plain", "percent", "total"] as const).map((mode) => (
                    <SegmentButton
                      key={mode}
                      type="button"
                      $active={rankFormat === mode}
                      aria-pressed={rankFormat === mode}
                      onClick={() => setRankFormat(mode)}
                    >
                      {titleCase(mode)}
                    </SegmentButton>
                  ))}
                </SegmentedControl>
              </OptionGroup>
            )}

            {capabilities.showLayout && (
              <OptionGroup>
                <OptionLabel id="embed-layout-label">
                  {view === "3d" ? "Number format" : "Layout"}
                </OptionLabel>
                <SegmentedControl
                  role="group"
                  aria-labelledby="embed-layout-label"
                >
                  <SegmentButton
                    type="button"
                    $active={!compact}
                    aria-pressed={!compact}
                    onClick={() => setCompactForView(false)}
                  >
                    Full
                  </SegmentButton>
                  <SegmentButton
                    type="button"
                    $active={compact}
                    aria-pressed={compact}
                    onClick={() => setCompactForView(true)}
                  >
                    Compact
                  </SegmentButton>
                </SegmentedControl>
              </OptionGroup>
            )}

            {capabilities.showGraph && (
              <OptionGroup>
                <OptionLabel id="embed-graph-label">
                  Contribution graph
                </OptionLabel>
                <SegmentedControl
                  role="group"
                  aria-labelledby="embed-graph-label"
                >
                  <SegmentButton
                    type="button"
                    $active={!graph}
                    aria-pressed={!graph}
                    onClick={() => setGraph(false)}
                  >
                    Off
                  </SegmentButton>
                  <SegmentButton
                    type="button"
                    $active={graph}
                    aria-pressed={graph}
                    onClick={() => setGraph(true)}
                  >
                    On
                  </SegmentButton>
                </SegmentedControl>
              </OptionGroup>
            )}

            {capabilities.showToday && (
              <OptionGroup>
                <OptionLabel id="embed-today-label">Today&apos;s usage</OptionLabel>
                <SegmentedControl
                  role="group"
                  aria-labelledby="embed-today-label"
                >
                  <SegmentButton
                    type="button"
                    $active={!today}
                    aria-pressed={!today}
                    onClick={() => setToday(false)}
                  >
                    Off
                  </SegmentButton>
                  <SegmentButton
                    type="button"
                    $active={today}
                    aria-pressed={today}
                    onClick={() => setToday(true)}
                  >
                    On
                  </SegmentButton>
                </SegmentedControl>
              </OptionGroup>
            )}

            {capabilities.showNumberFormats && (
              <>
                <OptionGroup>
                  <OptionLabel id="embed-token-format-label">
                    Token format
                  </OptionLabel>
                  <SegmentedControl
                    role="group"
                    aria-labelledby="embed-token-format-label"
                  >
                    <SegmentButton
                      type="button"
                      $active={tokensFormat === "compact"}
                      aria-pressed={tokensFormat === "compact"}
                      onClick={() => setTokensFormat("compact")}
                    >
                      Compact
                    </SegmentButton>
                    <SegmentButton
                      type="button"
                      $active={tokensFormat === "full"}
                      aria-pressed={tokensFormat === "full"}
                      onClick={() => setTokensFormat("full")}
                    >
                      Full
                    </SegmentButton>
                  </SegmentedControl>
                </OptionGroup>

                <OptionGroup>
                  <OptionLabel id="embed-cost-format-label">
                    Cost format
                  </OptionLabel>
                  <SegmentedControl
                    role="group"
                    aria-labelledby="embed-cost-format-label"
                  >
                    <SegmentButton
                      type="button"
                      $active={costFormat === "compact"}
                      aria-pressed={costFormat === "compact"}
                      onClick={() => setCostFormat("compact")}
                    >
                      Compact
                    </SegmentButton>
                    <SegmentButton
                      type="button"
                      $active={costFormat === "full"}
                      aria-pressed={costFormat === "full"}
                      onClick={() => setCostFormat("full")}
                    >
                      Full
                    </SegmentButton>
                  </SegmentedControl>
                </OptionGroup>
              </>
            )}

            <SnippetSection>
              <SnippetHeader>
                <SnippetTitle>Markdown snippet</SnippetTitle>
                <InlineActions>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-7 px-2 text-xs"
                    onClick={() => copyToClipboard(embedUrl, "Image URL")}
                  >
                    <CopyIcon data-icon="inline-start" />
                    Image URL
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-7 px-2 text-xs"
                    onClick={() => copyToClipboard(htmlSnippet, "HTML snippet")}
                  >
                    <CopyIcon data-icon="inline-start" />
                    HTML
                  </Button>
                </InlineActions>
              </SnippetHeader>

              <CodeBlock>{markdownSnippet}</CodeBlock>

              <PrimaryActions>
                <Button
                  type="button"
                  size="sm"
                  onClick={() =>
                    copyToClipboard(markdownSnippet, "Markdown snippet")
                  }
                >
                  <CopyIcon data-icon="inline-start" />
                  Copy markdown
                </Button>
                <SecondaryLink
                  href={profileUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  View profile
                </SecondaryLink>
              </PrimaryActions>
            </SnippetSection>
          </ControlsPanel>
        </DialogBody>
      </Dialog>
    </Overlay>,
    document.body,
  );
}

// ============================================================================
// Dialog chrome
// ============================================================================
//
// The scrollbars are styled twice on purpose: `scrollbar-color`/`-width` for
// Firefox and the standard, the ::-webkit- pseudo-elements for Safari and
// Chrome. Neither covers the other.

const Overlay = tw(
  "div",
  "fixed inset-0 z-[1000] flex items-center justify-center bg-[rgba(5,7,12,0.78)] p-4 backdrop-blur-[10px] [overscroll-behavior:contain] max-[640px]:items-end max-[640px]:px-0 max-[640px]:pb-0 max-[640px]:pt-2"
);

// A centred dialog on a desktop, a sheet rising from the bottom edge on a
// phone — hence the squared bottom corners and dropped bottom border there.
const Dialog = tw(
  "div",
  "grid h-[min(84dvh,760px)] max-h-[calc(100dvh-32px)] w-[min(100%,960px)] grid-rows-[auto_minmax(0,1fr)] overflow-hidden rounded-[14px] border border-muted-foreground/30 bg-card outline-none max-[840px]:h-[min(92dvh,860px)] max-[640px]:h-[min(96dvh,860px)] max-[640px]:max-h-[calc(100dvh-8px)] max-[640px]:w-full max-[640px]:rounded-b-none max-[640px]:border-b-0"
);

const DialogHeader = tw(
  "div",
  "flex items-center justify-between gap-3 border-b bg-card px-4 py-[13px] max-[640px]:px-3.5 max-[640px]:py-3"
);

const HeaderCopy = tw("div", "flex min-w-0 flex-col gap-0.5");

const DialogTitle = tw(
  "h2",
  "m-0 overflow-hidden text-ellipsis whitespace-nowrap text-lg font-semibold leading-snug text-foreground"
);

const DialogDescription = tw(
  "p",
  "m-0 overflow-hidden text-ellipsis whitespace-nowrap text-[0.8125rem] leading-snug text-muted-foreground max-[480px]:hidden"
);

const CloseButton = tw(
  "button",
  "inline-flex size-[34px] flex-none items-center justify-center rounded-lg border border-muted-foreground/30 bg-muted text-muted-foreground transition-colors duration-150 hover:bg-primary/10 hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring motion-reduce:transition-none pointer-coarse:size-11"
);

// Thin scrollbar, thumb inset from the track by a 2px border in the track
// colour so it reads as floating rather than filling the gutter.
const SCROLLBAR =
  "[scrollbar-color:color-mix(in_srgb,var(--muted-foreground)_52%,transparent)_var(--background)] [scrollbar-width:thin] " +
  "[&::-webkit-scrollbar]:w-[9px] [&::-webkit-scrollbar-track]:bg-background " +
  "[&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:border-2 [&::-webkit-scrollbar-thumb]:border-background [&::-webkit-scrollbar-thumb]:bg-[color-mix(in_srgb,var(--muted-foreground)_52%,transparent)]";

// Two panes side by side; below 840px they stack and the body itself scrolls,
// with padding that clears the home indicator.
const DialogBody = tw(
  "div",
  cn(
    "grid min-h-0 grid-cols-[minmax(0,1fr)_minmax(320px,360px)] overflow-hidden",
    SCROLLBAR,
    "max-[840px]:grid-cols-1 max-[840px]:content-start max-[840px]:overflow-y-auto max-[840px]:[overscroll-behavior:contain] max-[840px]:[scrollbar-gutter:stable] max-[840px]:pb-[max(24px,env(safe-area-inset-bottom))] max-[840px]:[scroll-padding-bottom:max(24px,env(safe-area-inset-bottom))]",
    "max-[640px]:pb-[max(32px,calc(env(safe-area-inset-bottom)+16px))] max-[640px]:[scroll-padding-bottom:max(32px,calc(env(safe-area-inset-bottom)+16px))]"
  )
);

const PreviewPanel = tw(
  "div",
  "flex min-h-0 min-w-0 flex-col overflow-hidden border-r bg-background p-4 max-[840px]:overflow-visible max-[840px]:border-b max-[840px]:border-r-0 max-[640px]:p-3"
);

const PreviewSurface = tw("div", "flex min-h-0 flex-[1_1_auto] flex-col gap-2");

const PreviewLabel = tw(
  "span",
  "text-[0.8125rem] font-semibold leading-snug text-foreground"
);

// The 3D card is taller, so the stacked layout reserves more room for it.
const PreviewFrame = ({
  $threeD,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"div"> & { $threeD: boolean }) => (
  <div
    {...props}
    className={cn(
      "flex min-h-0 flex-[1_1_auto] items-center justify-center overflow-hidden py-5",
      "max-[840px]:flex-none max-[640px]:py-2",
      $threeD
        ? "max-[840px]:h-80 max-[640px]:h-[272px]"
        : "max-[840px]:h-50 max-[640px]:h-40",
      className
    )}
  />
);

const PreviewImage = tw(
  "img",
  "h-auto max-h-full w-auto max-w-full object-contain"
);

const ControlsPanel = tw(
  "div",
  cn(
    "flex min-h-0 min-w-0 flex-col overflow-y-auto bg-card px-4 pb-4 pt-3.5 [overscroll-behavior:contain] [scrollbar-gutter:stable]",
    SCROLLBAR,
    "[&::-webkit-scrollbar-track]:border-l [&::-webkit-scrollbar-track]:border-border",
    "max-[840px]:overflow-visible max-[840px]:[scrollbar-gutter:auto]",
    "max-[640px]:px-3.5 max-[640px]:pb-[max(16px,env(safe-area-inset-bottom))] max-[640px]:pt-3"
  )
);

const ControlsHeader = tw("div", "flex items-center border-b pb-[9px]");

const ControlsTitle = tw(
  "h3",
  "m-0 text-sm font-semibold leading-snug text-foreground"
);

// Label beside control, or above it when the control needs the full width.
const OptionGroup = ({
  $stacked,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"div"> & { $stacked?: boolean }) => (
  <div
    {...props}
    className={cn(
      "grid border-b border-border py-2",
      $stacked
        ? "grid-cols-1 items-start gap-[7px]"
        : "grid-cols-[7rem_minmax(0,1fr)] items-center gap-2.5 max-[400px]:grid-cols-[6.25rem_minmax(0,1fr)] max-[400px]:gap-2",
      className
    )}
  />
);

const OptionLabel = tw(
  "span",
  "text-[0.8125rem] font-medium leading-snug text-muted-foreground"
);

const SelectWrap = tw("div", "relative min-w-0");

// 16px on touch: anything smaller makes iOS Safari zoom the page on focus.
const SelectControl = tw(
  "select",
  "min-h-[34px] w-full appearance-none rounded-lg border border-muted-foreground/30 bg-muted py-1.5 pl-2.5 pr-[30px] text-[0.8125rem] leading-snug text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring pointer-coarse:min-h-11 pointer-coarse:text-base"
);

const SelectIcon = tw(
  "span",
  "pointer-events-none absolute right-3 top-1/2 size-[7px] -translate-y-[70%] rotate-45 border-b-[1.5px] border-r-[1.5px] border-muted-foreground"
);

const SegmentedControl = tw(
  "div",
  "inline-flex w-fit max-w-full flex-wrap gap-0.5 rounded-lg border bg-muted p-0.5"
);

const SegmentButton = ({
  $active,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"button"> & { $active: boolean }) => (
  <button
    {...props}
    className={cn(
      "inline-flex min-h-7 items-center justify-center rounded-md border px-2 py-1 text-xs font-[550] leading-none transition-all duration-150 active:translate-y-px focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring motion-reduce:transition-none pointer-coarse:min-h-11 pointer-coarse:px-3",
      $active
        ? "border-muted-foreground/30 bg-primary/10 text-foreground"
        : "border-transparent bg-transparent text-muted-foreground hover:text-foreground",
      className
    )}
  />
);

// Ten across where there is a pointer; five 44px targets on touch.
const SwatchRow = tw(
  "div",
  "grid w-full grid-cols-[repeat(10,28px)] justify-between gap-0 pointer-coarse:w-fit pointer-coarse:grid-cols-[repeat(5,44px)] pointer-coarse:justify-start pointer-coarse:gap-2 max-[640px]:w-fit max-[640px]:grid-cols-[repeat(5,44px)] max-[640px]:justify-start max-[640px]:gap-2"
);

const Swatch = ({
  $active,
  $color,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"button"> & {
  $active: boolean;
  $color: string;
}) => (
  <button
    {...props}
    style={{ background: $color, ...style }}
    className={cn(
      // The ring is the selection state; the inner shadow keeps a pale swatch
      // from disappearing into the card behind it.
      "size-7 cursor-pointer rounded-[7px] border-2 border-card transition-transform duration-100 active:translate-y-px focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring motion-reduce:transition-none pointer-coarse:size-11 max-[640px]:size-11",
      $active
        ? "shadow-[0_0_0_1px_var(--foreground),inset_0_0_0_1px_rgba(0,0,0,0.18)]"
        : "shadow-[0_0_0_1px_color-mix(in_srgb,var(--muted-foreground)_30%,transparent),inset_0_0_0_1px_rgba(0,0,0,0.18)]"
    )}
  />
);

const SnippetSection = tw(
  "div",
  "mt-2.5 flex flex-col gap-2.5 rounded-[10px] border border-muted-foreground/30 bg-background p-3"
);

const SnippetHeader = tw(
  "div",
  "flex flex-wrap items-center justify-between gap-2"
);

const SnippetTitle = tw(
  "h3",
  "m-0 text-[0.8125rem] font-semibold leading-snug text-foreground"
);

const InlineActions = tw("div", "flex flex-wrap gap-[5px]");

const CodeBlock = tw(
  "pre",
  "m-0 max-h-[92px] overflow-auto whitespace-pre-wrap break-words rounded-lg border bg-muted p-2.5 font-mono text-[0.6875rem] leading-relaxed text-foreground max-[840px]:max-h-none max-[840px]:overflow-visible"
);

const PrimaryActions = tw("div", "flex flex-wrap gap-[7px]");

const SecondaryLink = tw(
  "a",
  "inline-flex min-h-[34px] items-center justify-center rounded-lg border border-muted-foreground/30 bg-muted px-[11px] py-[7px] text-xs font-[550] leading-none text-foreground no-underline transition-colors duration-150 hover:bg-primary/10 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring motion-reduce:transition-none pointer-coarse:min-h-11 pointer-coarse:px-3.5"
);

function CloseIcon() {
  return (
    <svg
      aria-hidden="true"
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
    >
      <path
        d="M18 6L6 18"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
      <path
        d="M6 6L18 18"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  );
}
