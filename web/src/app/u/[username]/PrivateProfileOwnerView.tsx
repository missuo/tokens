"use client";

import { useEffect, useState, type ReactNode } from "react";
import Link from "next/link";
import { usePathname, useSearchParams } from "next/navigation";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { CONTAINER } from "@/components/layout/Container";
import type { ProfileDevice } from "@/components/profile";
import { cn } from "@/lib/utils";
import ProfilePageClient, { type ProfileData } from "./ProfilePageClient";

type OwnerState =
  | { kind: "not-owner" }
  | { kind: "owner"; data: ProfileData; devices: ProfileDevice[] };

function usernameFromPath(pathname: string | null): string | null {
  const match = pathname?.match(/^\/u\/([^/]+)\/?$/);
  if (!match) return null;
  try {
    return decodeURIComponent(match[1]);
  } catch {
    return null;
  }
}

/**
 * The profile page renders "not found" for a private account, and that page is
 * edge-cached and identical for every reader — which is the point: nobody can
 * tell a private account from a missing one.
 *
 * The owner still wants to see their own numbers, so this runs in the browser
 * after the shared page has loaded. If the signed-in user is the one in the
 * URL, it fetches the profile with their session cookie (which the API
 * accepts for a private account) and renders it in place of `fallback`.
 * Everyone else keeps seeing `fallback`, and nothing per-viewer ever enters
 * the cached HTML.
 */
export default function PrivateProfileOwnerView({
  fallback,
}: {
  fallback: ReactNode;
}) {
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const username = usernameFromPath(pathname);
  const period = searchParams.get("period");
  const [state, setState] = useState<OwnerState>({ kind: "not-owner" });

  useEffect(() => {
    if (!username) return;
    let cancelled = false;

    async function load(target: string) {
      try {
        const sessionResponse = await fetch("/api/auth/session");
        if (!sessionResponse.ok) return;
        const session = (await sessionResponse.json()) as {
          user?: { username?: string } | null;
        };
        const sessionUsername = session.user?.username;
        if (!sessionUsername || sessionUsername.toLowerCase() !== target.toLowerCase()) {
          return;
        }

        const query =
          period === "week" || period === "month" ? `?period=${period}` : "";
        const encoded = encodeURIComponent(sessionUsername);
        const [profileResponse, devicesResponse] = await Promise.all([
          fetch(`/api/users/${encoded}${query}`, { cache: "no-store" }),
          fetch(`/api/users/${encoded}/devices`, { cache: "no-store" }),
        ]);
        if (!profileResponse.ok) return;

        const data = (await profileResponse.json()) as ProfileData;
        const devices = devicesResponse.ok
          ? (((await devicesResponse.json()) as { devices?: ProfileDevice[] })
              .devices ?? [])
          : [];

        if (!cancelled) setState({ kind: "owner", data, devices });
      } catch {
        // Stay on the fallback: to anyone but a verified owner, this is a
        // profile that does not exist.
      }
    }

    load(username);
    return () => {
      cancelled = true;
    };
  }, [username, period]);

  if (state.kind !== "owner" || !username) {
    return <>{fallback}</>;
  }

  return (
    <>
      <div className={cn(CONTAINER, "pt-6")}>
        <Alert>
          <AlertTitle>Your profile is private</AlertTitle>
          <AlertDescription>
            Only you can see this page. Everyone else gets &ldquo;User Not
            Found&rdquo;, and you are left out of the leaderboard. Apps and
            badges can read it with a read token from{" "}
            <Link href="/settings" className="underline underline-offset-2">
              Settings
            </Link>
            .
          </AlertDescription>
        </Alert>
      </div>
      <ProfilePageClient
        // Keyed on the period so switching it remounts with fresh data rather
        // than holding selection state that belonged to the previous range.
        key={state.data.period ?? "all"}
        initialData={state.data}
        initialDevices={state.devices}
        username={state.data.user.username}
      />
    </>
  );
}
