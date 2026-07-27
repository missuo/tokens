import { randomBytes, createHash } from "crypto";

/**
 * Generate a cryptographically secure random string.
 */
export function generateRandomString(length: number): string {
  return randomBytes(length).toString("hex").slice(0, length);
}

/**
 * Generate a human-readable user code for device flow.
 * Format: XXXX-XXXX (uppercase alphanumeric, no ambiguous chars)
 */
export function generateUserCode(): string {
  // Exclude ambiguous characters: 0, O, I, L, 1
  const chars = "ABCDEFGHJKMNPQRSTUVWXYZ23456789";
  // 256 is not a multiple of 31, so a plain `byte % 31` would make the first
  // eight characters ~12.5% more likely. Reject anything at or above the
  // largest multiple of the alphabet size and draw again (~3% of bytes).
  const limit = 256 - (256 % chars.length);
  let code = "";

  for (let i = 0; i < 8; i++) {
    let byte = randomBytes(1)[0];
    while (byte >= limit) {
      byte = randomBytes(1)[0];
    }

    code += chars[byte % chars.length];
    if (i === 3) code += "-";
  }

  return code;
}

/**
 * Generate an API token with prefix.
 * Format: tt_<random>
 */
export function generateApiToken(): string {
  return `tt_${randomBytes(24).toString("hex")}`;
}

/**
 * Hash a token using SHA256 for secure storage comparison.
 */
export function hashToken(token: string): string {
  return createHash("sha256").update(token).digest("hex");
}

/**
 * Generate a device code (internal use, not shown to user).
 */
export function generateDeviceCode(): string {
  return randomBytes(16).toString("hex");
}
