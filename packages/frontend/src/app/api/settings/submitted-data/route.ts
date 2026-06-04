import { revalidatePath, revalidateTag } from "next/cache";
import { NextResponse } from "next/server";
import { eq, sql, desc } from "drizzle-orm";
import { authenticatePersonalToken } from "@/lib/auth/personalTokens";
import { getSessionFromRequest } from "@/lib/auth/requestSession";
import { db, submissions, submittedDevices, dailyBreakdown } from "@/lib/db";
import { normalizeUsernameCacheKey, revalidateUsernamePaths } from "@/lib/db/usernameLookup";
import { getBearerToken } from "../../../../lib/auth/bearerToken";
import { revalidateUserGroupLeaderboards } from "@/lib/groups/cache";

async function resolveUser(request: Request): Promise<{ id: string; username: string } | null> {
  const token = getBearerToken(request.headers.get("Authorization"));
  if (token) {
    const result = await authenticatePersonalToken(token, { touchLastUsedAt: false });
    if (result.status === "valid") {
      return { id: result.userId, username: result.username };
    }
    return null;
  }

  const session = await getSessionFromRequest(request);
  if (session) {
    return { id: session.id, username: session.username };
  }
  return null;
}

export async function GET(request: Request) {
  try {
    const user = await resolveUser(request);
    if (!user) {
      return NextResponse.json({ error: "Not authenticated" }, { status: 401 });
    }

    const [submission] = await db
      .select({
        totalTokens: submissions.totalTokens,
        totalCost: submissions.totalCost,
        inputTokens: submissions.inputTokens,
        outputTokens: submissions.outputTokens,
        cacheReadTokens: submissions.cacheReadTokens,
        cacheCreationTokens: submissions.cacheCreationTokens,
        reasoningTokens: submissions.reasoningTokens,
        dateStart: submissions.dateStart,
        dateEnd: submissions.dateEnd,
        sourcesUsed: submissions.sourcesUsed,
        modelsUsed: submissions.modelsUsed,
        cliVersion: submissions.cliVersion,
        submitCount: submissions.submitCount,
        sessionCount: submissions.sessionCount,
        totalActiveTimeMs: submissions.totalActiveTimeMs,
        firstSubmittedAt: submissions.createdAt,
        lastSubmittedAt: submissions.updatedAt,
      })
      .from(submissions)
      .where(eq(submissions.userId, user.id))
      .limit(1);

    // Per-device records: each submitted device with its aggregated
    // contribution (tokens, cost, active days) from the daily breakdown.
    const deviceRows = await db
      .select({
        id: submittedDevices.id,
        displayName: submittedDevices.displayName,
        createdAt: submittedDevices.createdAt,
        lastSubmittedAt: submittedDevices.lastSubmittedAt,
        tokens: sql<number>`COALESCE(SUM(${dailyBreakdown.tokens}), 0)`,
        cost: sql<number>`COALESCE(SUM(${dailyBreakdown.cost}), 0)`,
        activeDays: sql<number>`COUNT(DISTINCT ${dailyBreakdown.date})`,
      })
      .from(submittedDevices)
      .leftJoin(dailyBreakdown, eq(dailyBreakdown.submittedDeviceId, submittedDevices.id))
      .where(eq(submittedDevices.userId, user.id))
      .groupBy(submittedDevices.id)
      .orderBy(desc(submittedDevices.lastSubmittedAt));

    const devices = deviceRows.map((d) => ({
      id: d.id,
      displayName: d.displayName,
      createdAt: d.createdAt,
      lastSubmittedAt: d.lastSubmittedAt,
      tokens: Number(d.tokens) || 0,
      cost: Number(d.cost) || 0,
      activeDays: Number(d.activeDays) || 0,
    }));

    return NextResponse.json({
      submission: submission
        ? {
            totalTokens: Number(submission.totalTokens) || 0,
            totalCost: Number(submission.totalCost) || 0,
            inputTokens: Number(submission.inputTokens) || 0,
            outputTokens: Number(submission.outputTokens) || 0,
            cacheReadTokens: Number(submission.cacheReadTokens) || 0,
            cacheCreationTokens: Number(submission.cacheCreationTokens) || 0,
            reasoningTokens: Number(submission.reasoningTokens) || 0,
            dateStart: submission.dateStart,
            dateEnd: submission.dateEnd,
            sourcesUsed: submission.sourcesUsed ?? [],
            modelsUsed: submission.modelsUsed ?? [],
            cliVersion: submission.cliVersion,
            submitCount: submission.submitCount ?? 0,
            sessionCount: submission.sessionCount,
            totalActiveTimeMs: submission.totalActiveTimeMs,
            firstSubmittedAt: submission.firstSubmittedAt,
            lastSubmittedAt: submission.lastSubmittedAt,
          }
        : null,
      devices,
    });
  } catch (error) {
    console.error("Submitted data fetch error:", error);
    return NextResponse.json({ error: "Failed to load submitted usage data" }, { status: 500 });
  }
}

export async function DELETE(request: Request) {
  try {
    const user = await resolveUser(request);
    if (!user) {
      return NextResponse.json({ error: "Not authenticated" }, { status: 401 });
    }

    const deletedRows = await db.transaction(async (tx) => {
      const deleted = await tx
        .delete(submissions)
        .where(eq(submissions.userId, user.id))
        .returning({ id: submissions.id });

      await tx
        .delete(submittedDevices)
        .where(eq(submittedDevices.userId, user.id));

      return deleted;
    });

    const usernameCacheKey = normalizeUsernameCacheKey(user.username);
    try {
      revalidateTag("leaderboard", "max");
      revalidateTag(`user:${usernameCacheKey}`, "max");
      revalidateTag("user-rank", "max");
      revalidateTag(`user-rank:${usernameCacheKey}`, "max");
      revalidateTag(`embed-user:${usernameCacheKey}`, "max");
      revalidateTag(`embed-user:${usernameCacheKey}:tokens`, "max");
      revalidateTag(`embed-user:${usernameCacheKey}:cost`, "max");
    } catch (cacheError) {
      console.error("Public cache invalidation failed after deletion:", cacheError);
    }

    try {
      await revalidateUserGroupLeaderboards(user.id);
    } catch (cacheError) {
      console.error("Group cache invalidation failed after deletion:", cacheError);
    }

    try {
      revalidatePath("/leaderboard");
      revalidatePath("/profile");
      revalidateUsernamePaths(user.username);
    } catch (cacheError) {
      console.error("Path revalidation failed after deletion:", cacheError);
    }

    return NextResponse.json({
      success: true,
      deleted: deletedRows.length > 0,
      deletedSubmissions: deletedRows.length,
    });
  } catch (error) {
    console.error("Submitted data delete error:", error);
    return NextResponse.json(
      { error: "Failed to delete submitted usage data" },
      { status: 500 }
    );
  }
}
