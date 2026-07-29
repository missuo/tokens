/* Interaction designs for FINAL 06 · Minimal Mono v2
   - Hover on cost chart day
   - Long-list scroll state
   - Settings panel
*/

function monoTheme(theme) {
  const dark = theme === "dark";
  return {
    dark,
    panel: dark ? "#111111" : "#fafafa",
    text: dark ? "#f2f2f2" : "#111111",
    muted: dark ? "#888" : "#777",
    faint: dark ? "#555" : "#aaa",
    line: dark ? "#2a2a2a" : "#e6e6e6",
    accent: dark ? "#f2f2f2" : "#111111",
    track: dark ? "#2a2a2a" : "#ebebeb",
    fill: dark ? "#f2f2f2" : "#111111",
    card: dark ? "rgba(255,255,255,.04)" : "rgba(0,0,0,.03)",
    cardBorder: dark ? "rgba(255,255,255,.12)" : "rgba(0,0,0,.1)",
    tooltipBg: dark ? "#1c1c1c" : "#fff",
    tooltipBorder: dark ? "rgba(255,255,255,.16)" : "rgba(0,0,0,.12)",
    tooltipShadow: dark ? "0 10px 28px rgba(0,0,0,.55)" : "0 10px 28px rgba(0,0,0,.12)",
    scrollThumb: dark ? "rgba(255,255,255,.28)" : "rgba(0,0,0,.22)",
    scrollTrack: "transparent",
    fadeTop: dark ? "linear-gradient(#111, transparent)" : "linear-gradient(#fafafa, transparent)",
    fadeBottom: dark ? "linear-gradient(transparent, #111)" : "linear-gradient(transparent, #fafafa)",
    font: '"IBM Plex Mono","SF Mono",ui-monospace,Menlo,monospace',
    cardTops: dark
      ? ["#f2f2f2", "rgba(255,255,255,.72)", "rgba(255,255,255,.48)", "rgba(255,255,255,.28)"]
      : ["#111", "rgba(0,0,0,.72)", "rgba(0,0,0,.48)", "rgba(0,0,0,.28)"],
  };
}

function SectionLabel({ t, children, right }) {
  return (
    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 12 }}>
      <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", textTransform: "uppercase" }}>{children}</div>
      {right ? <div style={{ fontSize: 10, color: t.muted, fontVariantNumeric: "tabular-nums" }}>{right}</div> : null}
    </div>
  );
}

