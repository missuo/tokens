import { getPublicProfileResponse } from "@/lib/publicProfileData";

// Private accounts answer differently per caller (owner, read token, anyone
// else), so this can never be served from a per-path cache.
export const dynamic = "force-dynamic";

interface RouteParams {
  params: Promise<{ username: string }>;
}

export async function GET(request: Request, context: RouteParams) {
  return getPublicProfileResponse(request, context);
}
