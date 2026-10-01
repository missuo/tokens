import { NextResponse } from "next/server";
import { eq } from "drizzle-orm";
import { getBearerToken } from "@/lib/auth/bearerToken";
import { authenticatePersonalToken } from "@/lib/auth/personalTokens";
import {
  issueReadTokenInTransaction,
  normalizeReadTokenName,
} from "@/lib/auth/readTokens";
import { db, users } from "@/lib/db";
import { PRIVATE_CACHE_CONTROL, revalidateUserVisibility } from "@/lib/privacy";

/**
 * POST /api/me/private — `tokens login --private`.
 *
 * Switches the account behind the Bearer submit token to private mode and
 * issues a read token, returned once in plaintext so the CLI can print it.
 * Running it again is safe: the account stays private and gets another token.
 *
 * Deliberately one-way. Making a profile public again only happens from the
 * Settings page with a browser session, so a submit token leaked from CI can
 * hide its owner's profile but never expose it.
 *
 * Body: { tokenName?: string }
 * 200:  { private: true, readToken: { id, name, token, createdAt } }
 */
export async function POST(request: Request) {
  try {
    const token = getBearerToken(request.headers.get("Authorization"));
    if (!token) {
      return NextResponse.json(
        { error: "Missing or invalid Authorization header" },
        { status: 401 }
      );
    }

    const auth = await authenticatePersonalToken(token);
    if (auth.status === "banned") {
      return NextResponse.json(
        { error: "This account has been banned for violating the platform rules" },
        { status: 403 }
      );
    }
    if (auth.status !== "valid") {
      return NextResponse.json(
        {
          error:
            auth.status === "expired" ? "API token has expired" : "Invalid API token",
        },
        { status: 401 }
      );
    }

    const body = await request.json().catch(() => ({}));
    const rawName =
      body && typeof body === "object"
        ? (body as { tokenName?: unknown }).tokenName
        : undefined;

    // One transaction: never a private account without the token the CLI is
    // about to print, nor a token minted for an account that did not switch.
    const readToken = await db.transaction(async (tx) => {
      await tx
        .update(users)
        .set({ isPrivate: true, updatedAt: new Date() })
        .where(eq(users.id, auth.userId));

      return issueReadTokenInTransaction(tx, {
        userId: auth.userId,
        name: normalizeReadTokenName(rawName),
      });
    });

    revalidateUserVisibility(auth.username);

    return NextResponse.json(
      {
        private: true,
        readToken: {
          id: readToken.id,
          name: readToken.name,
          token: readToken.token,
          createdAt: readToken.createdAt,
        },
      },
      { headers: { "Cache-Control": PRIVATE_CACHE_CONTROL } }
    );
  } catch (error) {
    console.error("Enable private mode error:", error);
    return NextResponse.json(
      { error: "Failed to enable private mode" },
      { status: 500 }
    );
  }
}