function MiniHeader({ t, titleRight = "usage · local" }) {
  return (
    <div style={{ padding: "16px 18px 4px" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
        <div style={{ fontSize: 11, letterSpacing: "0.14em", textTransform: "uppercase" }}>Tokens</div>
        <div style={{ fontSize: 10, color: t.muted }}>{titleRight}</div>
      </div>
    </div>
  );
}

function PeriodTabs({ t, active = "7d" }) {
  return (
    <div style={{ padding: "14px 18px 6px", display: "flex" }}>
      {PERIODS.map((p) => {
        const on = p === active;
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
  );
}

function FooterBar({ t, highlight }) {
  const item = (label, key) => (
    <span style={{ color: highlight === key ? t.text : t.muted, fontWeight: highlight === key ? 700 : 400 }}>{label}</span>
  );
  return (
    <div style={{ padding: "10px 18px 14px", display: "flex", fontSize: 11, letterSpacing: "0.04em", color: t.muted, flexWrap: "wrap", gap: 10 }}>
      <span>UPDATED 12M · INC</span>
      <span style={{ marginLeft: "auto" }}>{item("REFRESH", "refresh")}</span>
      {item("SETTINGS", "settings")}
      {item("TOKENS.CI", "site")}
      {item("QUIT", "quit")}
    </div>
  );
}

function PanelShell({ theme, children, screenLabel, width = PANEL_W }) {
  const t = monoTheme(theme);
  return (
    <MenuBarChrome theme={theme} title={MOCK.menuBarTitle} accent={t.accent}>
      <div
        data-screen-label={screenLabel}
        style={{
          width,
          fontFamily: t.font,
          color: t.text,
          background: t.panel,
          borderRadius: 2,
          border: `1px solid ${t.line}`,
          boxShadow: t.dark ? "0 16px 40px rgba(0,0,0,.5)" : "0 12px 32px rgba(0,0,0,.08)",
          position: "relative",
        }}
      >
        {children(t)}
      </div>
    </MenuBarChrome>
  );
}

/* ───────── FINAL rest state (alias of 06, for gallery pinning) ───────── */
function PanelFinalRest({ theme }) {
  // Reuse Panel06 if available
  if (typeof Panel06MinimalV2 === "function") {
    return <Panel06MinimalV2 theme={theme} />;
  }
  return null;
}

/* ───────── A · Chart day hover ───────── */
function CostChartHover({ theme, days, hoverIndex = 11, height = 128 }) {
  const t = monoTheme(theme);
  const axis = t.dark ? "rgba(255,255,255,.22)" : "rgba(0,0,0,.16)";
  const label = t.dark ? "rgba(255,255,255,.45)" : "rgba(0,0,0,.45)";
  const bar = t.fill;
  const grid = t.dark ? "rgba(255,255,255,.06)" : "rgba(0,0,0,.05)";
  const maxCost = Math.max(...days.map((d) => d.cost), 0.01);
  const yMax = Math.ceil(maxCost);
  const ticks = [0, yMax / 2, yMax];
  const padL = 34;
  const padR = 4;
  const padT = 8;
  const padB = 22;
  const plotH = height - padT - padB;
  const n = days.length;
  const gap = 3;
  const hover = days[hoverIndex];

  // approximate bar center for tooltip placement (percent)
  const barCenterPct = ((hoverIndex + 0.5) / n) * 100;

  return (
    <div style={{ position: "relative", height, width: "100%" }}>
      {ticks.map((v) => {
        const y = padT + plotH * (1 - v / yMax);
        return (
          <div key={v} style={{ position: "absolute", left: 0, right: 0, top: y }}>
            <div
              style={{
                position: "absolute",
                left: 0,
                width: padL - 6,
                textAlign: "right",
                fontSize: 9,
                color: label,
                transform: "translateY(-50%)",
                fontVariantNumeric: "tabular-nums",
              }}
            >
              ${v % 1 === 0 ? v.toFixed(0) : v.toFixed(1)}
            </div>
            <div style={{ marginLeft: padL, height: 1, background: v === 0 ? axis : grid }} />
          </div>
        );
      })}

      {/* Hover guide line */}
      <div
        style={{
          position: "absolute",
          left: `calc(${padL}px + (100% - ${padL + padR}px) * ${hoverIndex / n} + (100% - ${padL + padR}px) / ${n} / 2)`,
          top: padT,
          width: 1,
          height: plotH,
          background: t.dark ? "rgba(255,255,255,.28)" : "rgba(0,0,0,.2)",
          pointerEvents: "none",
        }}
      />

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
        {days.map((d, i) => {
          const h = Math.max(2, (d.cost / yMax) * plotH);
          const on = i === hoverIndex;
          return (
            <div key={d.date} style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", height: "100%", justifyContent: "flex-end" }}>
              <div
                style={{
                  width: "100%",
                  height: h,
                  background: bar,
                  borderRadius: "1px 1px 0 0",
                  opacity: on ? 1 : 0.28,
                  outline: on ? `1px solid ${t.accent}` : "none",
                  outlineOffset: 1,
                }}
              />
            </div>
          );
        })}
      </div>

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
          const show = i === 0 || i === n - 1 || i % 2 === 1 || i === hoverIndex;
          return (
            <div
              key={d.date}
              style={{
                flex: 1,
                textAlign: "center",
                fontSize: 9,
                color: i === hoverIndex ? t.text : label,
                fontWeight: i === hoverIndex ? 700 : 400,
                fontVariantNumeric: "tabular-nums",
                visibility: show ? "visible" : "hidden",
              }}
            >
              {d.label}
            </div>
          );
        })}
      </div>

      {/* Tooltip */}
      <div
        style={{
          position: "absolute",
          left: `clamp(8px, calc(${barCenterPct}% - 70px), calc(100% - 148px))`,
          top: 0,
          width: 140,
          background: t.tooltipBg,
          border: `1px solid ${t.tooltipBorder}`,
          boxShadow: t.tooltipShadow,
          borderRadius: 4,
          padding: "8px 10px",
          zIndex: 5,
          pointerEvents: "none",
        }}
      >
        <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.08em", marginBottom: 6 }}>2026-{hover.date}</div>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, marginBottom: 3 }}>
          <span style={{ color: t.muted }}>cost</span>
          <strong style={{ fontVariantNumeric: "tabular-nums" }}>{fmtCost(hover.cost)}</strong>
        </div>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12 }}>
          <span style={{ color: t.muted }}>tokens</span>
          <span style={{ fontVariantNumeric: "tabular-nums" }}>{fmtTokens(hover.tokens)}</span>
        </div>
        <div
          style={{
            position: "absolute",
            left: "50%",
            bottom: -5,
            width: 8,
            height: 8,
            background: t.tooltipBg,
            borderRight: `1px solid ${t.tooltipBorder}`,
            borderBottom: `1px solid ${t.tooltipBorder}`,
            transform: "translateX(-50%) rotate(45deg)",
          }}
        />
      </div>
    </div>
  );
}

