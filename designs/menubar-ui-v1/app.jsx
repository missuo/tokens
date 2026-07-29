const { useState } = React;

const FINAL = {
  id: "final",
  label: "FINAL · 06 Minimal Mono v2",
  blurb: "Locked visual language for this version",
  Component: PanelFinalRest,
};

const INTERACTIONS = [
  {
    id: "ix-hover",
    label: "IX-A · Day chart hover",
    blurb: "Bar focus · dim siblings · guide line · tooltip (cost + tokens)",
    Component: InteractionHoverDay,
  },
  {
    id: "ix-scroll",
    label: "IX-B · Long list scroll",
    blurb: "Fixed viewport · edge fades · thin mono scrollbar · mid-scroll state",
    Component: InteractionScroll,
  },
  {
    id: "ix-settings",
    label: "IX-C · Settings panel",
    blurb: "Display / Interval / Full rescan / CLI path — mono form language",
    Component: InteractionSettings,
  },
  {
    id: "ix-settings-ctx",
    label: "IX-D · Settings in context",
    blurb: "Sheet over dimmed main panel (open from footer Settings…)",
    Component: InteractionSettingsFromMenu,
  },
];

const ARCHIVE = [
  { id: "05", label: "05 · Minimal Mono", blurb: "Baseline", Component: Panel05Minimal },
  { id: "04", label: "04 · Brand Neon", blurb: "Breakdown source", Component: Panel04Neon },
  { id: "01", label: "01 · Native Glass", blurb: "HIG", Component: Panel01Native },
  { id: "02", label: "02 · Terminal", blurb: "Console", Component: Panel02Terminal },
  { id: "03", label: "03 · Dense Dashboard", blurb: "KPI", Component: Panel03Dense },
];

const TABS = [
  { id: "final", label: "Final" },
  { id: "interactions", label: "Interactions" },
  { id: "archive", label: "Archive 01–05" },
  { id: "all", label: "All" },
];

