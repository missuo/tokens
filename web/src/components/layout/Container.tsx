import { cn } from "@/lib/utils";

/**
 * The one page-width definition in the app.
 *
 * Header, footer and every route share it so the content edge never shifts
 * when navigating between them — width changing between pages reads as the
 * layout breaking, even when each page is fine on its own.
 */
export const CONTAINER = "mx-auto w-full max-w-[1200px] px-4 sm:px-6";

export function Container({
  className,
  children,
  as: Tag = "div",
  ...props
}: React.ComponentProps<"div"> & { as?: "div" | "main" | "section" }) {
  return (
    <Tag className={cn(CONTAINER, className)} {...props}>
      {children}
    </Tag>
  );
}
