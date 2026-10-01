import { NextResponse } from "next/server";
import { getSessionFromRequest } from "@/lib/auth/requestSession";
import {
  issueReadToken,
  listReadTokens,
  normalizeReadTokenName,
} from "@/lib/auth/readTokens";

/**
 * Read tokens for a private profile. Session-cookie only: a read token grants
 * access to everything the profile shows, so minting one is a Settings action,
 * not something a submit token in CI can do.
 */
export async function GET(request: Request) {
  try {
    const session = await getSessionFromRequest(request, {
      allowAuthorizationHeader: false,
    });
    if (!session) {
      return NextResponse.json({ error: "Not authenticated" }, { status: 401 });
    }

    const tokens = await listReadTokens(session.id);
    return NextResponse.json({ tokens });
  } catch (error) {
    console.error("Read tokens list error:", error);
    return NextResponse.json(
      { error: "Failed to fetch read tokens" },
      { status: 500 }
    );
  }
}

export async function POST(request: Request) {
  try {
    const session = await getSessionFromRequest(request, {
      allowAuthorizationHeader: false,
    });
    if (!session) {
      return NextResponse.json({ error: "Not authenticated" }, { status: 401 });
    }

    const body = await request.json().catch(() => ({}));
    const rawName =
      body && typeof body === "object" ? (body as { name?: unknown }).name : undefined;

    const token = await issueReadToken({
      userId: session.id,
      name: normalizeReadTokenName(rawName),
    });

    return NextResponse.json({ token }, { status: 201 });
  } catch (error) {
    console.error("Read token create error:", error);
    return NextResponse.json(
      { error: "Failed to create read token" },
      { status: 500 }
    );
  }
}
