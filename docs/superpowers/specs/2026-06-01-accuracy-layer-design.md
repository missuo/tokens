# Accuracy Layer Design

## Goal

Make tokens explain why a usage number is trustworthy, estimated, or likely different from another tool. The user should be able to compare tokens with ClaudeBar, Tokscale, provider dashboards, or tokens.ci without guessing which source or pricing assumption produced a total.

## Current Baseline

The CLI already computes token and cost totals from local client data, applies pricing when available, submits usage to tokens.ci, and now records recent submit outcomes locally. `tokens status --json` is the integration surface for service health and future menu bar/mobile clients.

The missing piece is provenance. A total such as `$12.34` does not currently say whether it came from local logs, a provider API dump, a cached pricing table, a custom override, or server-submitted data.

## Non-Goals

- Do not change counting formulas in the first pass.
- Do not fetch provider billing dashboards automatically.
- Do not make the menu bar or mobile app re-scan raw logs.
- Do not store raw prompts, completions, credentials, or chat content.
- Do not claim provider-billing accuracy when the number is only locally estimated.

## Recommended Approach

Add an additive provenance layer to existing CLI JSON outputs before changing UI. This keeps existing totals stable while giving future surfaces enough context to explain confidence and differences.

Two alternatives were considered:

- CLI-only labels: quickest, but future server/web consumers would have to recreate the model.
- Server-only labels: useful for team dashboards, but cannot explain local-only numbers before submission.
- Core provenance model: recommended, because local CLI, submit payloads, status output, menu bar, and web dashboard can share one vocabulary.

## Vocabulary

### Source Kind

- `local-scan`: parsed from local client files or local databases.
- `provider-official`: fetched from an official provider usage API.
- `submitted-server`: accepted by tokens.ci from a previous submit.
- `estimated-pricing`: cost derived from a pricing table rather than a provider bill.
- `custom-pricing`: cost derived from a user override.
- `unknown`: source is missing or cannot be classified.

### Confidence

- `high`: token counts or costs come from provider-official data, or from a local source with explicit token counters and fresh pricing.
- `medium`: local token counts are explicit, but cost uses cached or aliased pricing.
- `low`: tokens or cost are inferred, missing pricing, cost-only rows, stale caches, or unsupported source shape.

## Data Contract

Add a shared `accuracy` object to machine-readable reports:

```json
{
  "accuracy": {
    "confidence": "medium",
    "sources": [
      {
        "kind": "local-scan",
        "client": "codex",
        "label": "Codex local session logs",
        "confidence": "medium",
        "reason": "Explicit token counters with estimated pricing"
      }
    ],
    "pricing": {
      "kind": "estimated-pricing",
      "confidence": "medium",
      "source": "litellm-cache",
      "matchedModels": 12,
      "unpricedModels": 1,
      "stale": false
    },
    "warnings": [
      "1 model used alias pricing; provider billing may differ"
    ]
  }
}
```

Rules:

- The top-level confidence is the lowest meaningful confidence among included token and cost sources.
- Token confidence and cost confidence should remain separable internally, even if the first JSON surface exposes one combined confidence.
- Existing totals remain unchanged; the first implementation only explains them.
- Missing pricing should lower cost confidence without lowering token confidence.
- Server-submitted totals should include the `submissionId` or submit-history entry when available.

## CLI Surfaces

First implementation targets:

- `tokens --json`
- `tokens graph --json`
- `tokens models --json`
- `tokens status --json`

Text output should stay concise:

```text
Accuracy: medium (local scan + estimated pricing)
Pricing: 12 matched models, 1 unpriced model
```

Detailed explanation belongs in JSON first, then later in menu bar/web UI.

## Status Integration

`tokens status --json` should eventually include:

```json
{
  "data": {
    "latestAccuracy": {
      "confidence": "medium",
      "sourceKinds": ["local-scan", "estimated-pricing"],
      "warnings": ["1 unpriced model in latest submit"]
    }
  }
}
```

This lets a menu bar app answer "is the number trustworthy?" without scanning logs.

## Submit Integration

Submit history should be extended later with:

- `accuracyConfidence`
- `sourceKinds`
- `pricingSource`
- `unpricedModels`
- `warningCount`

The submit payload can include provenance metadata after the local JSON surface is stable. Server ingestion should treat provenance as diagnostic metadata, not as a billing source of truth.

## Implementation Phases

### Phase 1: Local Accuracy Summary

Create shared types for `AccuracyReport`, `AccuracySource`, and `PricingAccuracy`. Compute them from existing parsed messages and pricing lookup outcomes. Expose the summary in `tokens graph --json` and root `tokens --json`.

### Phase 2: Model-Level Pricing Explanation

Add per-model pricing source, matched key, alias status, and confidence to `tokens models --json`. Text output should only show warnings for low-confidence or unpriced models.

### Phase 3: Status And Submit History Bridge

Write compact accuracy summary fields into submit history and expose latest summary through `tokens status --json`.

### Phase 4: Comparison UX

Add a command or JSON section that explains likely differences against other products:

- ClaudeBar may show provider or quota-window numbers while tokens reports local logs.
- Tokscale may be local-only and easy to forget if not scheduled.
- Provider dashboards may use billing-period and post-processing adjustments.

## Testing Strategy

- Unit-test confidence aggregation separately from parsing.
- Add fixtures for explicit-token local logs, provider-official API rows, custom pricing, stale pricing fallback, alias pricing, and unpriced models.
- Snapshot only stable JSON shapes, not volatile timestamps or local paths.
- Verify text output stays quiet for high confidence and only warns when useful.

## Risks

- Over-labeling every row can make JSON noisy. Keep detailed row-level evidence behind model/client summaries first.
- Confidence names can imply more certainty than we have. Use conservative labels and explicit reasons.
- Server and CLI vocabulary can drift. Define shared strings before adding UI.

## Success Criteria

- A user can tell whether tokens is showing local usage, submitted usage, provider usage, or estimated cost.
- `tokens status --json` gives future menu bar/mobile clients enough data to show trust state without scanning logs.
- Differences from ClaudeBar, Tokscale, and provider dashboards can be explained by source kind, pricing source, and time window instead of hand-waving.
