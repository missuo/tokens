import { NextResponse } from "next/server";
import { getSessionFromRequest } from "@/lib/auth/requestSession";
import {
  PRIVATE_CACHE_CONTROL,
  getUserPrivacy,
  revalidateUserVisibility,
  setUserPrivacy,
} from "@/lib/privacy";

/**
 * GET   /api/settings/privacy  → { private: boolean }
 * PATCH /api/settings/privacy  { private: boolean } → { private: boolean }
 *
 * Session-cookie only, like every other Settings mutation: making a profile
 * public again is not something a CI submit token should be able to do. (The
 * CLI can only switch the other way, via POST /api/me/private.)
 */
export async function GET(request: Request) {
  try {
    const session = await getSessionFromRequest(request, {
      allowAuthorizationHeader: false,
    });
    if (!session) {
      return NextResponse.json({ error: "Not authenticated" }, { status: 401 });
    }

    return NextResponse.json(
      { private: await getUserPrivacy(session.id) },
      { headers: { "Cache-Control": PRIVATE_CACHE_CONTROL } }
    );
  } catch (error) {
    console.error("Privacy read error:", error);
    return NextResponse.json(
      { error: "Failed to load privacy settings" },
      { status: 500 }
    );
  }
}

export async function PATCH(request: Request) {
  try {
    const session = await getSessionFromRequest(request, {
      allowAuthorizationHeader: false,
    });
    if (!session) {
      return NextResponse.json({ error: "Not authenticated" }, { status: 401 });
    }

    const body = await request.json().catch(() => null);
    const isPrivate =
      body && typeof body === "object" ? (body as { private?: unknown }).private : undefined;
    if (typeof isPrivate !== "boolean") {
      return NextResponse.json(
        { error: "`private` must be a boolean" },
        { status: 400 }
      );
    }

    await setUserPrivacy(session.id, isPrivate);
    revalidateUserVisibility(session.username);

    return NextResponse.json({ private: isPrivate });
  } catch (error) {
    console.error("Privacy update error:", error);
    return NextResponse.json(
      { error: "Failed to update privacy settings" },
      { status: 500 }
    );
  }
}
