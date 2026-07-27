import { redirect } from "next/navigation";
import { getSession } from "@/lib/auth/session";

export default async function ProfilePage() {
  const session = await getSession();

  if (session) {
    redirect(`/u/${session.username}`);
  } else {
    // Comes back here after the handshake, which then forwards to /u/<name>.
    // Without returnTo the OAuth route falls back to /leaderboard and the user
    // never reaches the profile they asked for.
    redirect("/api/auth/github?returnTo=/profile");
  }
}
