// Shared mock usage data — same payload shape as `tokens usage --json`
const MOCK = {
  period: "7d",
  dateRange: { start: "2026-07-20", end: "2026-07-26" },
  generatedAt: "2026-07-26T12:48:00Z",
  scanMode: "incremental",
  summary: {
    totalTokens: 12400000,
    totalCost: 48.2,
    messages: 1842,
  },
  breakdown: [
    { key: "in", label: "in", value: 6200000 },
    { key: "out", label: "out", value: 3100000 },
    { key: "cache", label: "cache", value: 2800000 },
    { key: "reason", label: "reason", value: 300000 },
  ],
  clients: [
    {
      name: "Claude Code",
      tokens: 6800000,
      cost: 26.4,
      share: 0.55,
      models: [
        { name: "claude-opus-4-8", tokens: 4100000, cost: 18.2, share: 0.6 },
        { name: "claude-sonnet-5", tokens: 2700000, cost: 8.2, share: 0.4 },
      ],
    },
    {
      name: "Cursor",
      tokens: 3200000,
      cost: 12.1,
      share: 0.26,
      models: [{ name: "gpt-5.2", tokens: 3200000, cost: 12.1, share: 1 }],
    },
    {
      name: "Codex CLI",
      tokens: 2400000,
      cost: 9.7,
      share: 0.19,
      models: [{ name: "o4-mini", tokens: 2400000, cost: 9.7, share: 1 }],
    },
  ],
  models: [
    { name: "claude-opus-4-8", provider: "anthropic", tokens: 4100000, cost: 18.2, share: 0.33 },
    { name: "gpt-5.2", provider: "openai", tokens: 3200000, cost: 12.1, share: 0.26 },
    { name: "claude-sonnet-5", provider: "anthropic", tokens: 2700000, cost: 8.2, share: 0.22 },
  ],
  days: [
    { date: "07-26", tokens: 2100000, intensity: 0.9 },
    { date: "07-25", tokens: 1800000, intensity: 0.75 },
    { date: "07-24", tokens: 2400000, intensity: 1 },
    { date: "07-23", tokens: 1500000, intensity: 0.55 },
    { date: "07-22", tokens: 1900000, intensity: 0.7 },
  ],
  // Last 14 days for cost chart (oldest → newest)
  days14: [
    { date: "07-13", label: "13", cost: 2.1, tokens: 980000 },
    { date: "07-14", label: "14", cost: 3.4, tokens: 1200000 },
    { date: "07-15", label: "15", cost: 2.8, tokens: 1100000 },
    { date: "07-16", label: "16", cost: 4.6, tokens: 1650000 },
    { date: "07-17", label: "17", cost: 5.2, tokens: 1900000 },
    { date: "07-18", label: "18", cost: 1.9, tokens: 720000 },
    { date: "07-19", label: "19", cost: 2.4, tokens: 880000 },
    { date: "07-20", label: "20", cost: 3.8, tokens: 1400000 },
    { date: "07-21", label: "21", cost: 4.1, tokens: 1500000 },
    { date: "07-22", label: "22", cost: 3.2, tokens: 1900000 },
    { date: "07-23", label: "23", cost: 2.6, tokens: 1500000 },
    { date: "07-24", label: "24", cost: 5.8, tokens: 2400000 },
    { date: "07-25", label: "25", cost: 4.0, tokens: 1800000 },
    { date: "07-26", label: "26", cost: 4.8, tokens: 2100000 },
  ],
  menuBarTitle: "1.2M · $4.80",
  // Long list for scroll-state mock
  clientsLong: [
    { name: "Claude Code", tokens: 6800000, cost: 26.4, share: 0.42 },
    { name: "Cursor", tokens: 3200000, cost: 12.1, share: 0.2 },
    { name: "Codex CLI", tokens: 2400000, cost: 9.7, share: 0.15 },
    { name: "OpenCode", tokens: 980000, cost: 3.1, share: 0.06 },
    { name: "Gemini CLI", tokens: 720000, cost: 1.8, share: 0.045 },
    { name: "Copilot", tokens: 610000, cost: 2.4, share: 0.038 },
    { name: "Amp", tokens: 440000, cost: 1.2, share: 0.027 },
    { name: "Zed Agent", tokens: 380000, cost: 0.9, share: 0.024 },
    { name: "Kimi", tokens: 290000, cost: 0.7, share: 0.018 },
    { name: "Warp", tokens: 210000, cost: 0.5, share: 0.013 },
    { name: "Roo Code", tokens: 160000, cost: 0.4, share: 0.01 },
    { name: "Cline", tokens: 120000, cost: 0.3, share: 0.007 },
  ],
  modelsLong: [
    { name: "claude-opus-4-8", provider: "anthropic", tokens: 4100000, cost: 18.2, share: 0.25 },
    { name: "gpt-5.2", provider: "openai", tokens: 3200000, cost: 12.1, share: 0.2 },
    { name: "claude-sonnet-5", provider: "anthropic", tokens: 2700000, cost: 8.2, share: 0.17 },
    { name: "o4-mini", provider: "openai", tokens: 2400000, cost: 9.7, share: 0.15 },
    { name: "gemini-2.5-pro", provider: "google", tokens: 980000, cost: 3.1, share: 0.06 },
    { name: "claude-haiku-4-5", provider: "anthropic", tokens: 720000, cost: 1.4, share: 0.045 },
    { name: "gpt-4.1", provider: "openai", tokens: 610000, cost: 2.4, share: 0.038 },
    { name: "kimi-k2", provider: "moonshot", tokens: 440000, cost: 1.1, share: 0.027 },
    { name: "grok-4", provider: "xai", tokens: 310000, cost: 0.9, share: 0.019 },
    { name: "qwen3-coder", provider: "alibaba", tokens: 240000, cost: 0.6, share: 0.015 },
  ],
  settings: {
    displayMode: "Both",
    displayOptions: ["Tokens", "Cost", "Both"],
    scanInterval: "12 hours",
    scanOptions: ["1 hour", "6 hours", "12 hours", "24 hours", "Manual only"],
    binaryPath: "/opt/homebrew/bin/tokens",
    cliVersion: "0.4.2",
  },
};

function fmtTokens(n) {
  if (n >= 1e9) return (n / 1e9).toFixed(1).replace(/\.0$/, "") + "B";
  if (n >= 1e6) return (n / 1e6).toFixed(1).replace(/\.0$/, "") + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(1).replace(/\.0$/, "") + "K";
  return String(n);
}

function fmtCost(n) {
  return "$" + n.toFixed(2);
}

function fmtPct(n) {
  return Math.round(n * 100) + "%";
}

Object.assign(window, { MOCK, fmtTokens, fmtCost, fmtPct });
