import { NextResponse } from "next/server";
import { lt } from "drizzle-orm";
import { db, deviceCodes } from "@/lib/db";
import { generateDeviceCode, generateUserCode } from "@/lib/auth/utils";

const DEVICE_CODE_EXPIRY_SECONDS = 900; // 15 minutes
const POLL_INTERVAL_SECONDS = 5;
// Matches device_codes.device_name varchar(100): over-long names are a 400
// here rather than a driver-level 500 at insert time.
const DEVICE_NAME_MAX_LENGTH = 100;

export async function POST(request: Request) {
  try {
    const body = await request.json().catch(() => ({}));

    const rawDeviceName = body.deviceName;
    if (rawDeviceName != null && typeof rawDeviceName !== "string") {
      return NextResponse.json(
        { error: "Invalid device name" },
        { status: 400 }
      );
    }
    const trimmedDeviceName =
      typeof rawDeviceName === "string" ? rawDeviceName.trim() : "";
    if (trimmedDeviceName.length > DEVICE_NAME_MAX_LENGTH) {
      return NextResponse.json(
        { error: `Device name must be at most ${DEVICE_NAME_MAX_LENGTH} characters` },
        { status: 400 }
      );
    }
    const deviceName = trimmedDeviceName || "Unknown Device";

    // This endpoint is unauthenticated and writes a row per call, and expiry
    // is only ever a WHERE predicate — nothing else deletes these. Sweep on
    // create so the table is bounded by the number of in-flight logins.
    // Best-effort: a failed sweep must not block a login.
    try {
      await db.delete(deviceCodes).where(lt(deviceCodes.expiresAt, new Date()));
    } catch (error) {
      console.error("Expired device code cleanup error:", error);
    }

    // Generate codes
    const deviceCode = generateDeviceCode();
    const userCode = generateUserCode();
    const expiresAt = new Date(Date.now() + DEVICE_CODE_EXPIRY_SECONDS * 1000);

    // Store in database
    await db.insert(deviceCodes).values({
      deviceCode,
      userCode,
      deviceName,
      expiresAt,
    });

    const baseUrl = process.env.NEXT_PUBLIC_URL || "http://localhost:3000";

    return NextResponse.json({
      deviceCode,
      userCode,
      verificationUrl: `${baseUrl}/device`,
      expiresIn: DEVICE_CODE_EXPIRY_SECONDS,
      interval: POLL_INTERVAL_SECONDS,
    });
  } catch (error) {
    console.error("Device code generation error:", error);
    return NextResponse.json(
      { error: "Failed to generate device code" },
      { status: 500 }
    );
  }
}
