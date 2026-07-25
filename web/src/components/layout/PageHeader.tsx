import { Separator } from "@/components/ui/separator";

/**
 * The page heading every route uses.
 *
 * It exists so the distance from the navbar to the first line of content is
 * identical everywhere — when that gap differs between routes, switching tabs
 * reads as the layout jumping even though each page is fine alone.
 */
export function PageHeader({
  title,
  description,
}: {
  title: string;
  description?: string;
}) {
  return (
    <>
      <header className="flex flex-col gap-1.5">
        <h1 className="text-2xl font-semibold tracking-tight sm:text-3xl">
          {title}
        </h1>
        {/* The measure is capped rather than left to the 1200px container: at
            this size a full-width line runs ~148 characters, which is past the
            point where the eye reliably finds the next line. 90ch (~700px) is
            the compromise — long descriptions like /shame's use noticeably more
            of the width than the previous 72ch, short ones are unaffected. */}
        {description && (
          <p className="max-w-[90ch] text-sm leading-relaxed text-muted-foreground">
            {description}
          </p>
        )}
      </header>
      <Separator className="my-7" />
    </>
  );
}
