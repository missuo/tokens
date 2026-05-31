"use client";

import { useState, useEffect } from "react";
import { useRouter } from "nextjs-toploader/app";
import { Button } from "@heroui/react";
import { KeyIcon, CopyIcon, CheckIcon } from "@/components/ui/Icons";
import { Panel, PageHeader } from "@/components/ui/primitives";

interface User {
  id: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
  email: string | null;
}

interface ApiToken {
  id: string;
  name: string;
  createdAt: string;
  lastUsedAt: string | null;
}

interface CreatedApiToken extends ApiToken {
  token: string;
}

function apiTokenListItem(token: CreatedApiToken): ApiToken {
  return { id: token.id, name: token.name, createdAt: token.createdAt, lastUsedAt: token.lastUsedAt };
}

function prependApiToken(tokens: ApiToken[], token: ApiToken): ApiToken[] {
  return [token, ...tokens.filter((item) => item.id !== token.id)];
}

function mergeApiTokenList(serverTokens: ApiToken[], currentTokens: ApiToken[]): ApiToken[] {
  const serverTokenIds = new Set(serverTokens.map((token) => token.id));
  const localTokens = currentTokens.filter((token) => !serverTokenIds.has(token.id));
  return [...localTokens, ...serverTokens];
}

async function fetchApiTokens(): Promise<ApiToken[]> {
  const tokensResponse = await fetch("/api/settings/tokens");
  const tokensData = await tokensResponse.json();
  return Array.isArray(tokensData.tokens) ? tokensData.tokens : [];
}

