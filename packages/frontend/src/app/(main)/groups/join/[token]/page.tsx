import JoinGroupClient from "./JoinGroupClient";

export default async function JoinGroupPage({
  params,
}: {
  params: Promise<{ token: string }>;
}) {
  const { token } = await params;

  return (
    <div
      style={{
        minHeight: "100vh",
        display: "flex",
        flexDirection: "column",
        backgroundColor: "var(--color-bg-default)",
      }}
    >
      <main className="main-container">
        <JoinGroupClient token={token} />
      </main>
    </div>
  );
}
