import { getSession, getSessionFromHeader, type SessionUser } from "./session";

const MUTATING_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);

interface GetSessionFromRequestOptions {
  allowAuthorizationHeader?: boolean;
}

function getAllowedOrigins(): string[] {
  const env = process.env.CSRF_ALLOWED_ORIGINS;
  const origins = env
    ? env.split(",").map((o) => o.trim()).filter(Boolean)
    : ["https://tokens.ci", "http://localhost:3000"];

  // Self-hosted deployments already set NEXT_PUBLIC_URL for OAuth redirects;
  // the deployment's own origin is always a legitimate request source, so
  // include it whether or not CSRF_ALLOWED_ORIGINS is configured.
  const publicUrl = process.env.NEXT_PUBLIC_URL;
  if (publicUrl) {
    try {
      const url = new URL(publicUrl);
      // Only http(s) URLs have a real origin. Anything else (mailto:,
      // file:, ...) yields the opaque origin "null", and allowlisting the
      // literal string "null" would accept Origin: null requests from
      // sandboxed iframes - a CSRF hole.
      if (url.protocol === "http:" || url.protocol === "https:") {
        if (!origins.includes(url.origin)) {
          origins.push(url.origin);
        }
      }
    } catch {
      // Malformed NEXT_PUBLIC_URL; ignore and rely on the explicit list.
    }
  }

  return origins;
}

/**
 * The CSRF Origin gate on its own, without a session lookup.
 *
 * `getSessionFromRequest` collapses "forged origin" and "no valid session"
 * into the same `null`, which is right for routes that need a user but wrong
 * for logout: clearing a cookie is not privileged, and refusing to do it when
 * the session has already expired or the account was banned strands a live
 * cookie in the browser for the rest of its 30-day life.
 */
export function hasAllowedOrigin(request: Request): boolean {
  if (!MUTATING_METHODS.has(request.method)) {
    return true;
  }
  const origin = request.headers.get("Origin");
  return Boolean(origin && getAllowedOrigins().includes(origin));
}

export async function getSessionFromRequest(
  request: Request,
  options: GetSessionFromRequestOptions = {}
): Promise<SessionUser | null> {
  const authHeader = request.headers.get("Authorization");

  if (authHeader && options.allowAuthorizationHeader !== false) {
    return getSessionFromHeader(request);
  }

  if (MUTATING_METHODS.has(request.method)) {
    // Cookie-authenticated mutations must carry an Origin header that
    // matches the allowlist. A missing Origin header is also rejected:
    // modern browsers always set Origin on cross-origin mutating
    // requests, so a missing value typically means a non-browser client
    // that should be using a Bearer token instead.
    const origin = request.headers.get("Origin");
    const allowed = getAllowedOrigins();
    if (!origin || !allowed.includes(origin)) {
      return null;
    }
  }

  return getSession();
}
