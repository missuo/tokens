import { CONTAINER } from "@/components/layout/Container";
import { PageHeader } from "@/components/layout/PageHeader";
import { cn } from "@/lib/utils";

export const CONTACT_EMAIL = "hi@tokens.ci";

/**
 * Shared shell for the policy pages.
 *
 * Same width and heading rhythm as /docs so these do not read as bolted on,
 * but with a plain prose column: legal text is read top to bottom, not
 * scanned, so it gets no cards, no grid and no accent colour.
 */
export function LegalPage({
  title,
  description,
  updated,
  children,
}: {
  title: string;
  description: string;
  updated: string;
  children: React.ReactNode;
}) {
  return (
    <main className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")} id="main-content">
      <div className="mx-auto w-full max-w-[720px]">
        <PageHeader title={title} description={description} />
        <p className="-mt-2 text-xs text-muted-foreground">
          Last updated {updated}
        </p>
        <div className="mt-8 flex flex-col gap-8">{children}</div>
      </div>
    </main>
  );
}

export function Clause({
  heading,
  children,
}: {
  heading: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2.5">
      <h2 className="text-base font-semibold tracking-tight">{heading}</h2>
      <div className="flex flex-col gap-3 text-sm leading-relaxed text-muted-foreground [&_a]:underline [&_a]:underline-offset-2 [&_a:hover]:text-foreground">
        {children}
      </div>
    </section>
  );
}

export function Bullets({ items }: { items: React.ReactNode[] }) {
  return (
    <ul className="flex list-disc flex-col gap-1.5 pl-5">
      {items.map((item, i) => (
        <li key={i}>{item}</li>
      ))}
    </ul>
  );
}