function App() {
  const [theme, setTheme] = useState("dark");
  const [tab, setTab] = useState("interactions");

  let items = [];
  if (tab === "final") items = [FINAL];
  else if (tab === "interactions") items = [FINAL, ...INTERACTIONS];
  else if (tab === "archive") items = ARCHIVE;
  else items = [FINAL, ...INTERACTIONS, ...ARCHIVE];

  return (
    <div
      style={{
        minHeight: "100vh",
        background: theme === "dark" ? "#0b0c10" : "#eceae4",
        color: theme === "dark" ? "#f2efe8" : "#1c1814",
        fontFamily: '-apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif',
      }}
    >
      <header
        style={{
          position: "sticky",
          top: 0,
          zIndex: 20,
          backdropFilter: "blur(16px)",
          background: theme === "dark" ? "rgba(11,12,16,.86)" : "rgba(236,234,228,.9)",
          borderBottom: theme === "dark" ? "1px solid rgba(255,255,255,.08)" : "1px solid rgba(0,0,0,.08)",
          padding: "14px 20px",
        }}
      >
        <div style={{ maxWidth: 1480, margin: "0 auto" }}>
          <div style={{ display: "flex", gap: 16, alignItems: "flex-start", flexWrap: "wrap" }}>
            <div style={{ flex: "1 1 280px" }}>
              <div style={{ fontSize: 18, fontWeight: 700, letterSpacing: "-0.02em" }}>
                Tokens Menu Bar — Final + Interactions
              </div>
              <div style={{ fontSize: 13, opacity: 0.7, marginTop: 4, lineHeight: 1.45 }}>
                Visual final is <strong>06 Minimal Mono v2</strong>. Interaction frames: chart hover · long-list scroll · settings.
              </div>
            </div>
            <div style={{ display: "flex", gap: 4, padding: 4, borderRadius: 10, background: theme === "dark" ? "rgba(255,255,255,.08)" : "rgba(0,0,0,.06)" }}>
              {["light", "dark"].map((m) => (
                <button
                  key={m}
                  type="button"
                  onClick={() => setTheme(m)}
                  style={{
                    border: "none",
                    cursor: "pointer",
                    padding: "8px 12px",
                    borderRadius: 7,
                    background: theme === m ? (theme === "dark" ? "#fff" : "#111") : "transparent",
                    color: theme === m ? (theme === "dark" ? "#111" : "#fff") : "inherit",
                    fontWeight: 600,
                    fontSize: 12,
                    textTransform: "capitalize",
                  }}
                >
                  {m}
                </button>
              ))}
            </div>
          </div>

          <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginTop: 12 }}>
            {TABS.map((t) => (
              <FilterChip key={t.id} active={tab === t.id} onClick={() => setTab(t.id)} theme={theme}>
                {t.label}
              </FilterChip>
            ))}
          </div>
        </div>
      </header>

      <main style={{ maxWidth: 1480, margin: "0 auto", padding: "24px 20px 64px" }}>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: items.length === 1 ? "1fr" : "repeat(auto-fit, minmax(500px, 1fr))",
            gap: 28,
            justifyItems: "center",
          }}
        >
          {items.map((d) => {
            const C = d.Component;
            return (
              <section
                key={d.id}
                data-design-id={d.id}
                style={{ width: "100%", maxWidth: d.id === "ix-settings-ctx" ? 560 : 560 }}
              >
                <div style={{ marginBottom: 10, padding: "0 4px" }}>
                  <div style={{ fontSize: 15, fontWeight: 700 }}>
                    {d.id === "final" ? (
                      <span>
                        <span
                          style={{
                            display: "inline-block",
                            fontSize: 10,
                            letterSpacing: "0.08em",
                            padding: "2px 6px",
                            borderRadius: 999,
                            marginRight: 8,
                            verticalAlign: "middle",
                            background: theme === "dark" ? "rgba(255,255,255,.12)" : "rgba(0,0,0,.08)",
                          }}
                        >
                          FINAL
                        </span>
                        {d.label}
                      </span>
                    ) : (
                      d.label
                    )}
                  </div>
                  <div style={{ fontSize: 12, opacity: 0.65, marginTop: 2 }}>{d.blurb}</div>
                </div>
                <C theme={theme} />
              </section>
            );
          })}
        </div>

        <section
          style={{
            marginTop: 36,
            padding: 18,
            borderRadius: 12,
            background: theme === "dark" ? "rgba(255,255,255,.04)" : "rgba(255,255,255,.7)",
            border: theme === "dark" ? "1px solid rgba(255,255,255,.08)" : "1px solid rgba(0,0,0,.06)",
            maxWidth: 720,
            fontSize: 13,
            lineHeight: 1.55,
          }}
        >
          <div style={{ fontWeight: 700, marginBottom: 8 }}>Interaction notes (for implementation)</div>
          <ul style={{ margin: 0, paddingLeft: 18, opacity: 0.85 }}>
            <li>
              <strong>Chart hover:</strong> active bar full opacity + outline; others ~28%; vertical guide; tooltip with date / cost / tokens.
            </li>
            <li>
              <strong>Long lists:</strong> cap body ~420pt total panel height; section scroll with top/bottom fades; 3px mono thumb (not native bulky scroller).
            </li>
            <li>
              <strong>Settings:</strong> sheet/window 420×360; same mono type + hairline controls; sections Menu Bar / Scanning / CLI; primary DONE + FULL RESCAN.
            </li>
          </ul>
        </section>
      </main>
    </div>
  );
}

function FilterChip({ active, onClick, theme, children }) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        border: active
          ? theme === "dark"
            ? "1px solid rgba(255,255,255,.35)"
            : "1px solid rgba(0,0,0,.25)"
          : theme === "dark"
            ? "1px solid rgba(255,255,255,.12)"
            : "1px solid rgba(0,0,0,.1)",
        background: active ? (theme === "dark" ? "rgba(255,255,255,.14)" : "rgba(0,0,0,.08)") : "transparent",
        color: "inherit",
        borderRadius: 999,
        padding: "6px 12px",
        fontSize: 12,
        fontWeight: 650,
        cursor: "pointer",
      }}
    >
      {children}
    </button>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<App />);
