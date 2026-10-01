import { getPublicProfileDevicesResponse } from "@/lib/publicProfileDevices";

// Who is asking changes the answer (private accounts, masked device names), so
// this can never be served from a per-path cache.
export const dynamic = "force-dynamic";

interface RouteParams {
  params: Promise<{ username: string }>;
}

export async function GET(request: Request, context: RouteParams) {
  return getPublicProfileDevicesResponse(request, context);
}
