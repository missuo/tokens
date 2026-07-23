"use client";

import { useState, useEffect, useId } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTheme } from "next-themes";
import { Popover, PopoverTrigger, PopoverContent } from "@heroui/react";
import { PersonIcon, GearIcon, SignOutIcon } from "@/components/ui/Icons";

interface User {
  id: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
}

function GitHubIcon({ size = 18 }: { size?: number }) {
  return (
    <svg aria-hidden="true" width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0C5.374 0 0 5.373 0 12C0 17.302 3.438 21.8 8.207 23.387C8.806 23.498 9 23.126 9 22.81V20.576C5.662 21.302 4.967 19.16 4.967 19.16C4.421 17.773 3.634 17.404 3.634 17.404C2.545 16.659 3.717 16.675 3.717 16.675C4.922 16.759 5.556 17.912 5.556 17.912C6.626 19.746 8.363 19.216 9.048 18.909C9.155 18.134 9.466 17.604 9.81 17.305C7.145 17 4.343 15.971 4.343 11.374C4.343 10.063 4.812 8.993 5.579 8.153C5.455 7.85 5.044 6.629 5.696 4.977C5.696 4.977 6.704 4.655 8.997 6.207C9.954 5.941 10.98 5.808 12 5.803C13.02 5.808 14.047 5.941 15.006 6.207C17.297 4.655 18.303 4.977 18.303 4.977C18.956 6.63 18.545 7.851 18.421 8.153C19.19 8.993 19.656 10.064 19.656 11.374C19.656 15.983 16.849 16.998 14.177 17.295C14.607 17.667 15 18.397 15 19.517V22.81C15 23.129 15.192 23.504 15.801 23.386C20.566 21.797 24 17.3 24 12C24 5.373 18.627 0 12 0Z" />
    </svg>
  );
}

const TESTFLIGHT_URL = "https://testflight.apple.com/join/NWmvqqTX";

function TestFlightIcon({ size = 20 }: { size?: number }) {
  const gradientId = useId();
  return (
    <svg width={size} height={size} viewBox="0 0 28 28" aria-hidden="true" className="shrink-0">
      <defs>
        <linearGradient id={gradientId} x1="14" y1="0" x2="14" y2="28" gradientUnits="userSpaceOnUse">
          <stop stopColor="#40BEFF" />
          <stop offset="1" stopColor="#0A6BFF" />
        </linearGradient>
      </defs>
      <rect width="28" height="28" rx="6.5" fill={`url(#${gradientId})`} />
      <g
        fill="#fff"
        className="transition-transform duration-700 ease-out group-hover:rotate-[360deg]"
        style={{ transformOrigin: "14px 14px" }}
      >
        <ellipse cx="14" cy="8" rx="2.6" ry="4.3" />
        <ellipse cx="14" cy="8" rx="2.6" ry="4.3" transform="rotate(120 14 14)" />
        <ellipse cx="14" cy="8" rx="2.6" ry="4.3" transform="rotate(240 14 14)" />
        <circle cx="14" cy="14" r="2" />
      </g>
    </svg>
  );
}

function SunIcon({ size = 18 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
    </svg>
  );
}

function MoonIcon({ size = 18 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79Z" />
    </svg>
  );
}

function BrandMark({ size = 28 }: { size?: number }) {
  return (
    // eslint-disable-next-line @next/next/no-img-element
    <img src="/brand-logo.png" alt="Tokens" width={size} height={size} className="shrink-0" />
  );
}

function ThemeToggle() {
  const { resolvedTheme, setTheme } = useTheme();
  const [mounted, setMounted] = useState(false);
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setMounted(true);
  }, []);

  const isDark = resolvedTheme === "dark";
  return (
    <button
      type="button"
      aria-label="Toggle color theme"
      onClick={() => setTheme(isDark ? "light" : "dark")}
      className="grid h-9 w-9 place-items-center rounded-lg border border-line bg-surface text-muted transition hover:border-foreground/20 hover:text-foreground"
    >
      {mounted && isDark ? <MoonIcon /> : <SunIcon />}
    </button>
  );
}

function avatarSrc(user: User) {
  return user.avatarUrl || `https://github.com/${user.username}.png`;
}

