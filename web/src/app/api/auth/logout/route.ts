import { NextResponse } from "next/server";
import { clearSession } from "@/lib/auth/session";
import { hasAllowedOrigin } from "@/lib/auth/requestSession";

export async function POST(request: Request) {
  // The CSRF Origin allowlist applies — otherwise any site could force a
  // logout. But deliberately no session requirement: `getSession()` returns
  // null for an expired row *and* for a banned user, so gating the clear on it
  // meant the two states where signing out matters most were the two where it
  // silently did nothing, leaving `tt_session` alive for its full 30-day
  // maxAge. Clearing a cookie is not a privileged operation and `clearSession`
  // is idempotent.
  if (!hasAllowedOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  await clearSession();

  return NextResponse.json({
    success: true,
  });
}
