"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTheme } from "next-themes";
import { LogOutIcon, MenuIcon, MoonIcon, SettingsIcon, SunIcon, UserIcon } from "lucide-react";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import { CONTAINER } from "@/components/layout/Container";

interface User {
  id: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
}

const NAV_LINKS = [
  { href: "/leaderboard", label: "Leaderboard", authOnly: false, match: (p: string) => p === "/leaderboard" },
  { href: "/shame", label: "Hall of Shame", authOnly: false, match: (p: string) => p === "/shame" },
  { href: "/docs", label: "Docs", authOnly: false, match: (p: string) => p.startsWith("/docs") },
  { href: "/profile", label: "Profile", authOnly: true, match: (p: string) => p === "/profile" || p.startsWith("/u/") },
] as const;

/**
 * The blue tile, so the corner of the page and the browser tab show the same
 * thing. Colours are literal rather than themed: this is the brand tile, and
 * it has to read identically against a light header, a dark header and the
 * favicon beside it.
 */
function TokensMark() {
  return (
    <svg viewBox="0 0 64 64" width={22} height={22} aria-hidden="true" className="shrink-0">
      <rect width={64} height={64} rx={14} fill="#2F6FDB" />
      <g stroke="#FFFFFF" strokeWidth={5} strokeLinecap="round" fill="none">
        <path d="M14 15h36" />
        <path d="M32 15v34" />
        <path d="M22 27h20" opacity={0.75} />
        <path d="M24.5 37h15" opacity={0.5} />
        <path d="M27 47h10" opacity={0.3} />
      </g>
    </svg>
  );
}

function GitHubIcon({ className }: { className?: string }) {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="currentColor" className={className}>
      <path d="M12 0C5.374 0 0 5.373 0 12C0 17.302 3.438 21.8 8.207 23.387C8.806 23.498 9 23.126 9 22.81V20.576C5.662 21.302 4.967 19.16 4.967 19.16C4.421 17.773 3.634 17.404 3.634 17.404C2.545 16.659 3.717 16.675 3.717 16.675C4.922 16.759 5.556 17.912 5.556 17.912C6.626 19.746 8.363 19.216 9.048 18.909C9.155 18.134 9.466 17.604 9.81 17.305C7.145 17 4.343 15.971 4.343 11.374C4.343 10.063 4.812 8.993 5.579 8.153C5.455 7.85 5.044 6.629 5.696 4.977C5.696 4.977 6.704 4.655 8.997 6.207C9.954 5.941 10.98 5.808 12 5.803C13.02 5.808 14.047 5.941 15.006 6.207C17.297 4.655 18.303 4.977 18.303 4.977C18.956 6.63 18.545 7.851 18.421 8.153C19.19 8.993 19.656 10.064 19.656 11.374C19.656 15.983 16.849 16.998 14.177 17.295C14.607 17.667 15 18.397 15 19.517V22.81C15 23.129 15.192 23.504 15.801 23.386C20.566 21.797 24 17.3 24 12C24 5.373 18.627 0 12 0Z" />
    </svg>
  );
}

/**
 * Theme toggle. Rendering is deferred until mount because the resolved theme
 * is unknown during SSR, and swapping the icon on hydration would otherwise
 * flash the wrong one.
 */
function ThemeToggle() {
  const { resolvedTheme, setTheme } = useTheme();
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setMounted(true);
  }, []);

  const isDark = resolvedTheme === "dark";

  if (!mounted) {
    return <Skeleton className="size-8 rounded-md" />;
  }

  return (
    <Button
      variant="ghost"
      size="icon"
      className="size-8"
      aria-label={isDark ? "Switch to light theme" : "Switch to dark theme"}
      onClick={() => setTheme(isDark ? "light" : "dark")}
    >
      {isDark ? <MoonIcon /> : <SunIcon />}
    </Button>
  );
}

function avatarFor(user: User) {
  return user.avatarUrl || `https://github.com/${user.username}.png`;
}

