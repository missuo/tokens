import Link from "next/link";

export default function ProfileNotFound() {
  return (
    <div className="flex min-h-screen flex-col bg-background pt-16">
      <main className="mx-auto w-full max-w-[800px] flex-1 px-6 py-10">
        <div className="py-20 text-center">
          <h1 className="mb-2 text-2xl font-bold text-foreground">User Not Found</h1>
          <p className="mb-6 text-muted">This user doesn&apos;t exist or hasn&apos;t submitted any data yet.</p>
          <Link href="/leaderboard" className="inline-flex items-center rounded-lg bg-accent px-4 py-2 font-medium text-accent-foreground transition hover:opacity-90">
            Back to Leaderboard
          </Link>
        </div>
      </main>
    </div>
  );
}
