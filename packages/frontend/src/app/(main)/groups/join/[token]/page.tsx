import JoinGroupClient from "./JoinGroupClient";

export default async function JoinGroupPage({
  params,
}: {
  params: Promise<{ token: string }>;
}) {
  const { token } = await params;

  return (
    <main className="main-container" id="main-content">
      <JoinGroupClient token={token} />
    </main>
  );
}