function UserMenu({ user, onSignOut }: { user: User; onSignOut: () => void }) {
  const [open, setOpen] = useState(false);
  return (
    <Popover isOpen={open} onOpenChange={setOpen}>
      <PopoverTrigger>
        <button
          aria-label={`User menu for ${user.username}`}
          className="flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-lg ring-1 ring-line transition hover:ring-foreground/25"
        >
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src={avatarSrc(user)} alt={user.username} width={36} height={36} className="h-full w-full object-cover" />
        </button>
      </PopoverTrigger>
      <PopoverContent placement="bottom end" className="w-56 p-0">
        <div className="border-b border-line px-3 py-3">
          <p className="truncate text-sm font-semibold text-foreground">{user.displayName || user.username}</p>
          <p className="truncate font-mono text-xs text-muted">@{user.username}</p>
        </div>
        <div className="flex flex-col py-1">
          <Link href={`/u/${user.username}`} onClick={() => setOpen(false)} className="flex items-center gap-2.5 px-3 py-2 text-sm text-foreground transition hover:bg-foreground/5">
            <PersonIcon /> Your Profile
          </Link>
          <Link href="/settings" onClick={() => setOpen(false)} className="flex items-center gap-2.5 px-3 py-2 text-sm text-foreground transition hover:bg-foreground/5">
            <GearIcon /> Settings
          </Link>
        </div>
        <div className="border-t border-line py-1">
          <button onClick={() => { setOpen(false); onSignOut(); }} className="flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm text-danger transition hover:bg-danger/10">
            <SignOutIcon /> Sign Out
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}

const NAV_LINKS = [
  { href: "/leaderboard", label: "Leaderboard", authOnly: false, match: (p: string) => p === "/leaderboard" },
  { href: "/profile", label: "Profile", authOnly: true, match: (p: string) => p === "/profile" || p.startsWith("/u/") },
] as const;

export function Navigation() {
  const pathname = usePathname();
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [mobileOpen, setMobileOpen] = useState(false);

  useEffect(() => {
    fetch("/api/auth/session")
      .then((res) => res.json())
      .then((data) => {
        setUser(data.user || null);
        setIsLoading(false);
      })
      .catch(() => setIsLoading(false));
  }, []);

  const signOut = async () => {
    await fetch("/api/auth/logout", { method: "POST" });
    setUser(null);
    window.location.href = "/leaderboard";
  };

  const navLinks = NAV_LINKS.filter((l) => !l.authOnly || user);
  const hrefFor = (link: (typeof NAV_LINKS)[number]) =>
    link.href === "/profile" && user ? `/u/${user.username}` : link.href;

  const isActive = (link: (typeof NAV_LINKS)[number]) => link.match(pathname);

  const linkClass = (active: boolean) =>
    `relative rounded-lg px-3 py-1.5 text-sm font-medium transition ${
      active ? "text-foreground" : "text-muted hover:text-foreground"
    }`;

  return (
    <header className="sticky top-0 z-50 border-b border-line bg-background/80 backdrop-blur-xl">
      <nav aria-label="Main navigation" className="mx-auto flex h-14 max-w-[1200px] items-center gap-2 px-4 sm:px-6">
        <Link href="/leaderboard" className="flex shrink-0 items-center gap-2" aria-label="Tokens home">
          <BrandMark />
          <span className="font-mono text-[15px] font-bold tracking-tight text-foreground">tokens</span>
        </Link>

        <div className="ml-2 hidden items-center gap-0.5 sm:flex">
          {navLinks.map((l) => {
            const active = isActive(l);
            return (
              <Link key={l.href} href={hrefFor(l)} className={linkClass(active)} aria-current={active ? "page" : undefined}>
                {l.label}
                {active && <span className="absolute inset-x-3 -bottom-[15px] h-0.5 rounded-full bg-accent" />}
              </Link>
            );
          })}
        </div>

        <div className="ml-auto flex items-center gap-2">
          <a
            href={TESTFLIGHT_URL}
            target="_blank"
            rel="noopener noreferrer"
            aria-label="Download the iOS app on TestFlight"
            className="group flex h-9 items-center gap-2 rounded-lg border border-line bg-surface px-1.5 text-sm font-medium text-foreground transition hover:border-foreground/20 sm:pr-3"
          >
            <TestFlightIcon />
            <span className="hidden leading-none sm:inline">
              iOS App
              <span className="ml-1.5 rounded-[4px] bg-foreground/8 px-1 py-0.5 align-middle font-mono text-[10px] font-semibold uppercase tracking-wide text-muted">
                beta
              </span>
            </span>
          </a>
          <ThemeToggle />
          {isLoading ? (
            <div className="h-9 w-9 animate-pulse rounded-lg bg-foreground/10" />
          ) : user ? (
            <UserMenu user={user} onSignOut={signOut} />
          ) : (
            <a
              href="/api/auth/github"
              className="flex h-9 items-center gap-2 rounded-lg bg-foreground px-3 text-sm font-semibold text-background transition hover:opacity-90"
            >
              <GitHubIcon />
              <span className="max-[420px]:hidden">Sign in</span>
            </a>
          )}

          <div className="sm:hidden">
            <Popover isOpen={mobileOpen} onOpenChange={setMobileOpen}>
              <PopoverTrigger>
                <button aria-label="Open menu" className="grid h-9 w-9 place-items-center rounded-lg border border-line bg-surface text-foreground">
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                    <path d="M3 12h18M3 6h18M3 18h18" />
                  </svg>
                </button>
              </PopoverTrigger>
              <PopoverContent placement="bottom end" className="w-48 p-1">
                {navLinks.map((l) => (
                  <Link key={l.href} href={hrefFor(l)} onClick={() => setMobileOpen(false)} className="block rounded-lg px-3 py-2 text-sm font-medium text-foreground transition hover:bg-foreground/5">
                    {l.label}
                  </Link>
                ))}
                <Link href="/local" onClick={() => setMobileOpen(false)} className="block rounded-lg px-3 py-2 text-sm font-medium text-foreground transition hover:bg-foreground/5">
                  Local Viewer
                </Link>
                <a
                  href={TESTFLIGHT_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  onClick={() => setMobileOpen(false)}
                  className="group mt-1 flex items-center gap-2.5 rounded-lg border-t border-line px-3 pb-2 pt-2.5 text-sm font-medium text-foreground transition hover:bg-foreground/5"
                >
                  <TestFlightIcon size={22} />
                  <span className="flex flex-col leading-tight">
                    iOS App
                    <span className="font-mono text-[10px] font-normal text-muted">Get it on TestFlight</span>
                  </span>
                </a>
              </PopoverContent>
            </Popover>
          </div>
        </div>
      </nav>
    </header>
  );
}
