import Link from "next/link";

export interface BannedProfileData {
  banned: true;
  bannedAt: string | null;
  banReason: string | null;
  user: {
    username: string;
    displayName: string | null;
    avatarUrl: string | null;
    createdAt: string;
  };
}

/**
 * Replacement for the normal profile when the account is banned. The URL
 * stays reachable, but every statistic is withheld: the page renders only
 * the identity, a ban stamp, and the reason. The identity block is pushed
 * to grayscale so the stamp is the only thing with any color.
 */
export default function BannedProfileView({ data }: { data: BannedProfileData }) {
  const { user } = data;
  const banDate = data.bannedAt ? data.bannedAt.slice(0, 10) : null;

  return (
    <main className="main-container" id="main-content">
      <div className="mx-auto max-w-[560px] px-4 py-16 sm:px-6">
        <section className="relative overflow-hidden rounded-2xl border border-line bg-surface p-8 text-center">
          {/* Rotated rubber-stamp mark — deliberately the only colored element */}
          <div
            aria-hidden="true"
            className="pointer-events-none absolute right-4 top-6 rotate-12 rounded-md border-4 border-danger/70 px-3 py-1 font-mono text-xl font-black uppercase tracking-widest text-danger/80"
          >
            Banned
          </div>

          <div className="grayscale">
            {user.avatarUrl ? (
              // eslint-disable-next-line @next/next/no-img-element
              <img
                src={user.avatarUrl}
                alt={user.username}
                width={96}
                height={96}
                className="mx-auto h-24 w-24 rounded-2xl object-cover opacity-60 ring-1 ring-line"
              />
            ) : (
              <div className="mx-auto grid h-24 w-24 place-items-center rounded-2xl bg-foreground/10 font-mono text-3xl font-bold text-muted ring-1 ring-line">
                {user.username[0]?.toUpperCase() ?? "?"}
              </div>
            )}

            <h1 className="mt-5 font-mono text-xl font-bold text-muted line-through">
              @{user.username}
            </h1>
          </div>

          <p className="mt-2 inline-block rounded-md bg-danger/10 px-2.5 py-1 font-mono text-xs font-semibold uppercase tracking-wide text-danger">
            Account banned{banDate ? ` on ${banDate}` : ""}
          </p>

          <p className="mt-6 text-sm leading-relaxed text-muted">
            This account has been permanently banned for submitting fraudulent
            usage data. All of its statistics have been removed from public
            view and are excluded from every ranking.
          </p>

          {data.banReason && (
            <div className="mt-6 rounded-xl border border-danger/25 bg-danger/5 p-4 text-left">
              <h2 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-danger">
                Ban reason
              </h2>
              <p className="mt-2 text-sm leading-relaxed text-foreground/85">
                {data.banReason}
              </p>
            </div>
          )}

          <Link
            href="/shame"
            className="mt-8 inline-block rounded-lg border border-line bg-background px-4 py-2 text-sm font-medium text-foreground transition hover:border-foreground/25"
          >
            View the Hall of Shame →
          </Link>
        </section>
      </div>
    </main>
  );
}
