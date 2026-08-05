# Kimi Code Usage Identity Restoration

## Goal

Restore concrete model and provider identity for Kimi Code usage records that contain runtime routing aliases such as `__secondary__`, before those records reach aggregation, pricing, or the Menu Bar UI.

The required observed conversion is:

```text
usage.record model = "__secondary__"
matched llm.request modelAlias = "__secondary__"
matched llm.request model = "grok-4.5"
matched llm.request provider = "openai"
=> modelId = "grok-4.5", providerId = "xai"
```

## Scope

- Implement restoration in the Kimi Code parser.
- Preserve token totals, timestamps, message counts, scope filtering, and deduplication behavior.
- Restore provider ownership strongly enough for existing pricing lookup to calculate cost.
- Reparse unchanged historical Kimi files by invalidating only the Kimi source-message cache.
- Keep aggregation and Swift presentation layers free of Kimi-specific alias rules.

This change does not add Kimi workspace attribution or change Project assignment.

## Primary Evidence

Kimi Code persists `llm.request` before making a provider request and persists `usage.record` after a successful response finishes. The request record contains both the runtime alias and resolved concrete model, while the usage record deliberately stores the alias. Neither persisted event currently contains a shared correlation ID.

Therefore, physical stream order is the strongest available causal evidence. Current configuration is not reliable historical evidence because secondary-model configuration can change during a session, be overridden by environment, or synthesize `__secondary__` only in memory.

The primary-source investigation is recorded in `docs/research/2026-08-04-kimi-code-usage-identity.md`.

## Architecture

### Per-file ordered parser

Process each physical Kimi Code `wire.jsonl` independently in append order. Parser state is local to one invocation and must never be shared between main agents, child agents, sibling agents, sessions, or parallel scanner workers.

For every `llm.request`, retain an unmatched candidate containing:

- runtime `modelAlias`;
- resolved concrete `model`;
- logged provider/protocol hint;
- line order for deterministic matching.

For every `usage.record`, attempt correlation before deciding whether the record will be emitted.

### Alias correlation

When the usage model is a recognized routing alias such as `__secondary__`:

1. Search backward within the same file for the nearest unmatched request whose alias exactly equals the usage model.
2. Use JSONL line order, not timestamps.
3. Consume the matched request and retire older pending requests that would require crossing the completed request/usage pair.
4. Use the request's concrete model as the usage model.

This LIFO behavior selects the latest retry candidate while preventing an older failed request from being revived by a later usage record.

If no reliable same-stream match exists, keep the routing alias and use provider `unknown`. Do not infer identity from another file or from current global configuration.

### Concrete usage models

When a usage record already contains a non-routing model identifier, retain its normalized model name. A same-alias request may provide provider evidence, but ordinary usage model names are not replaced unnecessarily.

### Filtering order

Correlation and candidate consumption happen before existing filters:

1. correlate request and usage;
2. consume/retire request state;
3. apply `usageScope == "turn"`;
4. apply the existing zero-token omission;
5. construct the usage message.

This prevents ignored session-scope or zero-token records from leaving stale requests that could mislabel later usage.

`step.end` records remain ignored as duplicate/non-authoritative usage.

## Provider Resolution

The `llm.request.provider` value is a wire protocol hint, not always the commercial model owner. Resolve reporting provider in this order:

1. existing strong model-family inference from the resolved concrete model;
2. canonicalized logged provider only when it does not conflict with stronger model ownership evidence;
3. `unknown` when ownership remains ambiguous.

Required behavior includes:

- Grok models → `xai`, even when logged protocol is `openai`;
- GPT models → `openai`;
- Claude models → `anthropic`;
- Gemini models → `google`;
- Kimi/Moonshot models → repository canonical provider `moonshotai`;
- unknown custom model over OpenAI-compatible protocol → `unknown`, not automatically `openai`.

Legacy and Kimi Code Moonshot usage should use the same canonical `moonshotai` provider identity.

## Pricing

No pricing implementation change is required. Existing pricing lookup consumes model and provider after parsing. Once `__secondary__ / moonshot` becomes `grok-4.5 / xai`, the normal pricing path can resolve the applicable rate.

The parser must not set or estimate cost directly.

## Cache Invalidation

Increment only the Kimi source-message parser version. This invalidates cached Kimi shards and reparses unchanged historical `wire.jsonl` files without evicting other clients or changing the serialized message layout.

Do not bump the global cache format or public usage report schema.

Menu Bar startup, timer refresh, and manual refresh already request a scan; the Kimi parser-version mismatch will therefore rebuild Kimi identities and the aggregated snapshot. `--force-rescan` continues to provide full cache and snapshot replacement.

A separate usage-snapshot epoch is out of scope because the Menu Bar refresh path already self-heals and the public report schema is unchanged.

## Error Handling and Fallbacks

- Malformed JSON lines remain skipped using existing parser behavior.
- Requests lacking a nonempty alias or concrete model are not correlation candidates.
- Unmatched routing aliases remain visible as aliases with provider `unknown`; the parser does not guess.
- Truncated files beginning with an alias usage remain unresolved unless future event-time metadata provides an exact mapping.
- Truncated files ending with a request produce no fabricated usage.
- Main and subagent files never share pending candidates.
- Same-millisecond events use deterministic line order.

## Testing

Add parser-boundary Rust tests using temporary files with the real Kimi Code path shape.

Minimum required cases:

1. `__secondary__` paired with `grok-4.5` over OpenAI protocol becomes `grok-4.5 / xai`.
2. A retry sequence uses the latest matching request.
3. Completing a pair retires older candidates so later usage cannot cross the pair.
4. A zero-token usage consumes its request but emits no message.
5. Main and child agent files cannot contaminate each other's correlation state.
6. Session-scope and `step.end` records remain uncounted.
7. An unknown custom model over OpenAI protocol remains provider `unknown`.
8. Concrete Kimi usage uses canonical provider `moonshotai`.
9. Existing legacy Kimi parsing remains correct.
10. Kimi cache parser version is incremented and other client versions remain unchanged.

Verification must include:

- focused Kimi parser tests;
- complete Rust workspace tests for affected crates;
- complete Swift tests;
- release CLI and Menu Bar builds;
- a forced or refreshed live usage report showing no `__secondary__` model bucket and a restored `grok-4.5 / xai` bucket;
- release Menu Bar restart and visual confirmation in the Model section.

## Files Expected to Change

- Kimi parser and its inline tests.
- Kimi source-message parser version.
- Research, design, and implementation-plan documentation.
- Existing PR description after verification.

No production changes are expected in aggregation, pricing, usage-report serialization, or Swift UI code.

## Non-Goals

- Hiding Kimi usage.
- Hardcoding `__secondary__` to one configured model.
- Reading today's Kimi configuration as primary historical evidence.
- Correlating requests and usage across files by timestamp.
- Adding project/workspace attribution.
- Changing token accounting or deduplication.
- Adding externally persisted correlation provenance.
