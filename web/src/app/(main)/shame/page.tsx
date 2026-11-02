import type { Metadata } from "next";
import { isNotNull, desc, eq } from "drizzle-orm";
import { db, users, submissions } from "@/lib/db";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { BannedList, type BannedUser } from "@/components/shame/BannedList";
import { CONTAINER } from "@/components/layout/Container";
import { PageHeader } from "@/components/layout/PageHeader";
import { CONTACT_EMAIL } from "@/components/legal/LegalPage";
import { ogImageUrl } from "@/lib/og/ogImage";
import { cn } from "@/lib/utils";
import { getTranslator } from "@/lib/i18n/server";
import { richText } from "@/lib/i18n/rich";

export async function generateMetadata(): Promise<Metadata> {
  const { t } = await getTranslator();

  return {
    title: t("shame.meta.title"),
    description: t("shame.meta.description"),
    openGraph: {
      title: t("shame.meta.ogTitle"),
      description: t("shame.meta.ogDescription"),
      url: "https://tokens.ci",
      siteName: "Tokens",
      images: [
        {
          url: ogImageUrl({
            title: t("shame.meta.ogImageTitle"),
            subtitle: t("shame.meta.ogImageSubtitle"),
          }),
          width: 1200,
          height: 630,
        },
      ],
    },
    twitter: { card: "summary_large_image" },
  };
}

// Rendered per request rather than on an ISR window. The page reads
// `searchParams` for the banned-sign-in notice, which already makes it
// dynamic; pairing that with `revalidate` made regeneration fail on Workers
// and the route flapped between a cached 200 and a 500. The query is a single
// indexed scan over a handful of rows, so per-request is cheap.
export const dynamic = "force-dynamic";

function isMissingDatabaseUrl(error: unknown): boolean {
  return (
    error instanceof Error &&
    (error.message === "DATABASE_URL environment variable is not set" ||
      error.message.includes("connect") ||
      error.message.includes("ECONNREFUSED") ||
      (error as { code?: string }).code === "ECONNREFUSED")
  );
}

async function getBannedUsers(): Promise<BannedUser[]> {
  try {
    return await db
      .select({
        id: users.id,
        username: users.username,
        displayName: users.displayName,
        avatarUrl: users.avatarUrl,
        bannedAt: users.bannedAt,
        banReason: users.banReason,
        claimedTokens: submissions.totalTokens,
        claimedCost: submissions.totalCost,
      })
      .from(users)
      .leftJoin(submissions, eq(submissions.userId, users.id))
      .where(isNotNull(users.bannedAt))
      .orderBy(desc(users.bannedAt));
  } catch (error) {
    if (isMissingDatabaseUrl(error)) {
      return [];
    }
    throw error;
  }
}

interface PageProps {
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
}

export default async function HallOfShamePage({ searchParams }: PageProps) {
  const [{ locale, t }, banned, params] = await Promise.all([
    getTranslator(),
    getBannedUsers(),
    searchParams,
  ]);
  const showBannedLoginError = params.error === "account_banned";

  return (
    <main className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")} id="main-content">
      {showBannedLoginError && (
        <Alert variant="destructive" className="mb-6">
          <AlertTitle>{t("shame.bannedAccount.title")}</AlertTitle>
          <AlertDescription>{t("shame.bannedAccount.description")}</AlertDescription>
        </Alert>
      )}

      <div className="mx-auto w-full max-w-[860px]">
      <PageHeader
        title={t("shame.title")}
        description={t("shame.description")}
      />

      <div>
        <BannedList users={banned} locale={locale} t={t} />
      </div>

      <Card className="mt-10">
        <CardHeader>
          <CardTitle className="text-sm">{t("shame.report.title")}</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm leading-relaxed text-muted-foreground">
            {richText(t("shame.report.body"), {
              email: (
                <a
                  href={`mailto:${CONTACT_EMAIL}`}
                  className="underline underline-offset-2 transition-colors hover:text-foreground"
                >
                  {CONTACT_EMAIL}
                </a>
              ),
            })}
          </p>
        </CardContent>
      </Card>
      </div>
    </main>
  );
}
