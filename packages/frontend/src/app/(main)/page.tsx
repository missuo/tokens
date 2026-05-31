import { redirect } from "next/navigation";

// The marketing/About landing page was removed. The leaderboard is now the home.
export default function HomePage() {
  redirect("/leaderboard");
}