function InteractionHoverDay({ theme }) {
  const hoverIndex = 11; // 07-24 peak day
  return (
    <PanelShell theme={theme} screenLabel="ix-hover-day">
      {(t) => (
        <>
          <MiniHeader t={t} titleRight="hover · day" />
          <PeriodTabs t={t} />
          <div style={{ padding: "18px 18px 8px" }}>
            <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", marginBottom: 6 }}>TOTAL</div>
            <div style={{ fontSize: 28, fontWeight: 500, letterSpacing: "-0.04em", fontVariantNumeric: "tabular-nums" }}>
              {fmtTokens(MOCK.summary.totalTokens)}
            </div>
          </div>
          <div style={{ padding: "0 18px 18px" }}>
            <SectionLabel t={t} right="hover Jul 24">
              Cost · 14 days
            </SectionLabel>
            <CostChartHover theme={theme} days={MOCK.days14} hoverIndex={hoverIndex} height={140} />
            <div style={{ marginTop: 14, fontSize: 11, color: t.muted, lineHeight: 1.45 }}>
              Hover/focus a bar → dim siblings, show date guide + tooltip (cost + tokens).
            </div>
          </div>
          <FooterBar t={t} />
        </>
      )}
    </PanelShell>
  );
}

/* ───────── B · Long list scroll ───────── */
function InteractionScroll({ theme }) {
  return (
    <PanelShell theme={theme} screenLabel="ix-scroll">
      {(t) => (
        <>
          <MiniHeader t={t} titleRight="scroll · list" />
          <PeriodTabs t={t} active="All" />

          <div style={{ padding: "16px 18px 10px" }}>
            <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", marginBottom: 6 }}>TOTAL</div>
            <div style={{ fontSize: 28, fontWeight: 500, letterSpacing: "-0.04em", fontVariantNumeric: "tabular-nums" }}>16.2M</div>
            <div style={{ marginTop: 8, fontSize: 10, color: t.muted }}>2026-06-01 — 2026-07-26 · all time</div>
          </div>

          {/* Scroll region: fixed viewport with fade + thin mono scrollbar */}
          <div style={{ padding: "0 0 0 18px", position: "relative" }}>
            <SectionLabel t={t} right="12 clients">
              Client
            </SectionLabel>

            <div style={{ position: "relative", marginRight: 8 }}>
              {/* top fade — content scrolled mid-list */}
              <div
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  right: 10,
                  height: 18,
                  background: t.fadeTop,
                  zIndex: 2,
                  pointerEvents: "none",
                }}
              />
              <div
                style={{
                  position: "absolute",
                  bottom: 0,
                  left: 0,
                  right: 10,
                  height: 22,
                  background: t.fadeBottom,
                  zIndex: 2,
                  pointerEvents: "none",
                }}
              />

              <div
                style={{
                  maxHeight: 168,
                  overflow: "hidden",
                  position: "relative",
                  // simulate scrolled position by negative margin on inner
                }}
              >
                <div style={{ transform: "translateY(-52px)" }}>
                  {MOCK.clientsLong.map((c, i) => (
                    <div key={c.name} style={{ marginBottom: 11, paddingRight: 14, opacity: i < 2 || i > 7 ? 0.35 : 1 }}>
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
              </div>

              {/* custom thin scrollbar (always-visible mono style) */}
              <div
                style={{
                  position: "absolute",
                  top: 4,
                  right: 0,
                  bottom: 4,
                  width: 3,
                  background: t.dark ? "rgba(255,255,255,.06)" : "rgba(0,0,0,.05)",
                  borderRadius: 2,
                }}
              >
                <div
                  style={{
                    position: "absolute",
                    top: "22%",
                    left: 0,
                    width: 3,
                    height: "28%",
                    background: t.scrollThumb,
                    borderRadius: 2,
                  }}
                />
              </div>
            </div>

            <div style={{ fontSize: 10, color: t.muted, marginTop: 10, marginBottom: 18, letterSpacing: "0.04em" }}>
              SCROLL · 3 / 12 VISIBLE · FADE EDGES
            </div>
          </div>

          {/* Model section also indicates overflow */}
          <div style={{ padding: "0 18px 8px" }}>
            <SectionLabel t={t} right="10 models · scroll">
              Model
            </SectionLabel>
            <div style={{ position: "relative" }}>
              <div style={{ maxHeight: 96, overflow: "hidden" }}>
                {MOCK.modelsLong.slice(0, 5).map((m, i) => (
                  <div
                    key={m.name}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      fontSize: 12,
                      marginBottom: 8,
                      fontVariantNumeric: "tabular-nums",
                      opacity: i >= 4 ? 0.35 : 1,
                    }}
                  >
                    <span>
                      {m.name} <span style={{ color: t.muted }}>/ {m.provider}</span>
                    </span>
                    <span>{fmtTokens(m.tokens)}</span>
                  </div>
                ))}
              </div>
              <div style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: 20, background: t.fadeBottom, pointerEvents: "none" }} />
            </div>
          </div>

          <FooterBar t={t} />
        </>
      )}
    </PanelShell>
  );
}

