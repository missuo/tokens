# Restoring Kimi Code `usage.record` Model and Provider Identity

**Date:** 2026-08-04
**Scope:** Research only; no production-code changes.
**Kimi Code version inspected:** installed `~/.kimi-code/bin/kimi --version` = `0.32.0`; matching official [Kimi Code 0.32.0 release](https://github.com/MoonshotAI/kimi-code/releases/tag/%40moonshot-ai/kimi-code%400.32.0) at commit [`4ac7240f`](https://github.com/MoonshotAI/kimi-code/commit/4ac7240fff595b41a94a63c4b4ca74840ad95cf8), wire protocol manifest version 1.5.[13]

## Recommendation

Resolve a routed `usage.record.model`, whether a synthesized value such as `__secondary__` or an arbitrary alias such as `cheap`, from the **nearest preceding, still-unmatched `llm.request` in the same physical agent `wire.jsonl` whose `modelAlias` equals the usage record's model**. Use JSONL line order, not timestamps. When that exact same-alias request carries a differing concrete model, use the concrete request model; without a reliable match, retain the normalized recorded value. Kimi Code emits `llm.request` before making the provider request and emits `usage.record` only after the stream finishes successfully; the usage record deliberately stores the request's alias, while the request event stores both the alias and resolved concrete model. There is no shared request/usage ID in either persisted schema, so ID correlation is not presently available.[1][2][3]

For `llm.request { provider: "openai", model: "grok-4.5" }`, canonicalize the usage provider to `xai`, not `openai`: Kimi Code's `provider` field is populated from the resolved model's **wire protocol**, not its commercial vendor. The official tests state this distinction directly, and Kimi's supported configuration imports xAI as an OpenAI-compatible provider. Prefer event-time catalog/provider ID or xAI base URL when available, then the resolved `grok*` model family; this repository already canonicalizes `grok*` as `xai`.[3][11][14][15]

## Established Event Semantics

### Persisted schemas

Kimi Code's persisted `llm.request` schema contains `provider`, concrete `model`, optional `modelAlias`, optional `turnStep` and `attempt`, but no request ID.[1] Its persisted `usage.record` schema contains exactly `model`, `usage`, and optional `usageScope`; the source explicitly says this is the exact persisted field set and that legacy extra fields are ignored. An official test further states that turn-scoped records persist `usageScope` only, with no turn ID or context. It has no request ID, usage ID, provider, `turnStep`, agent ID, or profile name.[2][16]

The request service resolves a model alias to a requester and concrete model, writes `llm.request` with:

- `provider: input.protocol`
- `model: input.modelName`
- `modelAlias: input.modelAlias`
- optional request-only `turnStep` and `attempt`

It then performs the streaming request. Only after receiving a finish event does it call `usage.record(request.modelAlias, ...)`.[3] The usage service persists that alias unchanged and derives `usageScope` from whether the request source is a turn or session operation.[4]

Therefore, for a successful call, the durable causal sequence is:

```text
llm.request(alias=A, concrete model=M, protocol=P)
...zero or more unrelated loop/context events...
usage.record(model=A, usageScope=turn|session)
```

A failed or retried request can produce `llm.request` without a corresponding `usage.record`; recovery attempts record another request before retrying. Consequently, matching should be **last-in-first-out among unmatched requests**, not first-in-first-out.[3]

### `step.end` duplication and protocol generations

Kimi Code also persists the same completion usage inside a nested `context.append_loop_event` whose event is `step.end`. Official source and fixtures show that this duplicates the top-level `usage.record`; consumers must count one representation, preferably `usage.record`, and use `step.end` only for validation or fallback.[17]

The relative placement of `step.end` and `usage.record` differs by engine generation: v2 normally writes `usage.record` before the nested `step.end`, while legacy v1 writes `step.end` before its `afterStep` usage callback. Correlation must therefore not require a fixed ordering between those two forms. The latest eligible preceding `llm.request` remains the stable request-to-usage rule.[18]

### First-party raw artifacts from the installed client

Sanitized inspection of the installed 0.32.0 session artifacts confirmed the source semantics:

- `llm.request` keys included `type`, `kind`, `provider`, `model`, `modelAlias`, `thinkingEffort`, `maxTokens`, `toolSelect`, hashes, `messageCount`, `turnStep`, and `time`.
- `usage.record` keys included only `type`, `model`, `usage`, `usageScope`, and `time`.
- A subagent stream repeatedly contained `llm.request { modelAlias: "__secondary__", model: "grok-4.5", provider: "openai" }`, followed later by `usage.record { model: "__secondary__", usageScope: "turn" }`; the next request commonly began 0–1 ms after the usage record.
- Neither event contained a common request/usage identifier.
- Main and subagent records were physically separated under `agents/main/wire.jsonl` and `agents/agent-N/wire.jsonl`, matching the official data-location contract.[5]

Raw artifact sampled: `~/.kimi-code/sessions/wd_random-things_986fdb618290/session_d0a3b1b2-5c11-4feb-b7e8-23ea92b1b952/agents/agent-1/wire.jsonl`. Only structural fields were inspected; prompt/message content is not reproduced here.

### Alias and subagent semantics

`__secondary__` is a reserved, synthesized in-memory model ID. When `[secondary_model]` carries patch fields, Kimi Code copies the configured base model, merges the patch, drops routing aliases, and inserts the derived entry only into the effective runtime model view; it is stripped from config writes.[6] Subagent binding chooses the explicit spawn preference first, then the profile preference, then the configured secondary model. With patches it binds `__secondary__`; without patches it binds the concrete configured alias.[7][8]

Official docs also establish that:

- `[secondary_model].model` may point to any configured model/provider.
- `/secondary_model` applies changes live to newly spawned subagents.
- environment variables can override the secondary model.
- resumed subagents keep their existing model.
- `model_preference` is only symbolic (`primary` or `secondary`), never a concrete model.[8][10]

These rules make profile and current config useful only as fallbacks. They cannot safely replace per-request evidence. Model aliases are arbitrary routing keys and may be names such as `cheap`; they need not contain a provider prefix or recognizable model family.[19]

## Correlation Strategy Comparison

| Strategy | Reliability | Finding |
| --- | --- | --- |
| Shared request/usage ID | Best in principle; unavailable now | Neither official persisted schema has a correlation ID. Provider response IDs/trace IDs exist elsewhere in request handling but are not written to `usage.record` or `llm.request`.[1][2][3] |
| Same-file event ordering | **Primary strategy** | Official control flow writes a request before the provider call and usage after successful finish. Pair a usage record with the nearest preceding unmatched request having the same alias.[3][4] |
| `turnStep` / attempt metadata | Supporting evidence only | Present on `llm.request`, absent from `usage.record`; useful for diagnostics, retry handling, and fixture assertions, not as a direct join key.[1][2] |
| Agent/profile metadata | Partitioning and low-confidence hint | Agent identity is represented by the separate file path. Profile `model_preference` is symbolic and can be overridden per spawn; resumed agents retain their own model.[5][10] |
| Alias configuration | Last-resort fallback | `__secondary__` can be synthesized only in memory, patched, changed live, or overridden by environment. Reading today's config can mislabel historical records.[6][8] |
| Timestamp proximity | Tie-breaker only | Millisecond timestamps can collide, and clocks are not a stronger contract than append order. Use line ordinal; timestamps may reject impossible matches but should not select them. |

## Recommended Parser Algorithm

Process each `wire.jsonl` independently and in append order.

1. **Partition by physical agent stream.** Use `(session directory, agent directory, wire path)` as the correlation scope. Never carry request state from `agents/main` into `agents/agent-N`, between sibling subagents, or between resumed sessions.
2. **Keep line ordinal.** Parse `time` for the final usage timestamp, but use JSONL ordinal for correlation.
3. **Record request candidates.** For every `llm.request`, retain:
   - line ordinal and time;
   - `kind`, `turnStep`, and `attempt` when present;
   - `modelAlias`;
   - concrete `model`;
   - logged protocol (`provider`);
   - matched/unmatched state.
4. **Correlate every usage record before applying the zero-token filter.** Kimi writes `emptyUsage()` when a provider finishes without reporting usage. A zero-valued `usage.record` must still consume/close its request candidate even if no `UnifiedMessage` is ultimately emitted; otherwise the next nonzero record can be paired to the wrong request.[3]
5. **Preserve unmatched or already-concrete usage.** Without a reliable exact same-alias request, retain the normalized `usage.record.model`. When a matching request carries the same concrete model, resolution naturally preserves it while allowing the request to supply provider evidence.
6. **Resolve arbitrary aliases by causal pairing.** Search backward for the nearest unmatched request where `request.modelAlias == usage.model`. Prefer a request before the usage line; require the same physical stream. Mark it matched and use its concrete `request.model` and protocol whenever the request model differs, regardless of whether the alias is reserved (`__secondary__`) or arbitrary (`cheap`).
   - LIFO handles failed/retried attempts: the latest successful attempt is adjacent to the eventual usage, while completing the pair retires older pending attempts that would cross it.
   - A newer request with the same nonempty alias but no usable concrete model is a barrier and retires older same-alias candidates; later usage must not revive identity from before that unusable request.
   - Do not require timestamp equality or a maximum wall-clock gap; long model calls are legitimate.
7. **Constrain ambiguous cases.** Do not cross an already matched request/usage pair. If nested or concurrent requests ever appear in one stream and two candidates cannot be distinguished by alias plus LIFO, leave the identity unresolved rather than guessing. Preserve diagnostic provenance such as `resolvedFrom = request-order` and the source line numbers.
8. **Fallbacks, in descending confidence:**
   1. A future exact shared request/usage ID, if Kimi adds one, scoped to the same agent stream.
   2. Same-stream LIFO alias match described above.
   3. An event-time model/config snapshot explicitly present in the wire stream and uniquely resolving the alias.
   4. The session-start effective `[secondary_model]` mapping, including environment overlay, only when it is known to correspond to that session and no live change occurred.
   5. Profile `model_preference` or agent type only to choose between already known `primary`/`secondary` candidates, never to invent a concrete model.
   6. Leave the routing alias intact with provider `unknown` and an `unresolved_alias` provenance flag.
9. **Retain scope/dedup behavior.** Treat top-level `usage.record` as authoritative. If nested `step.end.event.usage` is parsed, use it only for validation or fallback and never add both. For strict turn totals, require explicit `usageScope: "turn"`; current writers emit `turn` or `session`, but the schema remains optional for compatibility, so an omitted scope cannot prove turn usage. Identity restoration must not alter token totals.[2][17]

### Provider canonicalization

Treat `llm.request.provider` as a **wire protocol hint**, not the model owner, because the recorder assigns it from `input.protocol`; the official protocol type intentionally contains protocol families rather than vendor identities.[3][14] Determine the reporting provider after resolving the concrete model:

1. Explicit event-time model catalog/provider ID or vendor base URL, when available (`xai`, `https://api.x.ai/v1`, etc.).
2. Explicit owner namespace in the resolved model identifier.
3. Strong model-family inference (`grok*` → `xai`, `claude*` → `anthropic`, `gemini*` → `google`, `kimi*` → the repository's Moonshot canonical ID).
4. Use a protocol label only when independent model evidence identifies the same owner (for example `gpt-*` plus protocol `openai`). Never infer commercial provider OpenAI from `llm.request.provider == "openai"` alone.
5. `unknown` when ownership remains ambiguous.

Required case:

```text
usage.record model = "__secondary__"
matched llm.request modelAlias = "__secondary__"
matched llm.request model = "grok-4.5"
matched llm.request provider = "openai"
=> modelId = "grok-4.5", providerId = "xai"
```

The `openai` value describes the OpenAI-compatible protocol. Reporting it as the owner would group and price a Grok model under the wrong provider. The repository's canonicalizer already maps `x_ai`/`xai` to `xai` and infers models containing `grok` as `xai`.[11]

## Main/Subagent Edge Cases

- **Main and child use different models:** safe when state is per file; unsafe if a session-global “last request” is used.
- **Sibling subagents run concurrently:** their timestamps interleave across files, but their line streams do not. Never merge before correlation.
- **Explicit `model: "primary"` spawn:** profile or secondary config does not prove the child used secondary; the child's own request does.
- **Profile says `secondary`, tool call overrides `primary`:** request event wins.
- **Secondary model changed live:** only later spawned subagents use the new binding; current config cannot relabel earlier child files.[8]
- **Resumed subagent:** it retains its model; correlate within its persisted agent stream rather than applying the main agent's current model.[10]
- **Patched secondary model:** usage may store `__secondary__`; config on disk intentionally lacks that derived entry.[6]
- **Failed request or recovery retry:** may leave unmatched request events. LIFO matching chooses the later successful attempt; a newer unusable same-alias request is a barrier that retires older same-alias candidates.
- **Truncated file starts with usage:** use a session-specific config fallback if provable; otherwise preserve unresolved alias.
- **Truncated file ends after request:** emit no usage record; do not fabricate usage.
- **Zero-token usage:** it must consume its matched request before being omitted from counted messages; otherwise request state drifts.
- **Session-scoped compaction:** it is still per-request usage, not a cumulative snapshot, but exclude it from strict turn totals when `usageScope: "session"`.
- **Missing `usageScope`:** classify conservatively as session/unknown-compatible; do not assume turn scope.
- **V1/v2 `step.end` placement:** accept either side of `usage.record`; never count both representations.
- **Same-millisecond events:** line order remains deterministic.

## Recommended Test Fixtures

Add small JSONL fixtures at the parser boundary, each with expected model/provider, token totals, and correlation provenance:

1. Concrete `usage.record` with matching concrete request remains concrete.
2. `__secondary__` usage paired to `grok-4.5` / protocol `openai` → `grok-4.5` / `xai`.
3. Arbitrary `cheap` alias paired to a differing concrete model restores that model.
4. Two sequential alias requests/usages → one-to-one LIFO matches.
5. Failed request, retry request, then usage → retry model selected; a newer unusable same-alias request blocks older candidates.
6. Main file uses Kimi while child file uses Grok with identical timestamps → no cross-agent contamination.
7. Two sibling agent files use different secondary models concurrently.
8. Child profile prefers secondary but explicit spawn uses primary → request wins.
9. Resumed child retains old model after main/secondary config changes.
10. Patched secondary emits `__secondary__` while disk config contains only the base alias.
11. Truncated leading alias usage with a provable session-start mapping → low-confidence config fallback.
12. Truncated leading alias usage with changed/unknown config → unresolved alias, provider `unknown`.
13. Ambiguous same-stream candidates → unresolved rather than arbitrary match.
14. `usageScope: "session"`, missing scope, and duplicated `step.end` do not change strict turn totals.
15. Both v1 (`step.end` before usage) and v2 (`usage` before `step.end`) fixture ordering count exactly once.
16. Same timestamp for usage and next request → line order pairs usage to the preceding request.
17. Zero-valued usage closes its request, then is omitted; the next usage matches the next request.
18. Unknown custom model over protocol `openai` → provider `unknown` unless event-time catalog/config identifies the vendor.

## Pinned Pre-fix Repository Gap

At base commit [`fe499bd7314934af4cf36724c81e177f8f74197e`](https://github.com/HuaileiW/tokens/commit/fe499bd7314934af4cf36724c81e177f8f74197e), before this restoration, the parser read only `usage.record`, normalized its `model` field, and always supplied the fixed provider `moonshot`; it did not deserialize or correlate `llm.request`.[12] Thus `__secondary__` remained synthetic and a resolved `grok-4.5` call was incorrectly attributed to Moonshot. The restored parser performs correlation inside the Kimi Code parser as a single ordered pass before constructing `UnifiedMessage`; aggregation and production UI code need no special alias logic.

## Sources

1. MoonshotAI Kimi Code, persisted `llm.request` schema: [`llmRequestOps.ts` lines 52–74](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/agent/llmRequester/llmRequestOps.ts#L52-L74).
2. MoonshotAI Kimi Code, exact persisted `usage.record` field set and schema: [`usageOps.ts` lines 1–12, 52–67](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/agent/usage/usageOps.ts#L1-L12) and [`#L52-L67`](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/agent/usage/usageOps.ts#L52-L67).
3. MoonshotAI Kimi Code, request-before-stream, usage-after-finish, alias recording, and request payload construction: [`llmRequesterService.ts` lines 380–443](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/agent/llmRequester/llmRequesterService.ts#L380-L443) and [`#L661-L704`](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/agent/llmRequester/llmRequesterService.ts#L661-L704).
4. MoonshotAI Kimi Code, usage scope and persisted alias: [`usageService.ts` lines 77–94](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/agent/usage/usageService.ts#L77-L94).
5. MoonshotAI Kimi Code official data locations for main and subagent wire files: [`data-locations.md` lines 71–84](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/docs/en/configuration/data-locations.md#L71-L84).
6. MoonshotAI Kimi Code, `__secondary__` synthesized overlay semantics: [`secondaryModelOverlay.ts` lines 1–28, 43–99](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/app/kosongConfig/secondaryModelOverlay.ts#L1-L28) and [`#L43-L99`](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/app/kosongConfig/secondaryModelOverlay.ts#L43-L99).
7. MoonshotAI Kimi Code, subagent model-binding precedence and derived alias selection: [`configSection.ts` lines 104–146](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/session/subagent/configSection.ts#L104-L146).
8. MoonshotAI Kimi Code official secondary-model configuration: [`config-files.md` lines 193–222](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/docs/en/configuration/config-files.md#L193-L222).
9. MoonshotAI Kimi Code official provider/model configuration semantics: provider `type` includes `openai`, while model aliases separately identify provider and server model: [`config-files.md` lines 122–163](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/docs/en/configuration/config-files.md#L122-L163).
10. MoonshotAI Kimi Code official agent profile model-preference and resume semantics: [`agents.md` lines 97–114](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/docs/en/customization/agents.md#L97-L114).
11. This repository's provider canonicalization conventions: [`cli/tokens-core/src/provider_identity.rs` lines 11–23 and 124–152](../../cli/tokens-core/src/provider_identity.rs).
12. This repository's pre-fix Kimi Code parser at base `fe499bd7314934af4cf36724c81e177f8f74197e`: [`cli/tokens-core/src/sessions/kimi.rs` lines 153–232](https://github.com/HuaileiW/tokens/blob/fe499bd7314934af4cf36724c81e177f8f74197e/cli/tokens-core/src/sessions/kimi.rs#L153-L232).
13. MoonshotAI Kimi Code 0.32.0 wire protocol manifest version 1.5: [`wire-manifest.d.ts` lines 1–16](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/docs/wire-manifest.d.ts#L1-L16).
14. MoonshotAI Kimi Code, official statement that durable `provider` is the wire protocol and protocol enum definitions: [`llmRequester.test.ts` lines 140–157](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/test/agent/llmRequester/llmRequester.test.ts#L140-L157) and [`protocol.ts` lines 1–18, 32–39](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/kosong/protocol/protocol.ts#L1-L18).
15. MoonshotAI Kimi Code, xAI imported as an OpenAI-compatible provider with Grok models: [`providers.md` lines 22–35](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/docs/en/configuration/providers.md#L22-L35) and [`provider.test.ts` lines 1003–1017, 1033–1064](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/apps/kimi-code/test/cli/provider.test.ts#L1003-L1017).
16. MoonshotAI Kimi Code, `usage.record` deliberately persists scope only, without turn ID/context: [`usage.test.ts` lines 150–198](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/test/agent/usage/usage.test.ts#L150-L198).
17. MoonshotAI Kimi Code, duplicate top-level `usage.record` and nested `step.end` usage: [`llmRequesterService.ts` lines 418–437](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/agent/llmRequester/llmRequesterService.ts#L418-L437), [`loopService.ts` lines 953–982](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/agent/loop/loopService.ts#L953-L982), and the [official fixture](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/apps/vis/server/test/fixtures/sessions/sample-main/agents/main/wire.jsonl#L7-L10).
18. MoonshotAI Kimi Code, legacy v1 `step.end` before `afterStep` usage recording: [`turn-step.ts` lines 392–441](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core/src/loop/turn-step.ts#L392-L441) and [`turn/index.ts` lines 878–882](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core/src/agent/turn/index.ts#L878-L882).
19. MoonshotAI Kimi Code, arbitrary configured model IDs, wire-facing names, and aliases: [`model.ts` lines 10–27](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/kosong/model/model.ts#L10-L27) and [`catalogService.ts` lines 147–161, 389–413](https://github.com/MoonshotAI/kimi-code/blob/4ac7240fff595b41a94a63c4b4ca74840ad95cf8/packages/agent-core-v2/src/kosong/model/catalogService.ts#L147-L161).
