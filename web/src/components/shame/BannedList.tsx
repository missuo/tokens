import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { formatCurrency, formatNumber } from "@/lib/format";

export interface BannedUser {
  id: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
  bannedAt: Date | null;
  banReason: string | null;
  claimedTokens: number | null;
  claimedCost: string | null;
}

/**
 * Cheaters often name the account after the brand they are promoting, so the
 * listing never prints the name in full — documenting the ban without handing
 * them the exposure they were after.
 */
export function maskUsername(username: string): string {
  if (username.length <= 3) {
    return `${username[0] ?? ""}${"*".repeat(Math.max(username.length - 1, 2))}`;
  }
  const keepEnd = username.length >= 7 ? 2 : 1;
  return `${username.slice(0, 2)}${"*".repeat(username.length - 2 - keepEnd)}${username.slice(-keepEnd)}`;
}

/**
 * One entry per ban.
 *
 * Built as a list rather than a stack of cards because this page only grows:
 * the reason is the substance, so it stays visible, but the surrounding
 * chrome is kept to a rule and a row of facts so fifty entries still scan.
 */
function BannedEntry({ user }: { user: BannedUser }) {
  const masked = maskUsername(user.username);
  const bannedOn = user.bannedAt ? user.bannedAt.toISOString().slice(0, 10) : null;

  return (
    <li className="border-b py-6 last:border-b-0">
      <div className="flex items-start gap-3.5">
        <Avatar className="size-9 shrink-0 grayscale">
          {user.avatarUrl && <AvatarImage src={user.avatarUrl} alt="" />}
          <AvatarFallback className="text-xs">
            {user.username[0]?.toUpperCase() ?? "?"}
          </AvatarFallback>
        </Avatar>

        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
            <span className="font-mono text-sm font-medium">@{masked}</span>
            <Badge variant="destructive">Banned</Badge>
            {bannedOn && (
              <span className="font-mono text-xs text-muted-foreground">{bannedOn}</span>
            )}
            {user.claimedTokens != null && (
              <span className="font-mono text-xs text-muted-foreground">
                claimed{" "}
                <span className="line-through">
                  {formatNumber(user.claimedTokens, true)}
                  {user.claimedCost != null &&
                    ` · ${formatCurrency(Number(user.claimedCost), true)}`}
                </span>
              </span>
            )}
          </div>

          {user.banReason && (
            <p className="text-sm leading-relaxed text-muted-foreground">
              {user.banReason}
            </p>
          )}
        </div>
      </div>
    </li>
  );
}

export function BannedList({ users }: { users: BannedUser[] }) {
  if (users.length === 0) {
    return (
      <Empty>
        <EmptyHeader>
          <EmptyTitle>No banned accounts</EmptyTitle>
          <EmptyDescription>Keep it that way.</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  return (
    <section>
      <div className="flex items-baseline justify-between border-b pb-2">
        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
          {users.length} account{users.length === 1 ? "" : "s"}
        </span>
        <span className="text-xs text-muted-foreground">Most recent first</span>
      </div>
      <ul className="mt-1 flex flex-col">
        {users.map((user) => (
          <BannedEntry key={user.id} user={user} />
        ))}
      </ul>
    </section>
  );
}
