"use client";

import { useState, useEffect, useCallback } from "react";
import { useRouter } from "nextjs-toploader/app";
import { KeyIcon } from "@/components/ui/Icons";
import { deviceDisplayLabel } from "@/lib/devices/shared";
import { cn, formatNumber, formatCurrency } from "@/lib/utils";
import { formatRelativeTime } from "@/lib/format";

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

// Subset of GET /api/users/[username]/devices we render here. That public
// endpoint already aggregates usage per device, so settings reuses it with
// the session user's username instead of adding a private listing route.
interface SettingsDevice {
  id: string;
  deviceKey: string;
  /** Resolved label (custom name or fallback) — what we render. */
  displayName: string;
  /** Raw user-set name (null = never renamed) — what we edit. */
  customName: string | null;
  lastSubmittedAt: string | null;
  totalTokens: number;
  totalCost: number;
  activeDays: number;
}

// Mirror the server-side RenameBodySchema in
// /api/settings/devices/[deviceId]/route.ts (varchar(120), no control chars).
const DEVICE_NAME_MAX_LENGTH = 120;
const DEVICE_NAME_CONTROL_CHARS = /\p{C}/u;

function validateDeviceName(name: string): string | null {
  if (name.length > DEVICE_NAME_MAX_LENGTH) {
    return `Device name must be ${DEVICE_NAME_MAX_LENGTH} characters or fewer`;
  }
  if (DEVICE_NAME_CONTROL_CHARS.test(name)) {
    return "Device name must not contain control characters";
  }
  return null;
}

// ============================================================================
// Shared UI primitives
// ============================================================================
//
// These were styled-components. `tw` keeps the same ergonomics — a named
// element with baked-in classes that still forwards every DOM prop — so the
// markup below is unchanged.
//
// Colours come from the shared palette, with two deliberate exceptions:
// --danger and --success are semantic colours carrying meaning, not neutrals
// that drifted, and shadcn's --destructive is a visibly softer red (#C84941
// against #B42318 in light), so folding them into it would restyle every
// destructive control on the page.

function tw<T extends keyof React.JSX.IntrinsicElements>(Tag: T, base: string) {
  const Component = ({
    className,
    ...props
  }: React.ComponentPropsWithoutRef<T>) => {
    const Element = Tag as React.ElementType;
    return <Element className={cn(base, className)} {...props} />;
  };
  Component.displayName = `tw(${String(Tag)})`;
  return Component;
}

const PageWrapper = tw(
  "div",
  "flex min-h-[calc(100dvh-56px)] flex-col bg-background text-foreground"
);

// Matches `CONTAINER` in components/layout/Container.tsx — every other route
// lays out at 1200px with the same gutters, and a narrower Settings page reads
// as a different site.
const MainContent = tw(
  "main",
  "mx-auto w-full max-w-[1200px] flex-1 px-4 pb-24 pt-10 sm:px-6 sm:pt-14"
);

const LoadingMain = tw("main", "flex flex-1 items-center justify-center");

// Mirrors PageHeader: 2xl/3xl semibold, not a one-off 1.75rem bold.
const Title = tw(
  "h1",
  "text-2xl font-semibold tracking-tight text-foreground sm:text-3xl"
);

// PageHeader's description: muted *foreground*, relaxed leading, and the same
// separator gap the other routes put between the header and the first section.
const Subtitle = tw(
  "p",
  "mb-7 mt-1.5 max-w-[72ch] text-sm leading-relaxed text-muted-foreground"
);

const Section = tw("section", "mb-6 rounded-xl border bg-card p-6");
const SectionTitle = tw("h2", "mb-4 text-lg font-semibold");
const ProfileWrapper = tw("div", "flex items-center gap-4");
const ProfileText = tw("p", "font-medium");
const SmallText = tw("p", "text-sm");
const CodeText = tw("code", "rounded px-1 py-0.5 text-xs");
const Description = tw("p", "mb-4 text-sm");
const FieldLabel = tw("label", "mb-2 block text-[13px] font-semibold");

const ActionRow = tw(
  "div",
  "mb-4 grid grid-cols-[1fr_auto] gap-3 max-[560px]:grid-cols-1"
);

const TextInput = tw(
  "input",
  "h-10 rounded-md border bg-background px-3 text-sm text-foreground"
);

