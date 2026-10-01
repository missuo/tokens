import { revalidatePath, revalidateTag } from "next/cache";
import { and, eq, gt, isNull, type SQL } from "drizzle-orm";
import { db, sessions, users } from "@/lib/db";
import { getBearerToken } from "@/lib/auth/bearerToken";
import { READ_TOKEN_PREFIX, verifyReadTokenForUser } from "@/lib/auth/readTokens";
import { hashToken } from "@/lib/auth/utils";
import {
  normalizeUsernameCacheKey,
  revalidateUsernamePaths,
} from "@/lib/db/usernameLookup";

const SESSION_COOKIE_NAME = "tt_session";

/**
 * Responses that carry a private account's data must never reach a shared
 * cache: the edge keys on URL alone, so a copy stored for the owner would be
 * handed to the next anonymous reader of the same URL.
 */
export const PRIVATE_CACHE_CONTROL = "private, no-store, max-age=0";

/**
 * Rows a public surface (leaderboard, ranking, global totals) may count.
 * Banned and private accounts are both invisible there; the difference is
 * that a banned profile still renders its notice, and a private one does not.
 */
export function publicUserCondition(): SQL {
  return and(isNull(users.bannedAt), eq(users.isPrivate, false)) as SQL;
}

/**
 * A read token from `Authorization: Bearer tkr_…`, or from `?token=tkr_…` for
 * the places that cannot set a header — an `<img>` badge, an embed in a
 * README. Anything that is not shaped like a read token is ignored, so a `tt_`
 * submit token in the header is never mistaken for one.
 */
export function getReadTokenFromRequest(request: Request): string | null {
  const headerToken = getBearerToken(request.headers.get("Authorization"));
  if (headerToken?.startsWith(READ_TOKEN_PREFIX)) return headerToken;

  try {
    const queryToken = new URL(request.url).searchParams.get("token")?.trim();
    if (queryToken?.startsWith(READ_TOKEN_PREFIX)) return queryToken;
  } catch {
    // Unparseable URL: there is no query token to read.
  }

  return null;
}

function getCookie(request: Request, name: string): string | null {
  const header = request.headers.get("cookie");
  if (!header) return null;

  for (const part of header.split(";")) {
    const separator = part.indexOf("=");
    if (separator === -1) continue;
    if (part.slice(0, separator).trim() !== name) continue;
    const value = part.slice(separator + 1).trim();
    try {
      return decodeURIComponent(value);
    } catch {
      return value;
    }
  }

  return null;
}

/**
 * The signed-in user id behind the request's session cookie, read from the
 * request itself rather than `cookies()`. The profile loaders run inside
 * `unstable_cache` with a synthetic request, where `cookies()` throws; reading
 * the header keeps them working and, because that synthetic request carries no
 * cookie, guarantees nothing cached there is ever an owner's view.
 */
async function getSessionUserIdFromRequest(request: Request): Promise<string | null> {
  const sessionToken = getCookie(request, SESSION_COOKIE_NAME);
  if (!sessionToken) return null;

  const [row] = await db
    .select({ userId: sessions.userId })
    .from(sessions)
    .innerJoin(users, eq(sessions.userId, users.id))
    .where(
      and(
        eq(sessions.tokenHash, hashToken(sessionToken)),
        gt(sessions.expiresAt, new Date()),
        isNull(users.bannedAt)
      )
    )
    .limit(1);

  return row?.userId ?? null;
}

export interface ViewerAccess {
  /** The request may read this user's data. */
  allowed: boolean;
  /**
   * The request is the user themselves, by session cookie. Owners see their
   * devices' real names; everyone else — read-token holders included, since a
   * token can be handed to anyone — sees them masked.
   */
  isOwner: boolean;
}

/**
 * What this request may see of `user`. Public accounts are readable by
 * anyone. A private one only by its owner's session or one of its read
 * tokens; `allowed: false` means the caller should answer exactly as it would
 * for a user who does not exist, so a private account cannot be told apart
 * from no account.
 */
export async function getViewerAccess(
  request: Request,
  user: { id: string; isPrivate: boolean }
): Promise<ViewerAccess> {
  const isOwner = (await getSessionUserIdFromRequest(request)) === user.id;
  if (!user.isPrivate || isOwner) {
    return { allowed: true, isOwner };
  }

  const readToken = getReadTokenFromRequest(request);
  const allowed = readToken ? await verifyReadTokenForUser(readToken, user.id) : false;
  return { allowed, isOwner: false };
}

/**
 * Drop every cached copy of `username`'s public surfaces after their
 * visibility changes, so going private takes effect now rather than when the
 * next 60s revalidation happens to run. Same tag set account deletion uses.
 */
export function revalidateUserVisibility(username: string): void {
  const usernameCacheKey = normalizeUsernameCacheKey(username);
  try {
    revalidateTag("leaderboard", "max");
    revalidateTag(`user:${usernameCacheKey}`, "max");
    revalidateTag("user-rank", "max");
    revalidateTag(`user-rank:${usernameCacheKey}`, "max");
    revalidateTag(`embed-user:${usernameCacheKey}`, "max");
    revalidateTag(`embed-user:${usernameCacheKey}:tokens`, "max");
    revalidateTag(`embed-user:${usernameCacheKey}:cost`, "max");
    revalidateTag(`embed-contrib:${usernameCacheKey}`, "max");
    revalidateTag(`embed-today:${usernameCacheKey}`, "max");

    revalidatePath("/leaderboard");
    revalidateUsernamePaths(username);
  } catch {
    // Cache invalidation is best-effort.
  }
}

export async function setUserPrivacy(userId: string, isPrivate: boolean): Promise<void> {
  await db
    .update(users)
    .set({ isPrivate, updatedAt: new Date() })
    .where(eq(users.id, userId));
}

export async function getUserPrivacy(userId: string): Promise<boolean> {
  const [row] = await db
    .select({ isPrivate: users.isPrivate })
    .from(users)
    .where(eq(users.id, userId))
    .limit(1);
  return row?.isPrivate ?? false;
}
