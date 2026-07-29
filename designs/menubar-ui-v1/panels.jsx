/* 5 Menu Bar panel variants — full expanded content (no max-height clip) */

const PERIODS = ["Today", "7d", "30d", "All"];
const PANEL_W = 400;

function MenuBarChrome({ theme, title, children, accent }) {
  const dark = theme === "dark";
  return (
    <div
      className="mb-chrome"
      style={{
        width: 480,
        padding: "24px 24px 28px",
        background: dark
          ? "linear-gradient(160deg,#1a1b22 0%,#0e0f14 60%,#16141c 100%)"
          : "linear-gradient(160deg,#dfe6f2 0%,#c8d2e4 45%,#b8c4da 100%)",
        borderRadius: 16,
        boxSizing: "border-box",
        display: "inline-block",
      }}
    >
      <div
        style={{
          height: 28,
          display: "flex",
          alignItems: "center",
          justifyContent: "flex-end",
          gap: 10,
          marginBottom: 10,
          paddingRight: 4,
          color: dark ? "rgba(255,255,255,.85)" : "rgba(0,0,0,.75)",
          font: "500 12px/1 -apple-system,BlinkMacSystemFont,system-ui,sans-serif",
        }}
      >
        <span style={{ opacity: 0.55 }}>Mon 26</span>
        <span style={{ opacity: 0.55 }}>9:41</span>
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 5,
            padding: "3px 8px",
            borderRadius: 6,
            background: dark ? "rgba(255,255,255,.1)" : "rgba(0,0,0,.08)",
            fontVariantNumeric: "tabular-nums",
            color: accent || (dark ? "#fff" : "#111"),
          }}
        >
          <span
            aria-hidden
            style={{
              width: 12,
              height: 12,
              borderRadius: 3,
              background: accent || (dark ? "#8ab4ff" : "#007aff"),
              opacity: 0.9,
            }}
          />
          {title}
        </span>
      </div>
      <div style={{ display: "flex", justifyContent: "center" }}>{children}</div>
    </div>
  );
}

function ShareBar({ share, color, height = 5, track, radius = 99 }) {
  return (
    <div style={{ height, borderRadius: radius, background: track, overflow: "hidden" }}>
      <div
        style={{
          width: `${Math.max(2, Math.min(100, share * 100))}%`,
          height: "100%",
          borderRadius: radius,
          background: color,
        }}
      />
    </div>
  );
}