const PrimaryButton = tw(
  "button",
  "h-10 cursor-pointer rounded-md border border-primary bg-primary px-3.5 text-sm font-semibold text-primary-foreground transition-opacity duration-150 disabled:cursor-not-allowed disabled:opacity-60"
);

const TokenReveal = tw(
  "div",
  "mb-4 rounded-md border border-[var(--success)] bg-[color-mix(in_srgb,var(--success)_8%,transparent)] p-3"
);

const TokenCodeRow = tw(
  "div",
  "mt-2 grid grid-cols-[minmax(0,1fr)_auto] gap-2 max-[560px]:grid-cols-1"
);

const TokenCode = tw(
  "code",
  "block overflow-x-auto whitespace-nowrap rounded-md bg-background px-3 py-2.5 text-[13px]"
);

const SecondaryButton = tw(
  "button",
  "h-[38px] cursor-pointer rounded-md border bg-background px-3 text-[13px] font-semibold text-foreground"
);

const ErrorText = tw("p", "-mt-1 mb-4 text-[13px] text-[var(--danger)]");
const EmptyState = tw("div", "py-8 text-center");
const EmptyIcon = tw("div", "mx-auto mb-3 opacity-50");
const EmptyText = tw("p", "mt-2 text-sm");
const TokenList = tw("div", "flex flex-col gap-3");

const TokenItem = tw(
  "div",
  "flex items-center justify-between rounded-xl border bg-muted p-4"
);

const TokenInfo = tw("div", "flex items-center gap-3");
const IconWrapper = tw("div", "text-muted-foreground");

const DeviceEditRow = tw(
  "div",
  "grid w-full grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2 max-[560px]:grid-cols-1"
);

const DangerButton = tw(
  "button",
  "cursor-pointer rounded-md border border-[var(--danger)] bg-transparent px-3 py-1 text-xs font-medium text-[var(--danger)] transition-all duration-150 hover:bg-[var(--danger-solid)] hover:text-white"
);

const InfoBanner = tw(
  "div",
  "rounded-md border bg-muted px-4 py-3 text-sm text-muted-foreground"
);

const AvatarImg = tw(
  "img",
  "flex-shrink-0 rounded-md object-cover ring-1 ring-border"
);

const TokenName = tw("p", "font-medium");

// ============================================================================
// Danger Zone
// ============================================================================

const DangerSection = tw(
  "section",
  "mb-6 rounded-xl border border-[color-mix(in_srgb,var(--danger)_40%,transparent)] bg-card p-6"
);

const DangerSectionTitle = tw(
  "h2",
  "mb-4 text-lg font-semibold text-[var(--danger)]"
);

const DangerActionRow = tw(
  "div",
  "flex items-center justify-between gap-4 py-4 [&:not(:last-child)]:border-b [&:not(:last-child)]:border-border"
);

const DangerActionInfo = tw("div", "min-w-0 flex-1");
const DangerActionTitle = tw("p", "mb-1 text-sm font-medium text-foreground");
const DangerActionDescription = tw("p", "text-[13px] text-muted-foreground");

const DangerActionButton = tw(
  "button",
  "flex-shrink-0 cursor-pointer rounded-md border border-[var(--danger)] bg-transparent px-4 py-1.5 text-[13px] font-medium text-[var(--danger)] transition-all duration-150 hover:bg-[var(--danger-solid)] hover:text-white"
);

// ============================================================================
// Confirmation modal
// ============================================================================

const ModalOverlay = tw(
  "div",
  "fixed inset-0 z-[1000] flex items-center justify-center bg-black/60 backdrop-blur-[4px]"
);

const ModalCard = tw(
  "div",
  "w-[calc(100%-32px)] max-w-[480px] rounded-2xl border bg-background p-6 shadow-[0_16px_48px_rgba(0,0,0,0.35)]"
);

const ModalTitle = tw("h3", "mb-3 text-base font-semibold text-[var(--danger)]");
const ModalBody = tw("p", "mb-5 text-sm leading-normal text-muted-foreground");

const ModalBulletList = tw(
  "ul",
  "mb-5 list-disc pl-5 text-sm leading-relaxed text-muted-foreground"
);

