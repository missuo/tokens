import { redirect } from "next/navigation";

// Keep the product UI focused: the leaderboard is the Tokens home page.
export default function HomePage() {
  redirect("/leaderboard");
}