/* ───────── C · Settings ───────── */
function SettingsRow({ t, label, children }) {
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12, marginBottom: 14 }}>
      <div style={{ fontSize: 12, color: t.muted, letterSpacing: "0.04em" }}>{label}</div>
      <div style={{ fontSize: 12, color: t.text }}>{children}</div>
    </div>
  );
}

function Segmented({ t, options, value }) {
  return (
    <div
      style={{
        display: "inline-flex",
        border: `1px solid ${t.cardBorder}`,
        borderRadius: 2,
        overflow: "hidden",
      }}
    >
      {options.map((opt) => {
        const on = opt === value;
        return (
          <div
            key={opt}
            style={{
              padding: "5px 9px",
              fontSize: 11,
              letterSpacing: "0.03em",
              background: on ? t.accent : "transparent",
              color: on ? (t.dark ? "#111" : "#fff") : t.muted,
              fontWeight: on ? 700 : 400,
            }}
          >
            {opt.toUpperCase()}
          </div>
        );
      })}
    </div>
  );
}

function InteractionSettings({ theme }) {
  const s = MOCK.settings;
  return (
    <PanelShell theme={theme} screenLabel="ix-settings" width={420}>
      {(t) => (
        <>
          {/* Window-like settings chrome still mono */}
          <div style={{ padding: "14px 18px 6px", display: "flex", alignItems: "baseline", justifyContent: "space-between" }}>
            <div>
              <div style={{ fontSize: 11, letterSpacing: "0.14em", textTransform: "uppercase" }}>Settings</div>
              <div style={{ fontSize: 10, color: t.muted, marginTop: 4 }}>tokens menu bar</div>
            </div>
            <div
              style={{
                fontSize: 11,
                letterSpacing: "0.08em",
                color: t.text,
                border: `1px solid ${t.cardBorder}`,
                padding: "5px 10px",
                borderRadius: 2,
              }}
            >
              DONE
            </div>
          </div>

          <div style={{ padding: "18px 18px 8px" }}>
            <SectionLabel t={t}>Menu Bar</SectionLabel>
            <SettingsRow t={t} label="DISPLAY">
              <Segmented t={t} options={s.displayOptions} value={s.displayMode} />
            </SettingsRow>
            <div style={{ fontSize: 10, color: t.faint, marginTop: -6, marginBottom: 18, lineHeight: 1.4 }}>
              Title in status item: tokens only / cost only / both
            </div>

            <SectionLabel t={t}>Scanning</SectionLabel>
            <SettingsRow t={t} label="INTERVAL">
              <div
                style={{
                  border: `1px solid ${t.cardBorder}`,
                  borderRadius: 2,
                  padding: "5px 10px",
                  fontSize: 11,
                  letterSpacing: "0.03em",
                  minWidth: 110,
                  textAlign: "right",
                  fontVariantNumeric: "tabular-nums",
                }}
              >
                {s.scanInterval.toUpperCase()} ▾
              </div>
            </SettingsRow>
            <div
              style={{
                marginBottom: 18,
                padding: "10px 12px",
                border: `1px solid ${t.cardBorder}`,
                borderRadius: 2,
                background: t.card,
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                gap: 12,
              }}
            >
              <div>
                <div style={{ fontSize: 12, letterSpacing: "0.04em" }}>FULL RESCAN NOW</div>
                <div style={{ fontSize: 10, color: t.muted, marginTop: 3 }}>Ignore caches · rebuild snapshot</div>
              </div>
              <div style={{ fontSize: 11, color: t.muted }}>RUN</div>
            </div>

            <SectionLabel t={t}>CLI</SectionLabel>
            <div style={{ marginBottom: 12 }}>
              <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.08em", marginBottom: 6 }}>RESOLVED PATH</div>
              <div
                style={{
                  fontSize: 11,
                  fontVariantNumeric: "tabular-nums",
                  padding: "8px 10px",
                  background: t.card,
                  border: `1px solid ${t.cardBorder}`,
                  borderRadius: 2,
                  wordBreak: "break-all",
                  lineHeight: 1.45,
                }}
              >
                {s.binaryPath}
              </div>
            </div>
            <SettingsRow t={t} label="VERSION">
              <span style={{ fontVariantNumeric: "tabular-nums" }}>{s.cliVersion}</span>
            </SettingsRow>
            <div style={{ display: "flex", gap: 10, marginTop: 4 }}>
              <div
                style={{
                  flex: 1,
                  textAlign: "center",
                  fontSize: 11,
                  letterSpacing: "0.06em",
                  padding: "9px 0",
                  border: `1px solid ${t.cardBorder}`,
                  borderRadius: 2,
                }}
              >
                RECHECK CLI
              </div>
              <div
                style={{
                  flex: 1,
                  textAlign: "center",
                  fontSize: 11,
                  letterSpacing: "0.06em",
                  padding: "9px 0",
                  border: `1px solid ${t.accent}`,
                  background: t.accent,
                  color: t.dark ? "#111" : "#fff",
                  borderRadius: 2,
                  fontWeight: 700,
                }}
              >
                DONE
              </div>
            </div>
          </div>

          <div style={{ padding: "16px 18px 14px", fontSize: 10, color: t.faint, letterSpacing: "0.04em" }}>
            SETTINGS · SHEET · 420×360 · MATCHES FINAL VISUAL LANGUAGE
          </div>
        </>
      )}
    </PanelShell>
  );
}

