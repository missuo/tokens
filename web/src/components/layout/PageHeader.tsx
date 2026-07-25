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
        {description && (
          <p className="max-w-[72ch] text-sm leading-relaxed text-muted-foreground">
            {description}
          </p>
        )}
      </header>
      <Separator className="my-7" />
    </>
  );
}
