import { randomBytes } from "crypto";
import { and, desc, eq, sql } from "drizzle-orm";
import { db, readTokens } from "@/lib/db";
import { hashToken } from "@/lib/auth/utils";

/**
 * Read tokens unlock a private account's data for whoever holds one — the iOS
 * app, a badge in a private README, a script. They are read-only by
 * construction: they live in their own table with their own `tkr_` prefix, so
 * nothing that authenticates submissions (`tt_` tokens) ever looks at them.
 */
export const READ_TOKEN_PREFIX = "tkr_";
export const READ_TOKEN_NAME_MAX_LENGTH = 100;
export const DEFAULT_READ_TOKEN_NAME = "Read token";

const READ_TOKEN_NAME_LOCK_NAMESPACE = "read_token_names";

type ReadTokenDb = Pick<typeof db, "execute" | "insert" | "select">;

export interface ReadTokenListItem {
  id: string;
  name: string;
  createdAt: Date;
  lastUsedAt: Date | null;
}

export interface IssuedReadToken extends ReadTokenListItem {
  token: string;
}

export function generateReadToken(): string {
  return `${READ_TOKEN_PREFIX}${randomBytes(24).toString("hex")}`;
}

export function normalizeReadTokenName(value: unknown): string {
  if (typeof value !== "string") return DEFAULT_READ_TOKEN_NAME;
  // Control characters would let a name smuggle newlines or escape sequences
  // into the Settings list and the CLI's terminal output.
  const cleaned = value.replace(/\p{C}/gu, "").trim();
  return cleaned ? cleaned.slice(0, READ_TOKEN_NAME_MAX_LENGTH) : DEFAULT_READ_TOKEN_NAME;
}

function uniqueName(baseName: string, existing: Iterable<string>): string {
  const names = new Set(existing);
  let finalName = baseName;
  let counter = 1;
  while (names.has(finalName)) {
    const suffix = ` (${counter})`;
    finalName = `${baseName.slice(0, READ_TOKEN_NAME_MAX_LENGTH - suffix.length)}${suffix}`;
    counter++;
  }
  return finalName;
}

/**
 * Issue a read token inside an existing transaction. Names are made unique per
 * user under an advisory lock, mirroring personal tokens, so two concurrent
 * `tokens login --private` runs on one host cannot collide on the constraint.
 */
export async function issueReadTokenInTransaction(
  tx: ReadTokenDb,
  { userId, name }: { userId: string; name: string }
): Promise<IssuedReadToken> {
  await tx.execute(sql`
    SELECT pg_advisory_xact_lock(
      hashtext(${READ_TOKEN_NAME_LOCK_NAMESPACE}),
      hashtext(${userId})
    )
  `);

  const existing = await tx
    .select({ name: readTokens.name })
    .from(readTokens)
    .where(eq(readTokens.userId, userId));

  const token = generateReadToken();
  const [created] = await tx
    .insert(readTokens)
    .values({
      userId,
      tokenHash: hashToken(token),
      name: uniqueName(name, existing.map((row) => row.name)),
    })
    .returning({
      id: readTokens.id,
      name: readTokens.name,
      createdAt: readTokens.createdAt,
      lastUsedAt: readTokens.lastUsedAt,
    });

  return { ...created, token };
}

export function issueReadToken(input: {
  userId: string;
  name: string;
}): Promise<IssuedReadToken> {
  return db.transaction((tx) => issueReadTokenInTransaction(tx, input));
}

export function listReadTokens(userId: string): Promise<ReadTokenListItem[]> {
  return db
    .select({
      id: readTokens.id,
      name: readTokens.name,
      createdAt: readTokens.createdAt,
      lastUsedAt: readTokens.lastUsedAt,
    })
    .from(readTokens)
    .where(eq(readTokens.userId, userId))
    .orderBy(desc(readTokens.createdAt));
}

export async function revokeReadToken(
  userId: string,
  tokenId: string
): Promise<boolean> {
  const result = await db
    .delete(readTokens)
    .where(and(eq(readTokens.id, tokenId), eq(readTokens.userId, userId)))
    .returning({ id: readTokens.id });
  return result.length > 0;
}

/**
 * True when `token` is a live read token belonging to `userId`. Matching on
 * both columns means a valid token for one account is simply "invalid" for
 * every other, with no way to tell the two cases apart from outside.
 */
export async function verifyReadTokenForUser(
  token: string,
  userId: string
): Promise<boolean> {
  if (!token.startsWith(READ_TOKEN_PREFIX)) return false;

  const [record] = await db
    .select({ id: readTokens.id })
    .from(readTokens)
    .where(
      and(eq(readTokens.tokenHash, hashToken(token)), eq(readTokens.userId, userId))
    )
    .limit(1);

  if (!record) return false;

  // Best-effort: a failed timestamp write must not turn a valid read into a
  // denied one.
  try {
    await db
      .update(readTokens)
      .set({ lastUsedAt: new Date() })
      .where(eq(readTokens.id, record.id));
  } catch (error) {
    console.error("Read token lastUsedAt update error:", error);
  }

  return true;
}