const ModalInput = tw(
  "input",
  "mb-4 box-border w-full rounded-md border bg-muted px-3 py-2 text-sm text-foreground outline-none focus:border-[var(--danger)] focus:shadow-[0_0_0_2px_color-mix(in_srgb,var(--danger)_20%,transparent)]"
);

const ModalActions = tw("div", "flex justify-end gap-2");

const CancelButton = tw(
  "button",
  "cursor-pointer rounded-md border bg-transparent px-4 py-1.5 text-[13px] font-medium text-foreground transition-all duration-150 hover:bg-muted"
);

const ConfirmDangerButton = ({
  $disabled,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"button"> & { $disabled?: boolean }) => (
  <button
    {...props}
    className={cn(
      "rounded-md border border-[var(--danger)] px-4 py-1.5 text-[13px] font-medium transition-all duration-150",
      $disabled
        ? "cursor-not-allowed bg-transparent text-[var(--danger)] opacity-50"
        : "cursor-pointer bg-[var(--danger-solid)] text-white hover:bg-[#8f1d14]",
      className
    )}
  />
);

const StepIndicator = tw("div", "mb-4 flex gap-1.5");

const StepDot = ({ $active }: { $active: boolean }) => (
  <div
    className={cn(
      "size-1.5 rounded-full transition-colors duration-150",
      $active ? "bg-[var(--danger)]" : "bg-border"
    )}
  />
);

// ============================================================================
// Confirmation modal component
// ============================================================================

type DangerAction = "delete-data" | "delete-account";

interface ConfirmationConfig {
  title: string;
  steps: Array<{
    body: React.ReactNode;
    confirmLabel: string;
  }>;
  typedConfirmation: string;
  onConfirm: () => Promise<void>;
}

const CONFIRMATION_CONFIGS: Record<DangerAction, ConfirmationConfig> = {
  "delete-data": {
    title: "Delete submitted data",
    steps: [
      {
        body: (
          <>
            <ModalBody>This will permanently remove all submitted usage data from your account:</ModalBody>
            <ModalBulletList>
              <li>Leaderboard entries</li>
              <li>Public profile stats</li>
              <li>Daily usage history</li>
            </ModalBulletList>
            <ModalBody style={{ marginBottom: 0 }}>
              Your account and API tokens will remain active. You can submit new data at any time.
            </ModalBody>
          </>
        ),
        confirmLabel: "I want to delete my data",
      },
      {
        body: (
          <ModalBody>
            This action <strong>cannot be undone</strong>. All your historical
            token usage and cost data will be permanently erased from the
            leaderboard and your public profile.
          </ModalBody>
        ),
        confirmLabel: "I understand, continue",
      },
    ],
    typedConfirmation: "delete my data",
    onConfirm: async () => {
      const res = await fetch("/api/settings/submitted-data", { method: "DELETE" });
      if (!res.ok) throw new Error("Failed to delete submitted data");
    },
  },
  "delete-account": {
    title: "Delete account",
    steps: [
      {
        body: (
          <>
            <ModalBody>This will permanently delete your entire account and all associated data:</ModalBody>
            <ModalBulletList>
              <li>User profile</li>
              <li>All submitted usage data</li>
              <li>Leaderboard entries</li>
              <li>API tokens and active sessions</li>
            </ModalBulletList>
            <ModalBody style={{ marginBottom: 0 }}>
              You will be signed out immediately. This cannot be reversed.
            </ModalBody>
          </>
        ),
        confirmLabel: "I want to delete my account",
      },
      {
        body: (
          <ModalBody>
            This action is <strong>permanent and irreversible</strong>. Your
            username will become available for others to register. All your data
            — submissions, tokens, sessions — will be wiped.
          </ModalBody>
        ),
        confirmLabel: "I understand, continue",
      },
    ],
    typedConfirmation: "delete my account",
    onConfirm: async () => {
      const res = await fetch("/api/settings/account", { method: "DELETE" });
      if (!res.ok) throw new Error("Failed to delete account");
    },
  },
};

function DangerConfirmationModal({
  action,
  onClose,
  onSuccess,
}: {
  action: DangerAction;
  onClose: () => void;
  onSuccess: () => void;
}) {
  const config = CONFIRMATION_CONFIGS[action];
  const totalSteps = config.steps.length + 1; // +1 for typed confirmation step
  const [step, setStep] = useState(0);
  const [typedValue, setTypedValue] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const isTypedStep = step === config.steps.length;
  const typedMatch = typedValue.toLowerCase().trim() === config.typedConfirmation;

  const handleConfirm = useCallback(async () => {
    if (isTypedStep) {
      if (!typedMatch || isSubmitting) return;
      setIsSubmitting(true);
      try {
        await config.onConfirm();
        onSuccess();
      } catch {
        alert(`Failed to ${action === "delete-data" ? "delete submitted data" : "delete account"}. Please try again.`);
        setIsSubmitting(false);
      }
    } else {
      setStep((s) => s + 1);
    }
  }, [isTypedStep, typedMatch, isSubmitting, config, onSuccess, action]);

  return (
    <ModalOverlay onClick={isSubmitting ? undefined : onClose}>
      <ModalCard onClick={(e) => e.stopPropagation()}>
        <StepIndicator>
          {["step-1", "step-2", "step-3"].slice(0, totalSteps).map((id, i) => (
            <StepDot key={id} $active={i <= step} />
          ))}
        </StepIndicator>

        <ModalTitle>⚠ {config.title}</ModalTitle>

        {isTypedStep ? (
          <>
            <ModalBody>
              Type <strong>{config.typedConfirmation}</strong> to confirm:
            </ModalBody>
            <ModalInput
              autoFocus
              value={typedValue}
              onChange={(e) => setTypedValue(e.target.value)}
              placeholder={config.typedConfirmation}
              onKeyDown={(e) => {
                if (e.key === "Enter" && typedMatch && !isSubmitting) {
                  handleConfirm();
                }
              }}
            />
          </>
        ) : (
          config.steps[step].body
        )}

        <ModalActions>
          <CancelButton onClick={onClose} disabled={isSubmitting}>
            Cancel
          </CancelButton>
          <ConfirmDangerButton
            $disabled={isTypedStep ? !typedMatch : false}
            disabled={(isTypedStep && !typedMatch) || isSubmitting}
            onClick={handleConfirm}
          >
            {isSubmitting
              ? "Deleting..."
              : isTypedStep
                ? config.steps[config.steps.length - 1].confirmLabel.replace("I understand, continue", "Delete permanently")
                : config.steps[step].confirmLabel}
          </ConfirmDangerButton>
        </ModalActions>
      </ModalCard>
    </ModalOverlay>
  );
}

function apiTokenListItem(token: CreatedApiToken): ApiToken {
  return {
    id: token.id,
    name: token.name,
    createdAt: token.createdAt,
    lastUsedAt: token.lastUsedAt,
  };
}

function prependApiToken(tokens: ApiToken[], token: ApiToken): ApiToken[] {
  return [token, ...tokens.filter((item) => item.id !== token.id)];
}

function mergeApiTokenList(
  serverTokens: ApiToken[],
  currentTokens: ApiToken[]
): ApiToken[] {
  const serverTokenIds = new Set(serverTokens.map((token) => token.id));
  const localTokens = currentTokens.filter(
    (token) => !serverTokenIds.has(token.id)
  );
  return [...localTokens, ...serverTokens];
}

async function fetchApiTokens(): Promise<ApiToken[]> {
  const tokensResponse = await fetch("/api/settings/tokens");
  const tokensData = await tokensResponse.json();
  return Array.isArray(tokensData.tokens) ? tokensData.tokens : [];
}

async function fetchDevices(username: string): Promise<SettingsDevice[]> {
  const devicesResponse = await fetch(
    `/api/users/${encodeURIComponent(username)}/devices`
  );
  if (!devicesResponse.ok) return [];
  const devicesData = await devicesResponse.json();
  return Array.isArray(devicesData.devices) ? devicesData.devices : [];
}

// ============================================================================
// Main component
// ============================================================================

export default function SettingsClient() {
  const router = useRouter();
  const [user, setUser] = useState<User | null>(null);
  const [tokens, setTokens] = useState<ApiToken[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [dangerAction, setDangerAction] = useState<DangerAction | null>(null);
  const [tokenName, setTokenName] = useState("CI token");
  const [createdToken, setCreatedToken] = useState<CreatedApiToken | null>(null);
  const [isCreatingToken, setIsCreatingToken] = useState(false);
  const [createTokenError, setCreateTokenError] = useState<string | null>(null);
  const [devices, setDevices] = useState<SettingsDevice[]>([]);
  const [editingDeviceId, setEditingDeviceId] = useState<string | null>(null);
  const [editingDeviceName, setEditingDeviceName] = useState("");
  const [isSavingDeviceName, setIsSavingDeviceName] = useState(false);
  const [deviceError, setDeviceError] = useState<string | null>(null);

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

        const [loadedTokens, loadedDevices] = await Promise.all([
          fetchApiTokens().catch(() => []),
          fetchDevices(sessionData.user.username).catch(
            () => [] as SettingsDevice[]
          ),
        ]);

        if (!cancelled) {
          setUser(sessionData.user);
          setTokens((current) => mergeApiTokenList(loadedTokens, current));
          setDevices(loadedDevices);
          setIsLoading(false);
        }
      } catch {
        if (!cancelled) {
          router.push("/leaderboard");
        }
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
      const response = await fetch(`/api/settings/tokens/${tokenId}`, {
        method: "DELETE",
      });

      if (response.ok) {
        setTokens(tokens.filter((t) => t.id !== tokenId));
      }
    } catch {
      alert("Failed to revoke token");
    }
  };

  const handleDangerSuccess = useCallback(() => {
    if (dangerAction === "delete-account") {
      // Account is gone — redirect to home.
      window.location.href = "/";
    } else {
      // Data deleted — close modal and stay.
      setDangerAction(null);
      alert("Submitted data has been deleted.");
    }
  }, [dangerAction]);

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
      if (!response.ok || !data.token) {
        throw new Error(data.error || "Failed to create token");
      }

      setCreatedToken(data.token);
      setTokens((current) =>
        prependApiToken(current, apiTokenListItem(data.token))
      );
    } catch (error) {
      setCreateTokenError(error instanceof Error ? error.message : "Failed to create token");
    } finally {
      setIsCreatingToken(false);
    }
  };

  const startEditingDevice = (device: SettingsDevice) => {
    setEditingDeviceId(device.id);
    setDeviceError(null);
    // Pre-fill from the raw custom name, not the resolved display label, so
    // an unnamed device starts empty and a custom name that happens to equal
    // the fallback label ("Unnamed device" etc.) is preserved.
    setEditingDeviceName(device.customName ?? "");
  };

  const cancelEditingDevice = () => {
    setEditingDeviceId(null);
    setEditingDeviceName("");
    setDeviceError(null);
  };

  const handleSaveDeviceName = async (device: SettingsDevice) => {
    const trimmed = editingDeviceName.trim();
    const validationError = validateDeviceName(trimmed);
    if (validationError) {
      setDeviceError(validationError);
      return;
    }

    setIsSavingDeviceName(true);
    setDeviceError(null);

    try {
      const response = await fetch(`/api/settings/devices/${device.id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        // Empty input clears the custom name; server stores null and the
        // display label falls back via deviceDisplayLabel.
        body: JSON.stringify({ name: trimmed === "" ? null : trimmed }),
      });

      const data = await response.json();
      if (!response.ok || !data.device) {
        throw new Error(data.error || "Failed to rename device");
      }

      setDevices((current) =>
        current.map((item) =>
          item.id === device.id
            ? {
                ...item,
                displayName: deviceDisplayLabel(
                  data.device.deviceKey,
                  data.device.displayName
                ),
                customName: data.device.displayName ?? null,
              }
            : item
        )
      );
      setEditingDeviceId(null);
      setEditingDeviceName("");
    } catch (error) {
      setDeviceError(
        error instanceof Error ? error.message : "Failed to rename device"
      );
    } finally {
      setIsSavingDeviceName(false);
    }
  };

  const handleCopyCreatedToken = async () => {
    if (!createdToken) return;
    await navigator.clipboard.writeText(createdToken.token);
    // The raw token is shown once and only once. After the user has copied
    // it we drop it from React state so it no longer lives in the component
    // tree (and thus no longer in any DevTools / extension snapshot of it).
    // Users who haven't copied yet still have the value in the reveal panel
    // until they navigate away.
    setCreatedToken(null);
  };

  if (isLoading) {
    return (
      <PageWrapper style={{ backgroundColor: "var(--background)" }}>
        <LoadingMain>
          <div style={{ color: "var(--muted-foreground)" }}>Loading...</div>
        </LoadingMain>
      </PageWrapper>
    );
  }

  if (!user) {
    return null;
  }

  return (
    <PageWrapper style={{ backgroundColor: "var(--background)" }}>
      <MainContent>
        <Title style={{ color: "var(--foreground)" }}>
          Settings
        </Title>
        <Subtitle>Manage your profile, API tokens, devices, and submitted data.</Subtitle>

        <Section>
          <SectionTitle style={{ color: "var(--foreground)" }}>
            Profile
          </SectionTitle>
          <ProfileWrapper>
            <AvatarImg
              src={user.avatarUrl || `https://github.com/${user.username}.png`}
              alt={user.username}
              width={64}
              height={64}
            />
            <div>
              <ProfileText style={{ color: "var(--foreground)" }}>
                {user.displayName || user.username}
              </ProfileText>
              <SmallText style={{ color: "var(--muted-foreground)" }}>
                @{user.username}
              </SmallText>
              {user.email && (
                <SmallText style={{ color: "var(--muted-foreground)" }}>
                  {user.email}
                </SmallText>
              )}
            </div>
          </ProfileWrapper>
          <InfoBanner style={{ marginTop: 16 }}>
            Profile information is synced from GitHub and cannot be edited here.
          </InfoBanner>
        </Section>

        <Section>
          <SectionTitle style={{ color: "var(--foreground)" }}>
            API Tokens
          </SectionTitle>
          <Description style={{ color: "var(--muted-foreground)" }}>
            Create a token for CI or use one generated by{" "}
            <CodeText
              style={{ backgroundColor: "var(--muted)" }}
            >
              tokens login
            </CodeText>{" "}
            from the CLI.
          </Description>

          <FieldLabel
            htmlFor="token-name"
            style={{ color: "var(--foreground)" }}
          >
            Token name
          </FieldLabel>
          <ActionRow>
            <TextInput
              id="token-name"
              value={tokenName}
              onChange={(event) => setTokenName(event.target.value)}
              maxLength={100}
            />
            <PrimaryButton
              type="button"
              disabled={isCreatingToken}
              onClick={handleCreateToken}
            >
              {isCreatingToken ? "Creating..." : "Create token"}
            </PrimaryButton>
          </ActionRow>

          {createTokenError && <ErrorText>{createTokenError}</ErrorText>}

          {createdToken && (
            <TokenReveal>
              <SmallText style={{ color: "var(--foreground)", fontWeight: 600 }}>
                Copy this token now. It will not be shown again.
              </SmallText>
              <TokenCodeRow>
                <TokenCode style={{ color: "var(--foreground)" }}>
                  {createdToken.token}
                </TokenCode>
                <SecondaryButton type="button" onClick={handleCopyCreatedToken}>
                  Copy
                </SecondaryButton>
              </TokenCodeRow>
            </TokenReveal>
          )}

          {tokens.length === 0 ? (
            <EmptyState style={{ color: "var(--muted-foreground)" }}>
              <EmptyIcon>
                <KeyIcon size={32} />
              </EmptyIcon>
              <p>No API tokens yet.</p>
              <EmptyText>
                Create one here or run{" "}
                <CodeText
                  style={{ backgroundColor: "var(--muted)" }}
                >
                  tokens login
                </CodeText>{" "}
                from the CLI.
              </EmptyText>
            </EmptyState>
          ) : (
            <TokenList>
              {tokens.map((token) => (
                <TokenItem key={token.id}>
                  <TokenInfo>
                    <IconWrapper>
                      <KeyIcon size={20} />
                    </IconWrapper>
                    <div>
                      <TokenName style={{ color: "var(--foreground)" }}>
                        {token.name}
                      </TokenName>
                      <SmallText style={{ color: "var(--muted-foreground)" }}>
                        Created {new Date(token.createdAt).toLocaleDateString()}
                        {token.lastUsedAt && (
                          <> - Last used {new Date(token.lastUsedAt).toLocaleDateString()}</>
                        )}
                      </SmallText>
                    </div>
                  </TokenInfo>
                  <DangerButton
                    onClick={() => handleRevokeToken(token.id)}
                  >
                    Revoke
                  </DangerButton>
                </TokenItem>
              ))}
            </TokenList>
          )}
        </Section>

        <Section>
          <SectionTitle style={{ color: "var(--foreground)" }}>
            Devices
          </SectionTitle>
          <Description style={{ color: "var(--muted-foreground)" }}>
            Machines that have submitted usage data. Rename a device to tell
            your machines apart — the name is shown on your public profile.
          </Description>

          {deviceError && <ErrorText>{deviceError}</ErrorText>}

          {devices.length === 0 ? (
            <EmptyState style={{ color: "var(--muted-foreground)" }}>
              <p>No devices yet.</p>
              <EmptyText>
                Run{" "}
                <CodeText
                  style={{ backgroundColor: "var(--muted)" }}
                >
                  bunx tokens-cli submit
                </CodeText>{" "}
                to register this machine.
              </EmptyText>
            </EmptyState>
          ) : (
            <TokenList>
              {devices.map((device) => (
                <TokenItem key={device.id}>
                  {editingDeviceId === device.id ? (
                    <DeviceEditRow>
                      <TextInput
                        aria-label="Device name"
                        value={editingDeviceName}
                        maxLength={DEVICE_NAME_MAX_LENGTH}
                        placeholder="Device name (empty to reset)"
                        autoFocus
                        disabled={isSavingDeviceName}
                        onChange={(event) =>
                          setEditingDeviceName(event.target.value)
                        }
                        onKeyDown={(event) => {
                          if (event.key === "Enter") {
                            event.preventDefault();
                            handleSaveDeviceName(device);
                          } else if (event.key === "Escape") {
                            cancelEditingDevice();
                          }
                        }}
                      />
                      <PrimaryButton
                        type="button"
                        disabled={isSavingDeviceName}
                        onClick={() => handleSaveDeviceName(device)}
                      >
                        {isSavingDeviceName ? "Saving..." : "Save"}
                      </PrimaryButton>
                      <SecondaryButton
                        type="button"
                        disabled={isSavingDeviceName}
                        onClick={cancelEditingDevice}
                      >
                        Cancel
                      </SecondaryButton>
                    </DeviceEditRow>
                  ) : (
                    <>
                      <TokenInfo>
                        <div>
                          <TokenName style={{ color: "var(--foreground)" }}>
                            {device.displayName}
                          </TokenName>
                          <SmallText style={{ color: "var(--muted-foreground)" }}>
                            {formatNumber(device.totalTokens)} tokens
                            {" · "}
                            {formatCurrency(device.totalCost)}
                            {" · "}
                            {device.activeDays} active{" "}
                            {device.activeDays === 1 ? "day" : "days"}
                            {" · "}
                            Last submit {formatRelativeTime(device.lastSubmittedAt)}
                          </SmallText>
                        </div>
                      </TokenInfo>
                      <SecondaryButton
                        type="button"
                        onClick={() => startEditingDevice(device)}
                      >
                        Rename
                      </SecondaryButton>
                    </>
                  )}
                </TokenItem>
              ))}
            </TokenList>
          )}
        </Section>

        <DangerSection>
          <DangerSectionTitle>
            Danger Zone
          </DangerSectionTitle>

          <DangerActionRow>
            <DangerActionInfo>
              <DangerActionTitle>Delete submitted data</DangerActionTitle>
              <DangerActionDescription>
                Remove all leaderboard entries, profile stats, and usage
                history. Your account and API tokens stay active.
              </DangerActionDescription>
            </DangerActionInfo>
            <DangerActionButton onClick={() => setDangerAction("delete-data")}>
              Delete data
            </DangerActionButton>
          </DangerActionRow>

          <DangerActionRow>
            <DangerActionInfo>
              <DangerActionTitle>Delete account</DangerActionTitle>
              <DangerActionDescription>
                Permanently delete your account and all associated data. This
                action is irreversible.
              </DangerActionDescription>
            </DangerActionInfo>
            <DangerActionButton onClick={() => setDangerAction("delete-account")}>
              Delete account
            </DangerActionButton>
          </DangerActionRow>
        </DangerSection>

      </MainContent>

      {dangerAction && (
        <DangerConfirmationModal
          action={dangerAction}
          onClose={() => setDangerAction(null)}
          onSuccess={handleDangerSuccess}
        />
      )}
    </PageWrapper>
  );
}