function UserMenu({ user, onSignOut }: { user: User; onSignOut: () => void }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <button
            aria-label={`Account menu for ${user.username}`}
            className="rounded-full ring-offset-background transition-opacity hover:opacity-85 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
          />
        }
      >
        <Avatar className="size-8">
          <AvatarImage src={avatarFor(user)} alt="" />
          <AvatarFallback className="text-[10px]">
            {user.username.slice(0, 2).toUpperCase()}
          </AvatarFallback>
        </Avatar>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-52">
        <div className="flex flex-col gap-0.5 px-2 py-1.5">
          <span className="truncate text-sm font-medium">
            {user.displayName || user.username}
          </span>
          <span className="truncate text-xs text-muted-foreground">@{user.username}</span>
        </div>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem render={<Link href={`/u/${user.username}`} />}>
            <UserIcon />
            Your profile
          </DropdownMenuItem>
          <DropdownMenuItem render={<Link href="/settings" />}>
            <SettingsIcon />
            Settings
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem variant="destructive" onClick={onSignOut}>
            <LogOutIcon />
            Sign out
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function Navigation() {
  const pathname = usePathname();
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  // A failed session request is not a sign-out. Holding the skeleton keeps the
  // header from flipping an authenticated user to a "Sign in" button — which,
  // if clicked, would send them back through OAuth for nothing.
  const [sessionFailed, setSessionFailed] = useState(false);

  // Retry rather than latch. Holding the skeleton on the first failure was
  // meant to avoid flipping an authenticated user to "Sign in", but with no
  // retry it never came back: one bad response left the header showing an
  // avatar placeholder forever, with the auth-only links hidden and no way to
  // sign in at all — including for signed-out visitors. A deploy takes this
  // service through roughly a minute of 502s, so that was a routine state, not
  // an edge case. After the retries are spent, fall through to the signed-out
  // header: a wrongly-shown "Sign in" costs one OAuth round trip, while a
  // permanent skeleton costs a manual reload the user has no reason to guess.
  useEffect(() => {
    let cancelled = false;
    const delays = [800, 2000, 5000];

    const attempt = async (tries = 0): Promise<void> => {
      try {
        const res = await fetch("/api/auth/session");
        if (!res.ok) throw new Error("Failed to load session");
        const data = await res.json();
        if (cancelled) return;
        setUser(data.user || null);
        setSessionFailed(false);
        setIsLoading(false);
      } catch {
        if (cancelled) return;
        if (tries < delays.length) {
          setSessionFailed(true);
          setTimeout(() => {
            if (!cancelled) void attempt(tries + 1);
          }, delays[tries]);
          return;
        }
        setSessionFailed(false);
        setUser(null);
        setIsLoading(false);
      }
    };

    void attempt();
    return () => {
      cancelled = true;
    };
  }, []);

  const signOut = async () => {
    await fetch("/api/auth/logout", { method: "POST" });
    setUser(null);
    window.location.href = "/leaderboard";
  };

  // Sign-in comes back to where it was clicked rather than dropping everyone on
  // /leaderboard, which is what the route falls back to when returnTo is absent.
  // Pathname only: the query string would need useSearchParams, which forces a
  // Suspense boundary around the whole header.
  const returnTo = pathname.startsWith("/") ? pathname : "/leaderboard";

  const links = NAV_LINKS.filter((l) => !l.authOnly || user);
  const hrefFor = (link: (typeof NAV_LINKS)[number]) =>
    link.href === "/profile" && user ? `/u/${user.username}` : link.href;

  return (
    <header className="sticky top-0 z-50 border-b bg-background/85 backdrop-blur-xl">
      <nav
        aria-label="Main navigation"
        className={cn(CONTAINER, "flex h-14 items-center gap-2")}
      >
        <Link href="/leaderboard" className="flex shrink-0 items-center gap-2" aria-label="Tokens home">
          {/* The mark paints with currentColor, so one file covers both themes. */}
          <TokensMark />
          <span className="text-[15px] font-semibold tracking-tight">Tokens</span>
        </Link>

        <div className="ml-3 hidden items-center gap-0.5 sm:flex">
          {links.map((link) => {
            const active = link.match(pathname);
            return (
              <Link
                key={link.href}
                href={hrefFor(link)}
                aria-current={active ? "page" : undefined}
                className={cn(
                  "rounded-md px-2.5 py-1.5 text-sm transition-colors",
                  active
                    ? "font-medium text-foreground"
                    : "text-muted-foreground hover:text-foreground"
                )}
              >
                {link.label}
              </Link>
            );
          })}
        </div>

        <div className="ml-auto flex items-center gap-1.5">
          <Button
            variant="ghost"
            size="icon"
            className="hidden size-8 sm:inline-flex"
            aria-label="Tokens on GitHub"
            render={
              <a
                href="https://github.com/missuo/tokens"
                target="_blank"
                rel="noopener noreferrer"
              />
            }
          >
            <GitHubIcon className="size-4" />
          </Button>

          <ThemeToggle />

          {isLoading || sessionFailed ? (
            <Skeleton className="size-8 rounded-full" />
          ) : user ? (
            <UserMenu user={user} onSignOut={signOut} />
          ) : (
            <Button
              size="sm"
              className="h-8"
              render={
                <a
                  href={`/api/auth/github?returnTo=${encodeURIComponent(returnTo)}`}
                />
              }
            >
              <GitHubIcon className="size-3.5" data-icon="inline-start" />
              Sign in
            </Button>
          )}

          {/* Mobile navigation lives behind a menu; the links do not fit. */}
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <Button variant="ghost" size="icon" className="size-8 sm:hidden" aria-label="Open menu" />
              }
            >
              <MenuIcon />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              <DropdownMenuGroup>
                {links.map((link) => (
                  <DropdownMenuItem key={link.href} render={<Link href={hrefFor(link)} />}>
                    {link.label}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </nav>
    </header>
  );
}
