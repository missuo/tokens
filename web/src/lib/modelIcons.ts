/**
 * Maps a model id onto a provider brand icon.
 *
 * Ported from the iOS app's `Brand.icon(forModel:)` so the same model shows the
 * same mark on both surfaces. Model ids arrive in many shapes — bare
 * (`claude-opus-4-6`), namespaced (`anthropic/claude-opus-4-6`), gateway-routed
 * (`accounts/fireworks/routers/kimi-k2`) — so matching is by substring against
 * an ordered rule list, most specific first.
 */

/** A gateway prefix names the host that served the model, and the host wins. */
const HOST_PREFIXES: ReadonlyArray<readonly [string, string]> = [
  ["accounts/fireworks", "fireworks"],
  ["@cf/", "cloudflare"],
];

/** Ordered most-specific first; the first match wins. */
const RULES: ReadonlyArray<{ needles: readonly string[]; icon: string }> = [
  { needles: ["composer", "cursor"], icon: "cursor" },
  { needles: ["claude", "anthropic", "fable", "sonnet", "opus", "haiku"], icon: "claude" },
  { needles: ["gpt", "codex", "openai", "o1-", "o3-"], icon: "openai" },
  { needles: ["gemma"], icon: "gemma" },
  { needles: ["gemini"], icon: "gemini" },
  { needles: ["deepseek"], icon: "deepseek" },
  { needles: ["kimi", "moonshot", "k2p", "k2-", "k2.", "k3"], icon: "kimi" },
  { needles: ["glm", "chatglm", "z-ai", "zai"], icon: "zai" },
  { needles: ["grok", "x-ai", "xai"], icon: "grok" },
  { needles: ["qwen"], icon: "qwen" },
  { needles: ["minimax"], icon: "minimax" },
  { needles: ["mimo", "xiaomi"], icon: "xiaomimimo" },
  { needles: ["llama"], icon: "meta" },
  { needles: ["mistral", "magistral", "devstral"], icon: "mistral" },
  { needles: ["nemotron", "nvidia"], icon: "nvidia" },
  { needles: ["step-", "stepfun"], icon: "stepfun" },
  { needles: ["hy3", "hunyuan"], icon: "hunyuan" },
  { needles: ["longcat"], icon: "longcat" },
  { needles: ["ling-", "inclusionai"], icon: "antgroup" },
  { needles: ["arcee", "trinity"], icon: "arcee" },
  { needles: ["swe-1", "windsurf"], icon: "windsurf" },
  { needles: ["doubao", "ark-"], icon: "doubao" },
  { needles: ["ernie", "wenxin", "cobuddy"], icon: "wenxin" },
  { needles: ["nous", "hermes"], icon: "nousresearch" },
  { needles: ["perplexity", "sonar"], icon: "perplexity" },
  // Hosting providers last — only reached when no model family matched.
  { needles: ["openrouter"], icon: "openrouter" },
  { needles: ["fireworks"], icon: "fireworks" },
  { needles: ["cloudflare"], icon: "cloudflare" },
  { needles: ["bedrock"], icon: "bedrock" },
  { needles: ["azure"], icon: "azure" },
  { needles: ["aws"], icon: "aws" },
  { needles: ["baidu"], icon: "baidu" },
  { needles: ["bytedance"], icon: "bytedance" },
  { needles: ["volcengine"], icon: "volcengine" },
  { needles: ["ollama", ":mlx"], icon: "ollama" },
];

/** Ids the backend emits as bookkeeping rather than real model usage. */
export function isSyntheticModel(model: string): boolean {
  const id = model.toLowerCase();
  return id === "<synthetic>" || id === "unknown" || id === "legacy";
}

/**
 * Returns the icon slug for a model, or `null` when nothing matches. Callers
 * must render a fallback rather than a gap — a row with a missing mark reads
 * as broken, not as "unknown provider".
 */
export function modelIconSlug(model: string): string | null {
  const id = model.toLowerCase();

  for (const [prefix, icon] of HOST_PREFIXES) {
    if (id.startsWith(prefix)) return icon;
  }
  for (const rule of RULES) {
    if (rule.needles.some((needle) => id.includes(needle))) return rule.icon;
  }
  return null;
}
