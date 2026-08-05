# PROTOTYPE — Bucketed report and snapshot-cache contract

## Question

What report payload and range-independent snapshot keep every dashboard section on one canonical time range while making a switch to a single-day hourly chart immediate?

This is throwaway planning code. It does not change the production CLI, cache, or Menu Bar app.

## Run

Round-trip the typed fixtures and check total coherence:

```bash
make prototype-report-contract
```

Regenerate the committed fixtures:

```bash
make prototype-report-contract \
  ARGS="emit docs/wayfinder/time-range-cost-chart/prototypes/report-cache-contract/fixtures"
```

Inspect one fixture:

```bash
make prototype-report-contract \
  ARGS="inspect docs/wayfinder/time-range-cost-chart/prototypes/report-cache-contract/fixtures/report-v3-today.json"
```

## Candidate report contract

A new caller requests report contract v3 explicitly. One response contains:

- the original selection and its resolved inclusive dates;
- the reporting timezone;
- summary, token breakdown, client, project, and model results, all aggregated from those same dates;
- one generic `timeSeries` with explicit hour, day, natural-week, or natural-month granularity;
- ordered chart buckets with machine-readable nominal and covered half-open bounds;
- `selectionStart`, which anchors the time-range boundary marker when context is present;
- explicit `contextOnly`, `active`, and `incompleteEdge` state derived for this request;
- explicit zero-valued buckets inside the selected range;
- `unplaced` totals for usage included in the range but not honestly assignable to an hourly bucket.

The UI formats labels from bucket bounds. It does not infer bucket dates, clipping, DST offsets, active state, context membership, or granularity.

`nominalStart` / `nominalEndExclusive` preserve the calendar identity of a chart bucket. `coveredStart` / `coveredEndExclusive` identify the portion represented by the chart after edge clipping or an active-hour cutoff. This lets a natural-week or natural-month bucket remain identifiable even when an edge is clipped.

Summary totals must equal the sum of non-context chart buckets plus `timeSeries.unplaced`. Client, project, model, and token-breakdown totals are computed from the same selected daily facts rather than from a separate chart query. `contextOnly` buckets are display context and are excluded from every selected-range total.

## Today minimum chart density

Today still means 00:00 through reporting now for every dashboard total. Its hourly Cost chart emits at least 12 buckets:

- At 01:30, the chart shows ten prior-day context buckets plus 00:00 and the active 01:00 hour.
- At and after 11:00, Today already supplies at least 12 buckets, so no context is added.
- Context buckets use real historical values when available and explicit zeroes otherwise.
- Context marks are visually de-emphasized.
- When context exists, the UI draws a vertical dashed marker at `selectionStart` to separate it from Today.
- Context never changes Today totals, active-day count, breakdowns, or `unplaced` totals.

## Candidate snapshot contract

Layer B becomes a full-history, range-independent facts snapshot. Each reporting day retains:

- daily totals and token breakdown;
- client/model contributions;
- project/model contributions;
- exact DST-aware hourly totals represented by absolute start/end instants;
- separate totals that are valid for the day but cannot be placed into a trustworthy hour.

Daily facts keep summary and all dashboard breakdowns fast. Hourly facts make a single-day selection fast. Longer chart granularities are folded from daily facts. The snapshot does not persist selection, granularity, zero-filled chart buckets, active state, or incomplete-edge state; those depend on the request and reporting time.

## Build seam

The candidate deep module has one interface:

```text
build_usage_report(facts_snapshot, selection, reporting_now) -> UsageReportV3
```

A live scan and a Layer B decode both produce the same range-independent facts. Range resolution, filtering, zero filling, chart aggregation, edge clipping, active-state derivation, and dashboard rollups stay behind this seam.

## Cache reuse and invalidity

- A range switch reuses Layer B when snapshot schema, reporting calendar day, and reporting timezone match.
- Changing Today / 7D / 30D / All / Custom does not invalidate Layer B because the snapshot is full-history and range-independent.
- Timer and manual refresh bypass Layer B, use the incremental source-message cache, and replace Layer B with refreshed facts.
- Force rescan clears both cache layers before rebuilding.
- Reporting-day rollover, timezone change, unreadable data, or snapshot-schema mismatch invalidates Layer B.
- Existing v2 snapshots are rebuilt from Layer A rather than migrated because they do not retain trustworthy hourly facts.
- Active and incomplete-edge flags are always recomputed from `reporting_now`; they can never go stale in the snapshot.

This preserves current same-day fast switching. Freshness during the day remains governed by the existing timer/manual refresh behavior rather than by a range change.

## Compatibility

Report and snapshot schema versions are independent, even when both happen to use version 3 initially.

- Existing `tokens usage --json --period <preset>` callers continue receiving report v2.
- New preset callers opt into v3 with `--contract v3`.
- New Custom callers use `--contract v3 --since <date> --until <date>`.
- The Menu Bar app migrates atomically to requesting and decoding v3; other callers are not silently changed.
- Snapshot evolution remains internal and does not force the external report version to change.

## Fixtures

- `snapshot-v3-sample.json` — two days of daily/hourly facts, including the prior-day hours needed for Today context and separate unplaced hourly totals.
- `report-v3-today.json` — an early 01:30 report with 12 real hourly buckets: ten prior-day context hours, two Today hours, a midnight selection boundary, and one active bucket.
- `report-v3-30d.json` — five natural-week buckets for July 6 through August 4; the final week is clipped and active.
- `report-v3-custom-historical.json` — five complete historical daily buckets for June 1 through June 5.

## Deferred decision

The contract preserves `unplaced` usage without fabricating an hour. How the single-day hourly Cost chart communicates or visualizes that amount is intentionally left to “Decide how hourly charts handle unreliable timestamps.”
