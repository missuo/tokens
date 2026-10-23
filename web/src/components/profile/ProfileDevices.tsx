"use client";

import { formatRelativeTime } from "@/lib/format";
import { formatCurrency, formatNumber } from "@/lib/utils";
import {
  ListBody,
  ListCaption,
  ListCard,
  ListCell,
  ListHead,
  ListHeaderCell,
  ListPrimaryCell,
  ListRow,
  ListTable,
  NumericValue,
} from "./listStyles";
import { tw } from "@/lib/tw";
import { groupCloudDevices } from "@/lib/devices/cloud";
import { cn } from "@/lib/utils";

/** Public device usage shape returned by the profile devices route. */
export interface ProfileDevice {
  id: string;
  deviceKey: string;
  displayName: string;
  customName: string | null;
  createdAt: string | null;
  lastSubmittedAt: string | null;
  totalTokens: number;
  totalCost: number;
  inputTokens: number;
  outputTokens: number;
  activeDays: number;
  firstDay: string | null;
  lastDay: string | null;
  /** Cloud session devices only: active days across the provider's sessions. */
  cloudGroupActiveDays?: number;
}

export interface ProfileDevicesProps {
  devices: ProfileDevice[];
  className?: string;
}

const DevicesSection = tw(
  "section",
  "overflow-hidden rounded-xl border bg-card text-foreground"
);

const SectionHeading = tw(
  "h2",
  "m-0 border-b px-4 py-3.5 text-[0.9375rem] font-semibold leading-tight tracking-tight text-foreground"
);

const DevicesList = ({
  className,
  ...props
}: React.ComponentPropsWithoutRef<"div">) => (
  <ListCard {...props} className={cn("border-y-0", className)} />
);

const DeviceName = tw(
  "span",
  "block overflow-hidden text-ellipsis whitespace-nowrap text-foreground max-[639px]:whitespace-normal max-[639px]:[overflow-wrap:anywhere]"
);

const LastSubmitted = tw(
  "span",
  "mt-1 block text-xs font-normal leading-tight text-muted-foreground"
);
/** Compact per-device usage for public profiles. */
export function ProfileDevices({ devices, className }: ProfileDevicesProps) {
  if (devices.length === 0) return null;
  // Each cloud agent session is its own device server-side; show one row per
  // cloud provider instead of one per session.
  const rows = groupCloudDevices(devices);

  return (
    <DevicesSection
      className={className}
      aria-labelledby="profile-devices-heading"
    >
      <SectionHeading id="profile-devices-heading">
        Devices · all-time
      </SectionHeading>

      <DevicesList>
        <ListTable>
          <ListCaption>Usage by device</ListCaption>
          <ListHead>
            <tr>
              <ListHeaderCell $width="52%">Device</ListHeaderCell>
              <ListHeaderCell $width="19%" $align="right">
                Tokens
              </ListHeaderCell>
              <ListHeaderCell $width="16%" $align="right">
                Cost
              </ListHeaderCell>
              <ListHeaderCell $width="13%" $align="right">
                Active days
              </ListHeaderCell>
            </tr>
          </ListHead>
          <ListBody>
            {rows.map((device) => (
              <ListRow key={device.id}>
                <ListPrimaryCell scope="row">
                  <DeviceName>{device.displayName}</DeviceName>
                  <LastSubmitted>
                    {device.cloudSessions != null &&
                      `${device.cloudSessions.toLocaleString("en-US")} ${
                        device.cloudSessions === 1 ? "session" : "sessions"
                      } · `}
                    Last submitted{" "}
                    <time
                      dateTime={device.lastSubmittedAt ?? undefined}
                      suppressHydrationWarning
                    >
                      {formatRelativeTime(device.lastSubmittedAt)}
                    </time>
                  </LastSubmitted>
                </ListPrimaryCell>
                <ListCell data-label="Tokens" $align="right">
                  <NumericValue
                    title={device.totalTokens.toLocaleString("en-US")}
                  >
                    {formatNumber(device.totalTokens)}
                  </NumericValue>
                </ListCell>
                <ListCell data-label="Cost" $align="right">
                  <NumericValue $accent>
                    {formatCurrency(device.totalCost)}
                  </NumericValue>
                </ListCell>
                <ListCell data-label="Active days" $align="right">
                  <NumericValue>
                    {device.activeDays.toLocaleString("en-US")}
                  </NumericValue>
                </ListCell>
              </ListRow>
            ))}
          </ListBody>
        </ListTable>
      </DevicesList>
    </DevicesSection>
  );
}
