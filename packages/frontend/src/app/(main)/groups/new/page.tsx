import { redirect } from "next/navigation";
import { getSession } from "@/lib/auth/session";
import CreateGroupClient from "./CreateGroupClient";

export default async function NewGroupPage() {
  const session = await getSession();

  if (!session) {
    redirect("/api/auth/github?returnTo=/groups/new");
  }

  return (
    <main className="main-container" id="main-content">
      <CreateGroupClient />
    </main>
  );
}
