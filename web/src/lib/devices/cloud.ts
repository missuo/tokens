// Devices that are cloud agent sessions rather than machines.
//
// `tokens cloud submit` (cli/tokens-cli/src/cloud.rs) runs inside the throwaway
// container of a Claude Code or Codex cloud session and submits as a device
// keyed to the session: `cloud_<provider>_<32 hex>`, the hex being a hash of
// the session id. One session is one device, so a session resumed in a new
// container lands on the same row (and is merged, not added twice), while
// separate sessions add up.
//
// This module is the server's side of that contract: it recognises the key,
// says which client such a device may report, and gives it a fixed label so no
// name chosen by the container (a hostname, say) is ever stored or shown.

export interface CloudDeviceProvider {
  /** The only client a device of this provider may submit. */
  client: string;
  /** Label shown in place of a device name. */
  label: string;
}

/** Keyed by the provider segment of the device key. Mirrors cloud.rs. */
export const CLOUD_DEVICE_PROVIDERS: Readonly<Record<string, CloudDeviceProvider>> = {
  claude: { client: "claude", label: "Claude Code cloud" },
  codex: { client: "codex", label: "Codex cloud" },
};

export const CLOUD_DEVICE_KEY_PREFIX = "cloud_";

/**
 * Shape of a well-formed cloud device key. Also valid as a PostgreSQL regex
 * (`~`), so SQL can select exactly the keys `parseCloudDeviceKey` accepts the
 * shape of.
 */
export const CLOUD_DEVICE_KEY_PATTERN = "^cloud_([a-z]+)_([0-9a-f]{32})$";

const CLOUD_DEVICE_KEY_RE = new RegExp(CLOUD_DEVICE_KEY_PATTERN);

export interface CloudDeviceInfo extends CloudDeviceProvider {
  provider: string;
}

/** True for any key in the reserved `cloud_` namespace, well-formed or not. */
export function isReservedCloudDeviceKey(deviceKey: string): boolean {
  return deviceKey.startsWith(CLOUD_DEVICE_KEY_PREFIX);
}

/** The provider of a well-formed cloud device key, or null. */
export function parseCloudDeviceKey(deviceKey: string): CloudDeviceInfo | null {
  const match = CLOUD_DEVICE_KEY_RE.exec(deviceKey);
  if (!match) return null;
  // Own keys only: `cloud_constructor_…` must not resolve through the prototype.
  if (!Object.hasOwn(CLOUD_DEVICE_PROVIDERS, match[1])) return null;
  return { provider: match[1], ...CLOUD_DEVICE_PROVIDERS[match[1]] };
}

interface GroupableDevice {
  id: string;
  deviceKey: string;
  displayName: string;
  lastSubmittedAt: string | null;
  totalTokens: number;
  totalCost: number;
  activeDays: number;
  /**
   * Distinct active days across all of this provider's cloud sessions,
   * computed by the server. Optional: absent from older/cached responses.
   */
  cloudGroupActiveDays?: number;
}

export type GroupedDevice<T extends GroupableDevice> = T & {
  /** Number of cloud sessions folded into this row; absent for real devices. */
  cloudSessions?: number;
};

/**
 * Fold every cloud-session device of one provider into a single row for
 * display, so a profile shows "Claude Code cloud · 37 sessions" instead of 37
 * rows. Active days are a union across sessions, which the per-device totals
 * cannot reconstruct, so the server sends that union as `cloudGroupActiveDays`
 * on every session row and the folded row uses it. A response without it
 * (older or cached) falls back to the largest single session's count, a lower
 * bound, rather than a sum that would overstate it.
 *
 * Order is preserved: the folded row takes the position of its most recently
 * submitted session.
 */
export function groupCloudDevices<T extends GroupableDevice>(devices: T[]): GroupedDevice<T>[] {
  const result: GroupedDevice<T>[] = [];
  const groups = new Map<string, GroupedDevice<T>>();

  for (const device of devices) {
    const cloud = parseCloudDeviceKey(device.deviceKey);
    if (!cloud) {
      result.push(device);
      continue;
    }

    const existing = groups.get(cloud.provider);
    if (!existing) {
      const row: GroupedDevice<T> = {
        ...device,
        id: `cloud:${cloud.provider}`,
        displayName: cloud.label,
        cloudSessions: 1,
        activeDays: device.cloudGroupActiveDays ?? device.activeDays,
      };
      groups.set(cloud.provider, row);
      result.push(row);
      continue;
    }

    existing.cloudSessions = (existing.cloudSessions ?? 1) + 1;
    existing.totalTokens += device.totalTokens;
    existing.totalCost += device.totalCost;
    existing.activeDays =
      device.cloudGroupActiveDays ?? Math.max(existing.activeDays, device.activeDays);
    if (
      device.lastSubmittedAt &&
      (!existing.lastSubmittedAt || device.lastSubmittedAt > existing.lastSubmittedAt)
    ) {
      existing.lastSubmittedAt = device.lastSubmittedAt;
    }
  }

  return result;
}