export default function SettingsClient() {
  const router = useRouter();
  const [user, setUser] = useState<User | null>(null);
  const [tokens, setTokens] = useState<ApiToken[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [tokenName, setTokenName] = useState("CI token");
  const [createdToken, setCreatedToken] = useState<CreatedApiToken | null>(null);
  const [isCreatingToken, setIsCreatingToken] = useState(false);
  const [createTokenError, setCreateTokenError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function loadSettings() {
      try {
        const sessionResponse = await fetch("/api/auth/session");
        const sessionData = await sessionResponse.json();
        if (cancelled) return;
        if (!sessionData.user) {
          router.push("/api/auth/github?returnTo=/settings");
          return;
        }
        const loadedTokens = await fetchApiTokens().catch(() => []);
        if (!cancelled) {
          setUser(sessionData.user);
          setTokens((current) => mergeApiTokenList(loadedTokens, current));
          setIsLoading(false);
        }
      } catch {
        if (!cancelled) router.push("/leaderboard");
      }
    }
    loadSettings();
    return () => {
      cancelled = true;
    };
  }, [router]);

  const handleRevokeToken = async (tokenId: string) => {
    if (!confirm("Are you sure you want to revoke this token?")) return;
    try {
      const response = await fetch(`/api/settings/tokens/${tokenId}`, { method: "DELETE" });
      if (response.ok) setTokens(tokens.filter((t) => t.id !== tokenId));
    } catch {
      alert("Failed to revoke token");
    }
  };

  const handleCreateToken = async () => {
    setIsCreatingToken(true);
    setCreateTokenError(null);
    try {
      const response = await fetch("/api/settings/tokens", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: tokenName }),
      });
      const data = await response.json();
      if (!response.ok || !data.token) throw new Error(data.error || "Failed to create token");
      setCreatedToken(data.token);
      setTokens((current) => prependApiToken(current, apiTokenListItem(data.token)));
    } catch (error) {
      setCreateTokenError(error instanceof Error ? error.message : "Failed to create token");
    } finally {
      setIsCreatingToken(false);
    }
  };

  const handleCopyCreatedToken = async () => {
    if (!createdToken) return;
    await navigator.clipboard.writeText(createdToken.token);
    setCopied(true);
    // The raw token is shown once. Drop it from state shortly after copy so it
    // no longer lives in the component tree / any devtools snapshot.
    setTimeout(() => {
      setCreatedToken(null);
      setCopied(false);
    }, 1200);
  };

  if (isLoading) {
    return (
      <div className="flex min-h-screen flex-col bg-background">
        <main className="flex flex-1 items-center justify-center">
          <div className="text-sm text-muted">Loading...</div>
        </main>
      </div>
    );
  }

  if (!user) return null;

  return (
    <div className="flex min-h-screen flex-col bg-background">
      <main className="mx-auto w-full max-w-3xl flex-1 px-4 py-8 sm:px-6 sm:py-10">
        <PageHeader title="Settings" subtitle="Manage your profile and personal API tokens." />

        <Panel className="mt-6 p-4 sm:p-6">
          <h2 className="text-base font-semibold text-foreground">Profile</h2>
          <div className="mt-4 flex items-center gap-4">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img
              src={user.avatarUrl || `https://github.com/${user.username}.png`}
              alt={user.username}
              width={64}
              height={64}
              className="h-16 w-16 shrink-0 rounded-lg border border-line object-cover"
            />
            <div className="min-w-0">
              <p className="truncate font-semibold text-foreground">{user.displayName || user.username}</p>
              <p className="truncate font-mono text-sm text-muted">@{user.username}</p>
              {user.email && <p className="truncate font-mono text-sm text-muted">{user.email}</p>}
            </div>
          </div>
          <div className="mt-4 rounded-lg border border-line bg-surface-secondary px-4 py-3 text-sm text-muted">
            Profile information is synced from GitHub and cannot be edited here.
          </div>
        </Panel>

        <Panel className="mt-6 p-4 sm:p-6">
          <h2 className="text-base font-semibold text-foreground">API Tokens</h2>
          <p className="mt-2 text-sm text-muted">
            Create a token for CI or use one generated by{" "}
            <code className="rounded-md bg-surface-secondary px-1.5 py-0.5 font-mono text-xs text-foreground">tokens login</code> from the CLI.
          </p>

          <div className="mt-5">
            <label
              htmlFor="token-name"
              className="mb-2 block text-[11px] font-semibold tracking-wider text-muted uppercase"
            >
              Token name
            </label>
            <div className="flex flex-col gap-2 sm:flex-row">
              <input
                id="token-name"
                value={tokenName}
                onChange={(e) => setTokenName(e.target.value)}
                maxLength={100}
                className="min-w-0 flex-1 rounded-lg border border-line bg-surface-secondary px-3 py-2 text-sm text-foreground outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/25"
              />
              <Button
                isDisabled={isCreatingToken}
                onPress={handleCreateToken}
                className="h-10 shrink-0 rounded-lg bg-accent px-4 text-sm font-semibold text-accent-foreground transition hover:opacity-90 disabled:opacity-60"
              >
                {isCreatingToken ? "Creating..." : "Create token"}
              </Button>
            </div>
          </div>

          {createTokenError && <p className="mt-2 text-[13px] text-danger">{createTokenError}</p>}

          {createdToken && (
            <div className="mt-4 rounded-lg border border-success/30 bg-success/10 p-3">
              <p className="text-sm font-semibold text-foreground">Copy this token now. It will not be shown again.</p>
              <div className="mt-2 flex flex-col gap-2 sm:flex-row">
                <code className="block min-w-0 flex-1 overflow-x-auto rounded-md border border-line bg-surface-secondary px-3 py-2.5 font-mono text-[13px] whitespace-nowrap text-foreground">
                  {createdToken.token}
                </code>
                <Button
                  onPress={handleCopyCreatedToken}
                  variant="ghost"
                  className="h-9 shrink-0 gap-1.5 rounded-lg border border-line bg-surface px-3 text-sm font-medium text-foreground transition hover:border-foreground/20 hover:bg-surface-secondary"
                >
                  {copied ? <CheckIcon size={16} /> : <CopyIcon size={16} />}
                  <span>{copied ? "Copied" : "Copy"}</span>
                </Button>
              </div>
            </div>
          )}

          {tokens.length === 0 ? (
            <div className="mt-5 rounded-lg border border-dashed border-line py-10 text-center text-muted">
              <div className="mx-auto mb-3 w-fit opacity-50">
                <KeyIcon size={32} />
              </div>
              <p className="text-sm font-medium text-foreground">No API tokens yet.</p>
              <p className="mt-1 text-sm">
                Create one here or run <code className="rounded-md bg-surface-secondary px-1.5 py-0.5 font-mono text-xs text-foreground">tokens login</code> from the CLI.
              </p>
            </div>
          ) : (
            <div className="mt-5 divide-y divide-line overflow-hidden rounded-lg border border-line">
              {tokens.map((token) => (
                <div
                  key={token.id}
                  className="flex flex-wrap items-center justify-between gap-3 bg-surface-secondary p-4"
                >
                  <div className="flex min-w-0 items-center gap-3">
                    <span className="shrink-0 text-muted">
                      <KeyIcon size={20} />
                    </span>
                    <div className="min-w-0">
                      <p className="truncate font-medium text-foreground">{token.name}</p>
                      <p className="text-xs text-muted">
                        Created{" "}
                        <span className="font-mono tabular-nums">{new Date(token.createdAt).toLocaleDateString()}</span>
                        {token.lastUsedAt && (
                          <>
                            {" · "}Last used{" "}
                            <span className="font-mono tabular-nums">{new Date(token.lastUsedAt).toLocaleDateString()}</span>
                          </>
                        )}
                      </p>
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    onPress={() => handleRevokeToken(token.id)}
                    className="h-9 shrink-0 rounded-lg border border-line bg-surface px-3 text-sm font-medium text-foreground transition hover:border-danger/40 hover:text-danger"
                  >
                    Revoke
                  </Button>
                </div>
              ))}
            </div>
          )}
        </Panel>
      </main>
    </div>
  );
}
