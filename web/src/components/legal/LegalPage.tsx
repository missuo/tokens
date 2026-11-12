import { CONTAINER } from "@/components/layout/Container";
import { PageHeader } from "@/components/layout/PageHeader";
import { cn } from "@/lib/utils";
import { getTranslator } from "@/lib/i18n/server";
import { DEFAULT_LOCALE } from "@/lib/i18n/config";
import { richText } from "@/lib/i18n/rich";

export const CONTACT_EMAIL = "hi@tokens.ci";

/**
 * Shared shell for the policy pages.
 *
 * Same width and heading rhythm as /docs so these do not read as bolted on,
 * but with a plain prose column: legal text is read top to bottom, not
 * scanned, so it gets no cards, no grid and no accent colour.
 */
export async function LegalPage({
  title,
  description,
  updated,
  englishHref,
  children,
}: {
  title: string;
  description: string;
  updated: string;
  /** The unprefixed (English, binding) address of this policy. */
  englishHref: string;
  children: React.ReactNode;
}) {
  const { locale, t } = await getTranslator();

  return (
    <main className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")} id="main-content">
      <div className="mx-auto w-full max-w-[720px]">
        {/* A translation is a courtesy, not the agreement: the English text
            is the one that holds, so every other language says so up front. */}
        {locale !== DEFAULT_LOCALE && (
          <p className="mb-6 rounded-md border bg-muted/40 px-3 py-2 text-xs leading-relaxed text-muted-foreground">
            {richText(t("legal.translationNotice"), {
              english: (
                <a
                  href={englishHref}
                  hrefLang="en"
                  className="underline underline-offset-2 hover:text-foreground"
                >
                  {t("legal.englishVersion")}
                </a>
              ),
            })}
          </p>
        )}
        <PageHeader title={title} description={description} />
        <p className="-mt-2 text-xs text-muted-foreground">
          {t("legal.lastUpdated", { date: updated })}
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
