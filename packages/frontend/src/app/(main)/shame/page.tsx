import type { Metadata } from "next";
import { isNotNull, desc, eq } from "drizzle-orm";
import { db, users, submissions } from "@/lib/db";
import { formatNumber, formatCurrency } from "@/lib/format";

export const metadata: Metadata = {
  title: "Hall of Shame - Tokens",
  description:
    "Accounts banned from the Tokens leaderboard for submitting fraudulent usage data.",
};

// The list only changes when a ban is issued; a short ISR window keeps the
// page fresh without hitting the database on every view.
export const revalidate = 300;

interface BannedUserRow {
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
  bannedAt: Date | null;
  banReason: string | null;
  claimedTokens: number | null;
  claimedCost: string | null;
}

function isMissingDatabaseUrl(error: unknown): boolean {
  return error instanceof Error && error.message === "DATABASE_URL environment variable is not set";
}

async function getBannedUsers(): Promise<BannedUserRow[]> {
  try {
    return await db
      .select({
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

function formatBanDate(date: Date | null): string {
  if (!date) {
    return "";
  }
  return date.toISOString().slice(0, 10);
}

function BannedUserCard({ user }: { user: BannedUserRow }) {
  const avatar = user.avatarUrl || `https://github.com/${user.username}.png`;

  return (
    <article className="rounded-xl border border-danger/30 bg-surface p-5">
      <div className="flex items-start gap-4">
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          src={avatar}
          alt={user.username}
          width={48}
          height={48}
          className="h-12 w-12 shrink-0 rounded-lg object-cover opacity-80 grayscale ring-1 ring-line"
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <h2 className="truncate font-mono text-base font-bold text-foreground">
              @{user.username}
            </h2>
            <span className="rounded-md bg-danger/10 px-2 py-0.5 font-mono text-[11px] font-semibold uppercase tracking-wide text-danger">
              Banned
            </span>
            {user.bannedAt && (
              <span className="font-mono text-xs text-muted">
                {formatBanDate(user.bannedAt)}
              </span>
            )}
          </div>
          {user.claimedTokens != null && (
            <p className="mt-1 font-mono text-xs text-muted">
              Forged claim:{" "}
              <span className="line-through">
                {formatNumber(user.claimedTokens, true)} tokens
                {user.claimedCost != null &&
                  ` / ${formatCurrency(Number(user.claimedCost), true)}`}
              </span>{" "}
              — excluded from all rankings
            </p>
          )}
        </div>
      </div>
      {user.banReason && (
        <p className="mt-4 text-sm leading-relaxed text-foreground/90">
          {user.banReason}
        </p>
      )}
    </article>
  );
}

interface PageProps {
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
}

export default async function HallOfShamePage({ searchParams }: PageProps) {
  const [bannedUsers, params] = await Promise.all([getBannedUsers(), searchParams]);
  const showBannedLoginError = params.error === "account_banned";

  return (
    <main className="main-container" id="main-content">
      <div className="mx-auto max-w-[720px] px-4 py-10 sm:px-6">
        {showBannedLoginError && (
          <div className="mb-6 rounded-xl border border-danger/40 bg-danger/10 px-4 py-3 text-sm font-medium text-danger">
            This account has been banned and can no longer sign in.
          </div>
        )}

        <header>
          <h1 className="text-2xl font-bold tracking-tight text-foreground">
            Hall of Shame
          </h1>
          <p className="mt-2 text-sm leading-relaxed text-muted">
            The leaderboard only works if the numbers on it are real. Accounts
            listed here submitted fraudulent usage data and are permanently
            banned: they cannot sign in or submit again, and none of their data
            counts toward any ranking. Their forged records are preserved as
            evidence.
          </p>
        </header>

        <section className="mt-8 flex flex-col gap-4" aria-label="Banned accounts">
          {bannedUsers.length === 0 ? (
            <p className="rounded-xl border border-line bg-surface px-4 py-6 text-center text-sm text-muted">
              No banned accounts. Keep it that way.
            </p>
          ) : (
            bannedUsers.map((user) => (
              <BannedUserCard key={user.username} user={user} />
            ))
          )}
        </section>

        <section className="mt-10 rounded-xl border border-line bg-surface p-5">
          <h2 className="text-sm font-semibold text-foreground">
            Spotted something suspicious?
          </h2>
          <p className="mt-2 text-sm leading-relaxed text-muted">
            We welcome reports from the community. If a profile&apos;s numbers
            look fabricated, let us know — every report is investigated against
            the submitted raw data. Dedicated reporting channels will be
            announced here soon.
          </p>
        </section>
      </div>
    </main>
  );
}