/* ───────── D · Settings opened from menu (context) ───────── */
function InteractionSettingsFromMenu({ theme }) {
  // Show main panel dimmed + settings floating — composition storyboard
  const t = monoTheme(theme);
  return (
    <div
      data-screen-label="ix-settings-context"
      style={{
        width: 520,
        padding: 24,
        boxSizing: "border-box",
        background: theme === "dark" ? "linear-gradient(160deg,#1a1b22 0%,#0e0f14 60%,#16141c 100%)" : "linear-gradient(160deg,#dfe6f2 0%,#c8d2e4 45%,#b8c4da 100%)",
        borderRadius: 16,
        position: "relative",
      }}
    >
      <div style={{ opacity: 0.35, pointerEvents: "none", transform: "scale(0.92)", transformOrigin: "top center" }}>
        <Panel06MinimalV2 theme={theme} />
      </div>
      <div
        style={{
          position: "absolute",
          left: "50%",
          top: 48,
          transform: "translateX(-50%)",
          filter: "drop-shadow(0 18px 40px rgba(0,0,0,.35))",
        }}
      >
        {/* compact settings card only */}
        <div
          style={{
            width: 380,
            fontFamily: t.font,
            color: t.text,
            background: t.panel,
            borderRadius: 2,
            border: `1px solid ${t.line}`,
            boxShadow: t.dark ? "0 16px 40px rgba(0,0,0,.55)" : "0 12px 32px rgba(0,0,0,.14)",
          }}
        >
          <div style={{ padding: "14px 16px", display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
            <div style={{ fontSize: 11, letterSpacing: "0.14em" }}>SETTINGS</div>
            <div style={{ fontSize: 11, letterSpacing: "0.08em" }}>DONE</div>
          </div>
          <div style={{ padding: "4px 16px 16px" }}>
            <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", marginBottom: 10 }}>MENU BAR</div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16 }}>
              <span style={{ fontSize: 12, color: t.muted }}>DISPLAY</span>
              <Segmented t={t} options={MOCK.settings.displayOptions} value={MOCK.settings.displayMode} />
            </div>
            <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", marginBottom: 10 }}>SCANNING</div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
              <span style={{ fontSize: 12, color: t.muted }}>INTERVAL</span>
              <span style={{ fontSize: 11, border: `1px solid ${t.cardBorder}`, padding: "4px 8px" }}>12 HOURS ▾</span>
            </div>
            <div style={{ fontSize: 11, letterSpacing: "0.05em", padding: "10px", border: `1px solid ${t.cardBorder}`, background: t.card, marginBottom: 14 }}>
              FULL RESCAN NOW
            </div>
            <div style={{ fontSize: 10, color: t.muted, letterSpacing: "0.1em", marginBottom: 8 }}>CLI</div>
            <div style={{ fontSize: 10, fontVariantNumeric: "tabular-nums", color: t.text, wordBreak: "break-all", lineHeight: 1.4 }}>
              {MOCK.settings.binaryPath}
            </div>
          </div>
        </div>
      </div>
      <div style={{ marginTop: 12, textAlign: "center", font: "500 11px/1.4 -apple-system,system-ui,sans-serif", color: theme === "dark" ? "rgba(255,255,255,.55)" : "rgba(0,0,0,.5)" }}>
        Settings opens as a sheet over the menu panel · same mono language
      </div>
    </div>
  );
}

Object.assign(window, {
  PanelFinalRest,
  InteractionHoverDay,
  InteractionScroll,
  InteractionSettings,
  InteractionSettingsFromMenu,
});
