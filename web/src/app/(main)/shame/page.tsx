import type { Metadata } from "next";
import { isNotNull, desc, eq } from "drizzle-orm";
import { db, users, submissions } from "@/lib/db";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { BannedList, type BannedUser } from "@/components/shame/BannedList";
import { CONTAINER } from "@/components/layout/Container";
import { PageHeader } from "@/components/layout/PageHeader";
import { CONTACT_EMAIL } from "@/components/legal/LegalPage";
import { cn } from "@/lib/utils";

export const metadata: Metadata = {
  title: "Hall of Shame - Tokens",
  description:
    "Accounts banned from the Tokens leaderboard for submitting fraudulent usage data.",
  openGraph: {
    title: "Hall of Shame — Tokens",
    description: "Accounts banned for submitting fraudulent usage data.",
    url: "https://tokens.ci",
    siteName: "Tokens",
    images: [
      {
        url: `/api/og?title=Hall+of+Shame&subtitle=Accounts+banned+for+submitting+fraudulent+usage+data.`,
        width: 1200,
        height: 630,
      },
    ],
  },
  twitter: { card: "summary_large_image" },
};

// Rendered per request rather than on an ISR window. The page reads
// `searchParams` for the banned-sign-in notice, which already makes it
// dynamic; pairing that with `revalidate` made regeneration fail on Workers
// and the route flapped between a cached 200 and a 500. The query is a single
// indexed scan over a handful of rows, so per-request is cheap.
export const dynamic = "force-dynamic";

function isMissingDatabaseUrl(error: unknown): boolean {
  return (
    error instanceof Error &&
    error.message === "DATABASE_URL environment variable is not set"
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
  const [banned, params] = await Promise.all([getBannedUsers(), searchParams]);
  const showBannedLoginError = params.error === "account_banned";

  return (
    <main className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")} id="main-content">
      {showBannedLoginError && (
        <Alert variant="destructive" className="mb-6">
          <AlertTitle>Account banned</AlertTitle>
          <AlertDescription>
            This account has been banned and can no longer sign in.
          </AlertDescription>
        </Alert>
      )}

      <div className="mx-auto w-full max-w-[860px]">
      <PageHeader
        title="Hall of Shame"
        description="The leaderboard only works if the numbers on it are real. Accounts listed here submitted fraudulent usage data and are permanently banned: they cannot sign in or submit again, and none of their data counts toward any ranking. Their records are kept as evidence, and usernames are partially masked so a ban cannot double as advertising."
      />

      <div>
        <BannedList users={banned} />
      </div>

      <Card className="mt-10">
        <CardHeader>
          <CardTitle className="text-sm">Spotted something suspicious?</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm leading-relaxed text-muted-foreground">
            Reports from the community are welcome. If a profile&apos;s numbers
            look fabricated, email{" "}
            <a
              href={`mailto:${CONTACT_EMAIL}`}
              className="underline underline-offset-2 transition-colors hover:text-foreground"
            >
              {CONTACT_EMAIL}
            </a>{" "}
            — every report is checked against the submitted raw data before
            anyone is banned.
          </p>
        </CardContent>
      </Card>
      </div>
    </main>
  );
}
