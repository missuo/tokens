// Shared helpers for the per-device read APIs under
// /api/users/[username]/devices/...
//
// Why a dedicated module: the listing endpoint, the per-device detail
// endpoint, and the rename endpoint all need the same display-name fallback
// and update-time normalization. Keeping them here prevents a class of bugs
// where one endpoint accepts an empty string and another doesn't.

export const LEGACY_DEVICE_KEY = "legacy-default";

/** Fallback label used when `submitted_devices.display_name` is null/empty. */
export function deviceDisplayLabel(
  deviceKey: string,
  displayName: string | null | undefined
): string {
  const trimmed = displayName?.trim();
  if (trimmed) return trimmed;
  if (deviceKey === LEGACY_DEVICE_KEY) return "Legacy submissions";
  return "Unnamed device";
}

/** Coerce any drizzle timestamp / Date / string into a stable ISO string. */
export function toIsoString(value: Date | string | null | undefined): string | null {
  if (!value) return null;
  if (value instanceof Date) return value.toISOString();
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

/**
 * A device name as the public profile shows it. Device names are frequently a
 * hostname — `johns-macbook-pro`, `acme-build-07` — which carries a real name
 * or an employer, so only the first and last two characters survive. The
 * middle is a fixed run of asterisks rather than one per character, so the
 * mask does not give away the length either.
 *
 * The fallback labels ("Unnamed device", "Legacy submissions") are ours, not
 * the user's, and are passed through untouched.
 */
export function maskDeviceName(name: string): string {
  const chars = Array.from(name.trim());
  if (chars.length === 0) return "";
  if (chars.length <= 4) return `${chars[0]}***`;
  return `${chars.slice(0, 2).join("")}****${chars.slice(-2).join("")}`;
}

export function publicDeviceDisplayLabel(
  deviceKey: string,
  displayName: string | null | undefined
): string {
  return displayName?.trim()
    ? maskDeviceName(displayName)
    : deviceDisplayLabel(deviceKey, displayName);
}
