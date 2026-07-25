import { BoxIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { BRAND_ICON_PATHS } from "@/lib/brandIcons.generated";
import { isSyntheticModel, modelIconSlug } from "@/lib/modelIcons";

/**
 * Brand mark for a model or platform.
 *
 * The mark is inlined rather than loaded through an <img> so its
 * `fill="currentColor"` resolves against the surrounding text colour — an
 * external image is a separate document where currentColor never resolves,
 * which is how Kimi's white-on-transparent glyph ended up invisible on a light
 * page. Anything unmatched falls back to a neutral glyph: a gap in a column of
 * marks reads as a rendering bug, not as "unknown provider".
 */
export function BrandGlyph({
  slug,
  size = 14,
  className,
}: {
  slug: string;
  size?: number;
  className?: string;
}) {
  const markup = BRAND_ICON_PATHS[slug];
  if (!markup) return null;

  return (
    // fill lives on the wrapper because the source SVGs declare
    // fill="currentColor" on their root and let the paths inherit it —
    // stripping the root to inline the markup dropped it, which left every
    // mark painting default black and vanishing in dark mode.
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="currentColor"
      aria-hidden="true"
      focusable="false"
      className={cn("shrink-0", className)}
      dangerouslySetInnerHTML={{ __html: markup }}
    />
  );
}

export function ModelIcon({
  model,
  size = 14,
  className,
}: {
  model: string;
  size?: number;
  className?: string;
}) {
  const slug = isSyntheticModel(model) ? null : modelIconSlug(model);
  const markup = slug ? BRAND_ICON_PATHS[slug] : undefined;

  if (!markup) {
    return (
      <BoxIcon
        className={cn("shrink-0 text-muted-foreground", className)}
        style={{ width: size, height: size }}
        aria-hidden="true"
      />
    );
  }

  return <BrandGlyph slug={slug!} size={size} className={className} />;
}