/* ───────── 01 Native Glass ───────── */
function Panel01Native({ theme }) {
  const dark = theme === "dark";
  const t = {
    panel: dark ? "rgba(40,40,44,.78)" : "rgba(255,255,255,.78)",
    text: dark ? "rgba(255,255,255,.92)" : "rgba(0,0,0,.88)",
    secondary: dark ? "rgba(255,255,255,.55)" : "rgba(0,0,0,.48)",
    divider: dark ? "rgba(255,255,255,.1)" : "rgba(0,0,0,.08)",
    card: dark ? "rgba(255,255,255,.06)" : "rgba(255,255,255,.55)",
    accent: dark ? "#0a84ff" : "#007aff",
    track: dark ? "rgba(255,255,255,.12)" : "rgba(0,0,0,.08)",
    font: '-apple-system,BlinkMacSystemFont,"SF Pro Text",system-ui,sans-serif',
  };
  return (
    <MenuBarChrome theme={theme} title={MOCK.menuBarTitle} accent={t.accent}>
      <div
        data-screen-label="01-native"
        style={{
          width: PANEL_W,
          fontFamily: t.font,
          color: t.text,
          background: t.panel,
          backdropFilter: "blur(40px) saturate(1.4)",
          WebkitBackdropFilter: "blur(40px) saturate(1.4)",
          borderRadius: 12,
          border: `0.5px solid ${dark ? "rgba(255,255,255,.18)" : "rgba(255,255,255,.7)"}`,
          boxShadow: dark
            ? "0 18px 50px rgba(0,0,0,.45), 0 0 0 0.5px rgba(0,0,0,.4)"
            : "0 18px 50px rgba(0,0,0,.18), 0 0 0 0.5px rgba(0,0,0,.06)",
        }}
      >
        <div style={{ padding: "14px 16px 10px" }}>
          <div style={{ fontSize: 15, fontWeight: 600, letterSpacing: "-0.01em" }}>Tokens</div>
        </div>
        <div style={{ height: 1, background: t.divider }} />

        <div style={{ padding: "12px 16px" }}>
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(4,1fr)",
              gap: 4,
              background: dark ? "rgba(255,255,255,.08)" : "rgba(0,0,0,.05)",
              borderRadius: 8,
              padding: 3,
            }}
          >
            {PERIODS.map((p) => {
              const on = p === "7d";
              return (
                <div
                  key={p}
                  style={{
                    textAlign: "center",
                    fontSize: 12,
                    fontWeight: on ? 600 : 500,
                    padding: "6px 0",
                    borderRadius: 6,
                    background: on ? (dark ? "rgba(255,255,255,.16)" : "#fff") : "transparent",
                    boxShadow: on ? (dark ? "none" : "0 1px 2px rgba(0,0,0,.08)") : "none",
                    color: on ? t.text : t.secondary,
                  }}
                >
                  {p}
                </div>
              );
            })}
          </div>
        </div>

        <div style={{ padding: "0 16px 4px" }}>
          <div style={{ background: t.card, borderRadius: 10, padding: 14, marginBottom: 14 }}>
            <div style={{ display: "flex", gap: 8 }}>
              {[
                ["Tokens", fmtTokens(MOCK.summary.totalTokens)],
                ["Cost", fmtCost(MOCK.summary.totalCost)],
                ["Msgs", String(MOCK.summary.messages)],
              ].map(([l, v]) => (
                <div key={l} style={{ flex: 1 }}>
                  <div style={{ fontSize: 11, color: t.secondary, marginBottom: 4 }}>{l}</div>
                  <div style={{ fontSize: 20, fontWeight: 600, fontVariantNumeric: "tabular-nums", letterSpacing: "-0.02em" }}>{v}</div>
                </div>
              ))}
            </div>
            <div style={{ marginTop: 8, fontSize: 11, color: t.secondary }}>
              {MOCK.dateRange.start} → {MOCK.dateRange.end}
            </div>
          </div>

          <div style={{ fontSize: 11, fontWeight: 600, color: t.secondary, marginBottom: 8 }}>Token breakdown</div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 8, marginBottom: 16 }}>
            {MOCK.breakdown.map((b) => (
              <div key={b.key}>
                <div style={{ fontSize: 11, color: t.secondary }}>{b.label}</div>
                <div style={{ fontSize: 13, fontWeight: 500, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(b.value)}</div>
              </div>
            ))}
          </div>

          <div style={{ fontSize: 11, fontWeight: 600, color: t.secondary, marginBottom: 8 }}>By client</div>
          {MOCK.clients.map((c) => (
            <div key={c.name} style={{ marginBottom: 12 }}>
              <div style={{ display: "flex", alignItems: "baseline", gap: 8, marginBottom: 4 }}>
                <div style={{ fontSize: 13, fontWeight: 500, flex: 1 }}>{c.name}</div>
                <div style={{ fontSize: 13, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(c.tokens)}</div>
                <div style={{ fontSize: 11, color: t.secondary, width: 52, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{fmtCost(c.cost)}</div>
              </div>
              <ShareBar share={c.share} color={t.accent} track={t.track} />
              <div style={{ fontSize: 11, color: t.secondary, marginTop: 3 }}>{fmtPct(c.share)}</div>
            </div>
          ))}

          <div style={{ fontSize: 11, fontWeight: 600, color: t.secondary, margin: "4px 0 8px" }}>By model</div>
          {MOCK.models.map((m) => (
            <div key={m.name} style={{ marginBottom: 12 }}>
              <div style={{ display: "flex", alignItems: "baseline", gap: 8, marginBottom: 4 }}>
                <div style={{ fontSize: 13, fontWeight: 500, flex: 1 }}>{m.name}</div>
                <div style={{ fontSize: 13, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(m.tokens)}</div>
                <div style={{ fontSize: 11, color: t.secondary, width: 52, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{fmtCost(m.cost)}</div>
              </div>
              <ShareBar share={m.share} color={t.accent} track={t.track} />
              <div style={{ fontSize: 11, color: t.secondary, marginTop: 3 }}>{m.provider}</div>
            </div>
          ))}

          <div style={{ fontSize: 11, fontWeight: 600, color: t.secondary, margin: "4px 0 8px" }}>By day</div>
          {MOCK.days.map((d) => (
            <div key={d.date} style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 6 }}>
              <div style={{ width: 44, fontSize: 12, fontVariantNumeric: "tabular-nums" }}>{d.date}</div>
              <div style={{ flex: 1 }}>
                <div
                  style={{
                    height: 8,
                    width: `${d.intensity * 100}%`,
                    borderRadius: 3,
                    background: t.accent,
                    opacity: 0.35 + d.intensity * 0.45,
                  }}
                />
              </div>
              <div style={{ width: 48, textAlign: "right", fontSize: 12, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(d.tokens)}</div>
            </div>
          ))}
        </div>

        <div style={{ height: 1, background: t.divider, marginTop: 8 }} />
        <div style={{ padding: "10px 16px 12px" }}>
          <div style={{ fontSize: 11, color: t.secondary, marginBottom: 8 }}>Updated 12m ago · incremental</div>
          <div style={{ display: "flex", gap: 14, fontSize: 13, color: t.accent }}>
            <span>Refresh</span>
            <span>Settings…</span>
            <span style={{ marginLeft: "auto" }}>tokens.ci</span>
            <span style={{ color: t.secondary }}>Quit</span>
          </div>
        </div>
      </div>
    </MenuBarChrome>
  );
}

/* ───────── 02 Terminal ───────── */
function Panel02Terminal({ theme }) {
  const dark = theme === "dark";
  const t = {
    panel: dark ? "#0c0f0c" : "#f4f7f0",
    text: dark ? "#b6f5a8" : "#1a2e14",
    dim: dark ? "#5a8a52" : "#5a7050",
    accent: dark ? "#7CFF6B" : "#2f7a28",
    warn: dark ? "#ffd479" : "#9a6b00",
    border: dark ? "#1e2e1c" : "#c5d4bc",
    fill: dark ? "#7CFF6B" : "#3d9a34",
    font: '"SF Mono","JetBrains Mono",ui-monospace,Menlo,monospace',
  };
  return (
    <MenuBarChrome theme={theme} title={MOCK.menuBarTitle} accent={t.accent}>
      <div
        data-screen-label="02-terminal"
        style={{
          width: PANEL_W,
          fontFamily: t.font,
          color: t.text,
          background: t.panel,
          borderRadius: 6,
          border: `1px solid ${t.border}`,
          boxShadow: dark ? "0 16px 40px rgba(0,0,0,.55), 0 0 0 1px #000" : "0 12px 32px rgba(30,50,20,.15)",
        }}
      >
        <div style={{ padding: "10px 12px", borderBottom: `1px solid ${t.border}`, display: "flex", gap: 8, alignItems: "center" }}>
          <span style={{ color: t.accent }}>$</span>
          <span style={{ fontSize: 12 }}>tokens usage --period 7d</span>
          <span style={{ marginLeft: "auto", color: t.dim, fontSize: 11 }}>● live</span>
        </div>
        <div style={{ padding: 12, fontSize: 12, lineHeight: 1.55 }}>
          <div style={{ color: t.dim, marginBottom: 6 }}>
            # period {MOCK.dateRange.start} → {MOCK.dateRange.end}
          </div>
          <div style={{ display: "flex", gap: 16, marginBottom: 12, flexWrap: "wrap" }}>
            <span>
              <span style={{ color: t.dim }}>TOKENS</span>{" "}
              <strong style={{ color: t.accent, fontSize: 16 }}>{fmtTokens(MOCK.summary.totalTokens)}</strong>
            </span>
            <span>
              <span style={{ color: t.dim }}>COST</span>{" "}
              <strong style={{ color: t.warn, fontSize: 16 }}>{fmtCost(MOCK.summary.totalCost)}</strong>
            </span>
            <span>
              <span style={{ color: t.dim }}>MSGS</span> <strong style={{ fontSize: 16 }}>{MOCK.summary.messages}</strong>
            </span>
          </div>

          <div style={{ color: t.dim, marginBottom: 4 }}>// breakdown</div>
          <div style={{ marginBottom: 12 }}>
            {MOCK.breakdown.map((b) => (
              <div key={b.key} style={{ display: "flex", gap: 8 }}>
                <span style={{ width: 56, color: t.dim }}>{b.label}</span>
                <span style={{ width: 48, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(b.value)}</span>
                <span style={{ color: t.dim }}>
                  {"█".repeat(Math.max(1, Math.round((b.value / MOCK.summary.totalTokens) * 18)))}
                  {"░".repeat(Math.max(0, 18 - Math.round((b.value / MOCK.summary.totalTokens) * 18)))}
                </span>
              </div>
            ))}
          </div>

          <div style={{ color: t.dim, marginBottom: 4 }}>// by client</div>
          {MOCK.clients.map((c) => (
            <div key={c.name} style={{ marginBottom: 6 }}>
              <div style={{ display: "flex", gap: 8 }}>
                <span style={{ flex: 1 }}>{c.name}</span>
                <span style={{ fontVariantNumeric: "tabular-nums" }}>{fmtTokens(c.tokens)}</span>
                <span style={{ color: t.dim, width: 52, textAlign: "right" }}>{fmtCost(c.cost)}</span>
              </div>
              <div style={{ color: t.fill, fontSize: 11 }}>
                {"▓".repeat(Math.round(c.share * 28))}
                <span style={{ color: t.dim }}>{"░".repeat(28 - Math.round(c.share * 28))}</span>
                <span style={{ color: t.dim }}> {fmtPct(c.share)}</span>
              </div>
            </div>
          ))}

          <div style={{ color: t.dim, margin: "10px 0 4px" }}>// by model</div>
          {MOCK.models.map((m) => (
            <div key={m.name} style={{ display: "flex", gap: 8, marginBottom: 4 }}>
              <span style={{ flex: 1 }}>{m.name}</span>
              <span style={{ color: t.dim }}>{m.provider}</span>
              <span style={{ fontVariantNumeric: "tabular-nums" }}>{fmtTokens(m.tokens)}</span>
            </div>
          ))}

          <div style={{ color: t.dim, margin: "10px 0 4px" }}>// by day</div>
          {MOCK.days.map((d) => (
            <div key={d.date} style={{ display: "flex", gap: 8 }}>
              <span style={{ width: 40, color: t.dim }}>{d.date}</span>
              <span style={{ color: t.fill }}>{"▊".repeat(Math.round(d.intensity * 14))}</span>
              <span style={{ fontVariantNumeric: "tabular-nums" }}>{fmtTokens(d.tokens)}</span>
            </div>
          ))}
        </div>
        <div style={{ borderTop: `1px solid ${t.border}`, padding: "8px 12px", display: "flex", gap: 12, fontSize: 11, color: t.dim, flexWrap: "wrap" }}>
          <span>
            <span style={{ color: t.accent }}>[r]</span> refresh
          </span>
          <span>
            <span style={{ color: t.accent }}>[s]</span> settings
          </span>
          <span>
            <span style={{ color: t.accent }}>[q]</span> quit
          </span>
          <span style={{ marginLeft: "auto" }}>updated 12m · {MOCK.scanMode}</span>
        </div>
      </div>
    </MenuBarChrome>
  );
}

/* ───────── 03 Dense Dashboard ───────── */
function Panel03Dense({ theme }) {
  const dark = theme === "dark";
  const t = {
    panel: dark ? "#14161c" : "#f6f7f9",
    text: dark ? "#eef0f4" : "#14161c",
    muted: dark ? "#8b92a3" : "#6b7280",
    card: dark ? "#1c1f28" : "#ffffff",
    border: dark ? "#2a2f3a" : "#e4e7ec",
    accent: "#6366f1",
    accent2: "#22c55e",
    accent3: "#f59e0b",
    accent4: "#ec4899",
    track: dark ? "#2a2f3a" : "#eceef2",
    font: "Inter,ui-sans-serif,system-ui,-apple-system,sans-serif",
  };
  const chips = [
    { ...MOCK.breakdown[0], color: t.accent },
    { ...MOCK.breakdown[1], color: t.accent2 },
    { ...MOCK.breakdown[2], color: t.accent3 },
    { ...MOCK.breakdown[3], color: t.accent4 },
  ];
  return (
    <MenuBarChrome theme={theme} title={MOCK.menuBarTitle} accent={t.accent}>
      <div
        data-screen-label="03-dense"
        style={{
          width: PANEL_W,
          fontFamily: t.font,
          color: t.text,
          background: t.panel,
          borderRadius: 14,
          border: `1px solid ${t.border}`,
          boxShadow: dark ? "0 20px 48px rgba(0,0,0,.5)" : "0 16px 40px rgba(20,22,28,.12)",
        }}
      >
        <div style={{ padding: "12px 14px 8px", display: "flex", alignItems: "center", gap: 8 }}>
          <div
            style={{
              width: 22,
              height: 22,
              borderRadius: 6,
              background: `linear-gradient(135deg,${t.accent},${t.accent4})`,
              display: "grid",
              placeItems: "center",
              color: "#fff",
              fontSize: 11,
              fontWeight: 700,
            }}
          >
            T
          </div>
          <div style={{ fontSize: 13, fontWeight: 650 }}>Usage</div>
          <div style={{ marginLeft: "auto", display: "flex", background: t.card, border: `1px solid ${t.border}`, borderRadius: 8, padding: 2, gap: 1 }}>
            {PERIODS.map((p) => {
              const on = p === "7d";
              return (
                <div
                  key={p}
                  style={{
                    fontSize: 11,
                    fontWeight: 600,
                    padding: "4px 8px",
                    borderRadius: 6,
                    background: on ? t.accent : "transparent",
                    color: on ? "#fff" : t.muted,
                  }}
                >
                  {p}
                </div>
              );
            })}
          </div>
        </div>

        <div style={{ padding: "4px 14px 12px", display: "grid", gridTemplateColumns: "1.2fr 1fr 1fr", gap: 8 }}>
          {[
            { l: "Tokens", v: fmtTokens(MOCK.summary.totalTokens), c: t.accent },
            { l: "Cost", v: fmtCost(MOCK.summary.totalCost), c: t.accent3 },
            { l: "Msgs", v: String(MOCK.summary.messages), c: t.accent2 },
          ].map((m) => (
            <div key={m.l} style={{ background: t.card, border: `1px solid ${t.border}`, borderRadius: 10, padding: "10px 10px 8px" }}>
              <div style={{ fontSize: 10, fontWeight: 600, color: t.muted, textTransform: "uppercase", letterSpacing: "0.04em" }}>{m.l}</div>
              <div style={{ fontSize: 18, fontWeight: 700, fontVariantNumeric: "tabular-nums", marginTop: 2, color: m.c }}>{m.v}</div>
            </div>
          ))}
        </div>

        <div style={{ padding: "0 14px 12px" }}>
          <div style={{ display: "flex", height: 10, borderRadius: 99, overflow: "hidden", marginBottom: 8 }}>
            {chips.map((b) => (
              <div key={b.key} style={{ width: `${(b.value / MOCK.summary.totalTokens) * 100}%`, background: b.color }} />
            ))}
          </div>
          <div style={{ display: "flex", gap: 10, flexWrap: "wrap" }}>
            {chips.map((b) => (
              <div key={b.key} style={{ display: "flex", alignItems: "center", gap: 5, fontSize: 11, color: t.muted }}>
                <span style={{ width: 7, height: 7, borderRadius: 99, background: b.color }} />
                {b.label} <strong style={{ color: t.text, fontWeight: 600 }}>{fmtTokens(b.value)}</strong>
              </div>
            ))}
          </div>
          <div style={{ marginTop: 8, fontSize: 11, color: t.muted }}>
            {MOCK.dateRange.start} → {MOCK.dateRange.end}
          </div>
        </div>

        <div style={{ padding: "0 14px 10px" }}>
          <div style={{ fontSize: 10, fontWeight: 700, color: t.muted, letterSpacing: "0.05em", textTransform: "uppercase", marginBottom: 6 }}>Clients</div>
          {MOCK.clients.map((c, i) => (
            <div key={c.name} style={{ display: "grid", gridTemplateColumns: "18px 1fr auto auto", gap: 8, alignItems: "center", marginBottom: 8 }}>
              <div
                style={{
                  width: 18,
                  height: 18,
                  borderRadius: 5,
                  background: [t.accent, t.accent2, t.accent3][i],
                  color: "#fff",
                  fontSize: 10,
                  fontWeight: 700,
                  display: "grid",
                  placeItems: "center",
                }}
              >
                {c.name[0]}
              </div>
              <div>
                <div style={{ fontSize: 12, fontWeight: 600 }}>{c.name}</div>
                <ShareBar share={c.share} color={[t.accent, t.accent2, t.accent3][i]} track={t.track} height={4} />
              </div>
              <div style={{ fontSize: 12, fontWeight: 650, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(c.tokens)}</div>
              <div style={{ fontSize: 11, color: t.muted, fontVariantNumeric: "tabular-nums", width: 48, textAlign: "right" }}>{fmtCost(c.cost)}</div>
            </div>
          ))}
        </div>

        <div style={{ padding: "0 14px 10px" }}>
          <div style={{ fontSize: 10, fontWeight: 700, color: t.muted, letterSpacing: "0.05em", textTransform: "uppercase", marginBottom: 6 }}>Models</div>
          {MOCK.models.map((m, i) => (
            <div key={m.name} style={{ display: "grid", gridTemplateColumns: "1fr auto auto", gap: 8, alignItems: "center", marginBottom: 8 }}>
              <div>
                <div style={{ fontSize: 12, fontWeight: 600 }}>{m.name}</div>
                <div style={{ fontSize: 10, color: t.muted }}>{m.provider}</div>
              </div>
              <div style={{ fontSize: 12, fontWeight: 650, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(m.tokens)}</div>
              <div style={{ fontSize: 11, color: t.muted, fontVariantNumeric: "tabular-nums", width: 48, textAlign: "right" }}>{fmtCost(m.cost)}</div>
            </div>
          ))}
        </div>

        <div style={{ padding: "0 14px 12px" }}>
          <div style={{ fontSize: 10, fontWeight: 700, color: t.muted, letterSpacing: "0.05em", textTransform: "uppercase", marginBottom: 6 }}>Last 5 days</div>
          <div style={{ display: "flex", alignItems: "flex-end", gap: 6, height: 72, background: t.card, border: `1px solid ${t.border}`, borderRadius: 10, padding: "8px 10px" }}>
            {MOCK.days
              .slice()
              .reverse()
              .map((d) => (
                <div key={d.date} style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", gap: 4, height: "100%", justifyContent: "flex-end" }}>
                  <div style={{ width: "100%", borderRadius: 4, background: `linear-gradient(180deg,${t.accent},${t.accent}88)`, height: `${d.intensity * 100}%`, minHeight: 6 }} />
                  <div style={{ fontSize: 9, color: t.muted }}>{d.date}</div>
                </div>
              ))}
          </div>
        </div>

        <div style={{ borderTop: `1px solid ${t.border}`, padding: "10px 14px", display: "flex", alignItems: "center", gap: 10, fontSize: 11, color: t.muted }}>
          <span>Updated 12m · {MOCK.scanMode}</span>
          <span style={{ marginLeft: "auto", color: t.accent, fontWeight: 600 }}>Refresh</span>
          <span>Settings…</span>
          <span>tokens.ci</span>
          <span>Quit</span>
        </div>
      </div>
    </MenuBarChrome>
  );
}

/* ───────── 04 Brand Neon ───────── */
function Panel04Neon({ theme }) {
  const dark = theme === "dark";
  const t = {
    panel: "#0a0a0f",
    text: "#f5f5f7",
    muted: "rgba(255,255,255,.55)",
    accent: "#ff4d6d",
    accent2: "#7c5cff",
    accent3: "#00e5a8",
    card: "rgba(255,255,255,.04)",
    border: "rgba(255,255,255,.1)",
    track: "rgba(255,255,255,.08)",
    font: 'ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif',
  };
  return (
    <MenuBarChrome theme={theme} title={MOCK.menuBarTitle} accent={t.accent}>
      <div
        data-screen-label="04-neon"
        style={{
          width: PANEL_W,
          fontFamily: t.font,
          color: t.text,
          background: t.panel,
          borderRadius: 16,
          border: `1px solid ${t.border}`,
          boxShadow: "0 0 0 1px rgba(255,77,109,.15), 0 20px 60px rgba(0,0,0,.55), 0 0 40px rgba(124,92,255,.12)",
          position: "relative",
        }}
      >
        <div
          style={{
            position: "absolute",
            inset: 0,
            borderRadius: 16,
            background:
              "radial-gradient(ellipse 80% 50% at 20% 0%, rgba(124,92,255,.22), transparent 55%), radial-gradient(ellipse 70% 40% at 90% 10%, rgba(255,77,109,.18), transparent 50%)",
            pointerEvents: "none",
          }}
        />
        <div style={{ position: "relative" }}>
          <div style={{ padding: "14px 16px 10px", display: "flex", alignItems: "center", gap: 10 }}>
            <div
              style={{
                width: 28,
                height: 28,
                borderRadius: 8,
                background: "linear-gradient(135deg,#ff4d6d,#7c5cff)",
                display: "grid",
                placeItems: "center",
                boxShadow: "0 0 16px rgba(255,77,109,.45)",
              }}
            >
              <svg width="16" height="16" viewBox="0 0 64 64" style={{ color: "#fff" }}>
                <g stroke="currentColor" strokeWidth="6" strokeLinecap="round" fill="none">
                  <path d="M14 15h36" />
                  <path d="M32 15v34" />
                  <path d="M22 27h20" opacity="0.75" />
                  <path d="M24.5 37h15" opacity="0.5" />
                  <path d="M27 47h10" opacity="0.3" />
                </g>
              </svg>
            </div>
            <div>
              <div style={{ fontSize: 14, fontWeight: 700, letterSpacing: "-0.02em" }}>Tokens</div>
              <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.08em", textTransform: "uppercase" }}>Local burn rate</div>
            </div>
            <div
              style={{
                marginLeft: "auto",
                fontSize: 10,
                fontWeight: 700,
                color: t.accent3,
                padding: "4px 8px",
                borderRadius: 99,
                background: "rgba(0,229,168,.12)",
                border: "1px solid rgba(0,229,168,.25)",
              }}
            >
              LIVE
            </div>
          </div>

          <div style={{ padding: "0 16px 12px" }}>
            <div style={{ display: "flex", gap: 4, marginBottom: 14 }}>
              {PERIODS.map((p) => {
                const on = p === "7d";
                return (
                  <div
                    key={p}
                    style={{
                      flex: 1,
                      textAlign: "center",
                      fontSize: 11,
                      fontWeight: 700,
                      padding: "7px 0",
                      borderRadius: 8,
                      background: on ? "linear-gradient(135deg,rgba(255,77,109,.25),rgba(124,92,255,.25))" : t.card,
                      border: on ? "1px solid rgba(255,77,109,.45)" : `1px solid ${t.border}`,
                      color: on ? "#fff" : t.muted,
                      boxShadow: on ? "0 0 12px rgba(255,77,109,.2)" : "none",
                    }}
                  >
                    {p}
                  </div>
                );
              })}
            </div>

            <div style={{ background: t.card, border: `1px solid ${t.border}`, borderRadius: 14, padding: 14, marginBottom: 12 }}>
              <div style={{ fontSize: 11, color: t.muted, marginBottom: 4 }}>Total tokens · 7 days</div>
              <div
                style={{
                  fontSize: 32,
                  fontWeight: 800,
                  letterSpacing: "-0.03em",
                  fontVariantNumeric: "tabular-nums",
                  background: "linear-gradient(90deg,#fff 0%,#ff4d6d 55%,#7c5cff 100%)",
                  WebkitBackgroundClip: "text",
                  backgroundClip: "text",
                  color: "transparent",
                }}
              >
                {fmtTokens(MOCK.summary.totalTokens)}
              </div>
              <div style={{ display: "flex", gap: 16, marginTop: 10 }}>
                <div>
                  <div style={{ fontSize: 10, color: t.muted }}>Cost</div>
                  <div style={{ fontSize: 16, fontWeight: 700, color: t.accent3 }}>{fmtCost(MOCK.summary.totalCost)}</div>
                </div>
                <div>
                  <div style={{ fontSize: 10, color: t.muted }}>Messages</div>
                  <div style={{ fontSize: 16, fontWeight: 700 }}>{MOCK.summary.messages}</div>
                </div>
                <div style={{ marginLeft: "auto", alignSelf: "end", fontSize: 10, color: t.muted }}>
                  {MOCK.dateRange.start} → {MOCK.dateRange.end}
                </div>
              </div>
            </div>

            <div style={{ fontSize: 10, fontWeight: 700, letterSpacing: "0.08em", color: t.muted, marginBottom: 8 }}>BREAKDOWN</div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 6, marginBottom: 14 }}>
              {MOCK.breakdown.map((b, i) => {
                const colors = [t.accent, t.accent2, t.accent3, "#ffc14d"];
                return (
                  <div key={b.key} style={{ background: t.card, border: `1px solid ${t.border}`, borderRadius: 10, padding: "8px 8px 7px", borderTop: `2px solid ${colors[i]}` }}>
                    <div style={{ fontSize: 10, color: t.muted }}>{b.label}</div>
                    <div style={{ fontSize: 12, fontWeight: 700, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(b.value)}</div>
                  </div>
                );
              })}
            </div>

            <div style={{ fontSize: 10, fontWeight: 700, letterSpacing: "0.08em", color: t.muted, marginBottom: 8 }}>BY CLIENT</div>
            {MOCK.clients.map((c, i) => {
              const colors = [t.accent, t.accent2, t.accent3];
              return (
                <div key={c.name} style={{ marginBottom: 10 }}>
                  <div style={{ display: "flex", alignItems: "baseline", gap: 8, marginBottom: 4 }}>
                    <span style={{ fontSize: 12, fontWeight: 600, flex: 1 }}>{c.name}</span>
                    <span style={{ fontSize: 12, fontWeight: 700, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(c.tokens)}</span>
                    <span style={{ fontSize: 11, color: t.muted, width: 48, textAlign: "right" }}>{fmtCost(c.cost)}</span>
                  </div>
                  <ShareBar share={c.share} color={colors[i]} track={t.track} height={6} />
                </div>
              );
            })}

            <div style={{ fontSize: 10, fontWeight: 700, letterSpacing: "0.08em", color: t.muted, margin: "12px 0 8px" }}>BY MODEL</div>
            {MOCK.models.map((m, i) => {
              const colors = [t.accent, t.accent2, t.accent3];
              return (
                <div key={m.name} style={{ marginBottom: 10 }}>
                  <div style={{ display: "flex", alignItems: "baseline", gap: 8, marginBottom: 4 }}>
                    <span style={{ fontSize: 12, fontWeight: 600, flex: 1 }}>{m.name}</span>
                    <span style={{ fontSize: 11, color: t.muted }}>{m.provider}</span>
                    <span style={{ fontSize: 12, fontWeight: 700, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(m.tokens)}</span>
                  </div>
                  <ShareBar share={m.share} color={colors[i % 3]} track={t.track} height={4} />
                </div>
              );
            })}

            <div style={{ fontSize: 10, fontWeight: 700, letterSpacing: "0.08em", color: t.muted, margin: "12px 0 8px" }}>BY DAY</div>
            {MOCK.days.map((d) => (
              <div key={d.date} style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 5 }}>
                <span style={{ width: 40, fontSize: 11, color: t.muted, fontVariantNumeric: "tabular-nums" }}>{d.date}</span>
                <div style={{ flex: 1, height: 8, borderRadius: 99, background: t.track, overflow: "hidden" }}>
                  <div
                    style={{
                      width: `${d.intensity * 100}%`,
                      height: "100%",
                      background: "linear-gradient(90deg,#ff4d6d,#7c5cff)",
                      boxShadow: "0 0 8px rgba(255,77,109,.4)",
                    }}
                  />
                </div>
                <span style={{ width: 44, textAlign: "right", fontSize: 11, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(d.tokens)}</span>
              </div>
            ))}
          </div>

          <div style={{ borderTop: `1px solid ${t.border}`, padding: "10px 16px", display: "flex", gap: 12, fontSize: 12, alignItems: "center" }}>
            <span style={{ color: t.muted, fontSize: 11 }}>Updated 12m · incremental</span>
            <span style={{ marginLeft: "auto", color: t.accent, fontWeight: 700 }}>Refresh</span>
            <span style={{ color: t.muted }}>Settings…</span>
            <span style={{ color: t.accent2, fontWeight: 600 }}>tokens.ci</span>
            <span style={{ color: t.muted }}>Quit</span>
          </div>
        </div>
      </div>
    </MenuBarChrome>
  );
}

/* ───────── 05 Minimal Mono ───────── */
function Panel05Minimal({ theme }) {
  const dark = theme === "dark";
  const t = {
    panel: dark ? "#111111" : "#fafafa",
    text: dark ? "#f2f2f2" : "#111111",
    muted: dark ? "#888" : "#777",
    line: dark ? "#2a2a2a" : "#e6e6e6",
    accent: dark ? "#f2f2f2" : "#111111",
    track: dark ? "#2a2a2a" : "#ebebeb",
    fill: dark ? "#f2f2f2" : "#111111",
    font: '"IBM Plex Mono","SF Mono",ui-monospace,Menlo,monospace',
  };
  return (
    <MenuBarChrome theme={theme} title={MOCK.menuBarTitle} accent={t.accent}>
      <div
        data-screen-label="05-minimal"
        style={{
          width: PANEL_W,
          fontFamily: t.font,
          color: t.text,
          background: t.panel,
          borderRadius: 2,
          border: `1px solid ${t.line}`,
          boxShadow: dark ? "0 16px 40px rgba(0,0,0,.5)" : "0 12px 32px rgba(0,0,0,.08)",
        }}
      >
        <div style={{ padding: "16px 18px 12px", borderBottom: `1px solid ${t.line}` }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
            <div style={{ fontSize: 11, letterSpacing: "0.14em", textTransform: "uppercase" }}>Tokens</div>
            <div style={{ fontSize: 10, color: t.muted }}>usage · local</div>
          </div>
        </div>

        <div style={{ padding: "12px 18px", borderBottom: `1px solid ${t.line}`, display: "flex" }}>
          {PERIODS.map((p) => {
            const on = p === "7d";
            return (
              <div
                key={p}
                style={{
                  flex: 1,
                  textAlign: "center",
                  fontSize: 11,
                  padding: "6px 0",
                  borderBottom: on ? `2px solid ${t.accent}` : "2px solid transparent",
                  color: on ? t.text : t.muted,
                  letterSpacing: "0.04em",
                }}
              >
                {p.toUpperCase()}
              </div>
            );
          })}
        </div>

        <div style={{ padding: "18px 18px 14px" }}>
          <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", marginBottom: 6 }}>TOTAL</div>
          <div style={{ fontSize: 36, fontWeight: 500, letterSpacing: "-0.04em", fontVariantNumeric: "tabular-nums", lineHeight: 1 }}>
            {fmtTokens(MOCK.summary.totalTokens)}
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginTop: 14, paddingTop: 14, borderTop: `1px solid ${t.line}` }}>
            <div>
              <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.08em" }}>COST</div>
              <div style={{ fontSize: 18, fontVariantNumeric: "tabular-nums" }}>{fmtCost(MOCK.summary.totalCost)}</div>
            </div>
            <div>
              <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.08em" }}>MESSAGES</div>
              <div style={{ fontSize: 18, fontVariantNumeric: "tabular-nums" }}>{MOCK.summary.messages}</div>
            </div>
          </div>
          <div style={{ marginTop: 10, fontSize: 10, color: t.muted }}>
            {MOCK.dateRange.start} — {MOCK.dateRange.end}
          </div>
        </div>

        <div style={{ padding: "0 18px 14px" }}>
          <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", marginBottom: 10, borderTop: `1px solid ${t.line}`, paddingTop: 14 }}>BREAKDOWN</div>
          {MOCK.breakdown.map((b) => (
            <div key={b.key} style={{ display: "flex", justifyContent: "space-between", fontSize: 12, marginBottom: 6, fontVariantNumeric: "tabular-nums" }}>
              <span style={{ color: t.muted }}>{b.label}</span>
              <span>{fmtTokens(b.value)}</span>
            </div>
          ))}
        </div>

        <div style={{ padding: "0 18px 14px" }}>
          <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", marginBottom: 10, borderTop: `1px solid ${t.line}`, paddingTop: 14 }}>CLIENT</div>
          {MOCK.clients.map((c) => (
            <div key={c.name} style={{ marginBottom: 10 }}>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, marginBottom: 4 }}>
                <span>{c.name}</span>
                <span style={{ fontVariantNumeric: "tabular-nums" }}>
                  {fmtTokens(c.tokens)} · {fmtPct(c.share)}
                </span>
              </div>
              <ShareBar share={c.share} color={t.fill} track={t.track} height={2} radius={0} />
            </div>
          ))}
        </div>

        <div style={{ padding: "0 18px 14px" }}>
          <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", marginBottom: 10, borderTop: `1px solid ${t.line}`, paddingTop: 14 }}>MODEL</div>
          {MOCK.models.map((m) => (
            <div key={m.name} style={{ display: "flex", justifyContent: "space-between", fontSize: 12, marginBottom: 8, fontVariantNumeric: "tabular-nums" }}>
              <span>
                {m.name} <span style={{ color: t.muted }}>/ {m.provider}</span>
              </span>
              <span>{fmtTokens(m.tokens)}</span>
            </div>
          ))}
        </div>

        <div style={{ padding: "0 18px 16px" }}>
          <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", marginBottom: 10, borderTop: `1px solid ${t.line}`, paddingTop: 14 }}>DAY</div>
          {MOCK.days.map((d) => (
            <div key={d.date} style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 6, fontSize: 12 }}>
              <span style={{ width: 40, color: t.muted, fontVariantNumeric: "tabular-nums" }}>{d.date}</span>
              <div style={{ flex: 1, height: 2, background: t.track }}>
                <div style={{ width: `${d.intensity * 100}%`, height: "100%", background: t.fill }} />
              </div>
              <span style={{ width: 48, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{fmtTokens(d.tokens)}</span>
            </div>
          ))}
        </div>

        <div style={{ borderTop: `1px solid ${t.line}`, padding: "12px 18px", display: "flex", fontSize: 11, letterSpacing: "0.04em", color: t.muted, flexWrap: "wrap", gap: 10 }}>
          <span>UPDATED 12M · INC</span>
          <span style={{ marginLeft: "auto", color: t.text }}>REFRESH</span>
          <span>SETTINGS</span>
          <span>TOKENS.CI</span>
          <span>QUIT</span>
        </div>
      </div>
    </MenuBarChrome>
  );
}

/* ───────── 06 Minimal Mono v2 (hybrid) ─────────
   Base: 05 Minimal Mono
   Breakdown: 04 Brand Neon card grid (mono treatment)
   Sections: spacing only, no horizontal rules
   Day: 14-day cost chart (Y = cost, X = date)
*/
function CostChart14({ theme, days, height = 120 }) {
  const dark = theme === "dark";
  const axis = dark ? "rgba(255,255,255,.22)" : "rgba(0,0,0,.16)";
  const label = dark ? "rgba(255,255,255,.45)" : "rgba(0,0,0,.45)";
  const bar = dark ? "#f2f2f2" : "#111111";
  const grid = dark ? "rgba(255,255,255,.06)" : "rgba(0,0,0,.05)";
  const maxCost = Math.max(...days.map((d) => d.cost), 0.01);
  // nice Y ticks: 0, mid, max
  const yMax = Math.ceil(maxCost);
  const ticks = [0, yMax / 2, yMax];
  const padL = 34;
  const padR = 4;
  const padT = 8;
  const padB = 22;
  const plotH = height - padT - padB;
  const n = days.length;
  const gap = 3;

  return (
    <div style={{ position: "relative", height, width: "100%" }}>
      {/* Y grid + labels */}
      {ticks.map((v) => {
        const y = padT + plotH * (1 - v / yMax);
        return (
          <div key={v} style={{ position: "absolute", left: 0, right: 0, top: y }}>
            <div style={{ position: "absolute", left: 0, width: padL - 6, textAlign: "right", fontSize: 9, color: label, transform: "translateY(-50%)", fontVariantNumeric: "tabular-nums" }}>
              ${v % 1 === 0 ? v.toFixed(0) : v.toFixed(1)}
            </div>
            <div style={{ marginLeft: padL, height: 1, background: v === 0 ? axis : grid }} />
          </div>
        );
      })}

      {/* Bars */}
      <div
        style={{
          position: "absolute",
          left: padL,
          right: padR,
          top: padT,
          height: plotH,
          display: "flex",
          alignItems: "flex-end",
          gap,
        }}
      >
        {days.map((d) => {
          const h = Math.max(2, (d.cost / yMax) * plotH);
          return (
            <div key={d.date} style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", height: "100%", justifyContent: "flex-end" }} title={`${d.date}: ${fmtCost(d.cost)}`}>
              <div
                style={{
                  width: "100%",
                  height: h,
                  background: bar,
                  borderRadius: "1px 1px 0 0",
                  opacity: 0.88,
                }}
              />
            </div>
          );
        })}
      </div>

      {/* X labels — show every other day to avoid clutter, always ends */}
      <div
        style={{
          position: "absolute",
          left: padL,
          right: padR,
          bottom: 0,
          height: padB,
          display: "flex",
          gap,
          alignItems: "flex-end",
        }}
      >
        {days.map((d, i) => {
          const show = i === 0 || i === n - 1 || i % 2 === 1;
          return (
            <div
              key={d.date}
              style={{
                flex: 1,
                textAlign: "center",
                fontSize: 9,
                color: label,
                fontVariantNumeric: "tabular-nums",
                visibility: show ? "visible" : "hidden",
              }}
            >
              {d.label}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function Panel06MinimalV2({ theme }) {
  const dark = theme === "dark";
  const t = {
    panel: dark ? "#111111" : "#fafafa",
    text: dark ? "#f2f2f2" : "#111111",
    muted: dark ? "#888" : "#777",
    line: dark ? "#2a2a2a" : "#e6e6e6",
    accent: dark ? "#f2f2f2" : "#111111",
    track: dark ? "#2a2a2a" : "#ebebeb",
    fill: dark ? "#f2f2f2" : "#111111",
    card: dark ? "rgba(255,255,255,.04)" : "rgba(0,0,0,.03)",
    cardBorder: dark ? "rgba(255,255,255,.12)" : "rgba(0,0,0,.1)",
    font: '"IBM Plex Mono","SF Mono",ui-monospace,Menlo,monospace',
  };
  // Mono reinterpretation of neon card accent tops
  const cardTops = dark
    ? ["#f2f2f2", "rgba(255,255,255,.72)", "rgba(255,255,255,.48)", "rgba(255,255,255,.28)"]
    : ["#111", "rgba(0,0,0,.72)", "rgba(0,0,0,.48)", "rgba(0,0,0,.28)"];

  const sectionPad = { padding: "0 18px", marginBottom: 22 };
  const sectionLabel = {
    fontSize: 10,
    color: t.muted,
    letterSpacing: "0.1em",
    marginBottom: 12,
    textTransform: "uppercase",
  };

  return (
    <MenuBarChrome theme={theme} title={MOCK.menuBarTitle} accent={t.accent}>
      <div
        data-screen-label="06-minimal-v2"
        style={{
          width: PANEL_W,
          fontFamily: t.font,
          color: t.text,
          background: t.panel,
          borderRadius: 2,
          border: `1px solid ${t.line}`,
          boxShadow: dark ? "0 16px 40px rgba(0,0,0,.5)" : "0 12px 32px rgba(0,0,0,.08)",
        }}
      >
        {/* Header — no hairline under it, space only */}
        <div style={{ padding: "16px 18px 4px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
            <div style={{ fontSize: 11, letterSpacing: "0.14em", textTransform: "uppercase" }}>Tokens</div>
            <div style={{ fontSize: 10, color: t.muted }}>usage · local</div>
          </div>
        </div>

        {/* Period tabs — underline indicator only, no full-width section rules */}
        <div style={{ padding: "14px 18px 6px", display: "flex" }}>
          {PERIODS.map((p) => {
            const on = p === "7d";
            return (
              <div
                key={p}
                style={{
                  flex: 1,
                  textAlign: "center",
                  fontSize: 11,
                  padding: "6px 0",
                  borderBottom: on ? `2px solid ${t.accent}` : "2px solid transparent",
                  color: on ? t.text : t.muted,
                  letterSpacing: "0.04em",
                }}
              >
                {p.toUpperCase()}
              </div>
            );
          })}
        </div>

        {/* Summary */}
        <div style={{ ...sectionPad, paddingTop: 18, marginBottom: 8 }}>
          <div style={sectionLabel}>Total</div>
          <div style={{ fontSize: 36, fontWeight: 500, letterSpacing: "-0.04em", fontVariantNumeric: "tabular-nums", lineHeight: 1 }}>
            {fmtTokens(MOCK.summary.totalTokens)}
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16, marginTop: 16 }}>
            <div>
              <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.08em" }}>COST</div>
              <div style={{ fontSize: 18, fontVariantNumeric: "tabular-nums", marginTop: 4 }}>{fmtCost(MOCK.summary.totalCost)}</div>
            </div>
            <div>
              <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.08em" }}>MESSAGES</div>
              <div style={{ fontSize: 18, fontVariantNumeric: "tabular-nums", marginTop: 4 }}>{MOCK.summary.messages}</div>
            </div>
          </div>
          <div style={{ marginTop: 12, fontSize: 10, color: t.muted }}>
            {MOCK.dateRange.start} — {MOCK.dateRange.end}
          </div>
        </div>

        {/* Breakdown — 04-style card grid, mono palette */}
        <div style={sectionPad}>
          <div style={sectionLabel}>Breakdown</div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 6 }}>
            {MOCK.breakdown.map((b, i) => (
              <div
                key={b.key}
                style={{
                  background: t.card,
                  border: `1px solid ${t.cardBorder}`,
                  borderRadius: 8,
                  padding: "10px 8px 9px",
                  borderTop: `2px solid ${cardTops[i]}`,
                }}
              >
                <div style={{ fontSize: 10, color: t.muted, marginBottom: 4 }}>{b.label}</div>
                <div style={{ fontSize: 12, fontWeight: 700, fontVariantNumeric: "tabular-nums" }}>{fmtTokens(b.value)}</div>
              </div>
            ))}
          </div>
        </div>

        {/* Client */}
        <div style={sectionPad}>
          <div style={sectionLabel}>Client</div>
          {MOCK.clients.map((c) => (
            <div key={c.name} style={{ marginBottom: 12 }}>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, marginBottom: 5 }}>
                <span>{c.name}</span>
                <span style={{ fontVariantNumeric: "tabular-nums" }}>
                  {fmtTokens(c.tokens)} · {fmtPct(c.share)}
                </span>
              </div>
              <ShareBar share={c.share} color={t.fill} track={t.track} height={2} radius={0} />
            </div>
          ))}
        </div>

        {/* Model */}
        <div style={sectionPad}>
          <div style={sectionLabel}>Model</div>
          {MOCK.models.map((m) => (
            <div key={m.name} style={{ display: "flex", justifyContent: "space-between", fontSize: 12, marginBottom: 10, fontVariantNumeric: "tabular-nums" }}>
              <span>
                {m.name} <span style={{ color: t.muted }}>/ {m.provider}</span>
              </span>
              <span>{fmtTokens(m.tokens)}</span>
            </div>
          ))}
        </div>

        {/* Cost chart — last 14 days */}
        <div style={{ ...sectionPad, marginBottom: 18 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 12 }}>
            <div style={{ ...sectionLabel, marginBottom: 0 }}>Cost · 14 days</div>
            <div style={{ fontSize: 10, color: t.muted, fontVariantNumeric: "tabular-nums" }}>Y = $ · X = date</div>
          </div>
          <CostChart14 theme={theme} days={MOCK.days14} height={128} />
        </div>

        {/* Footer — space separator only, no top border */}
        <div style={{ padding: "4px 18px 14px", display: "flex", fontSize: 11, letterSpacing: "0.04em", color: t.muted, flexWrap: "wrap", gap: 10 }}>
          <span>UPDATED 12M · INC</span>
          <span style={{ marginLeft: "auto", color: t.text }}>REFRESH</span>
          <span>SETTINGS</span>
          <span>TOKENS.CI</span>
          <span>QUIT</span>
        </div>
      </div>
    </MenuBarChrome>
  );
}

Object.assign(window, {
  Panel01Native,
  Panel02Terminal,
  Panel03Dense,
  Panel04Neon,
  Panel05Minimal,
  Panel06MinimalV2,
  PANEL_W,
});
