# PROTOTYPE — Canonical time range and chart buckets

## Question

Do Today / 7D / 30D / All / Custom reduce to one predictable inclusive date range and automatic hour/day/natural-week/natural-month bucket model, including DST, leap-day, active-bucket, and incomplete-edge behavior?

This is throwaway planning code. It does not implement the production report contract.

## Run

Interactive TUI:

```bash
make prototype-time-range
```

Edge-case scenario sweep:

```bash
make prototype-time-range ARGS=scenarios
```

## Candidate rules encoded for review

- Today totals = reporting today from 00:00 through the active local hour.
- Today’s Cost chart shows at least 12 hourly buckets. Before 11:00, it prepends prior-day context hours; those hours are excluded from Today totals and separated at midnight by a vertical dashed marker.
- 7D = reporting today minus 6 days through today, inclusive.
- 30D = reporting today minus 29 days through today, inclusive.
- All = earliest known usage date through today; with no known usage it resolves to Today.
- Custom accepts any non-inverted historical date range, including dates before the first known usage record, and rejects future end dates.
- Granularity follows actual inclusive span: 1 day → hour; 2–14 → day; 15–90 → natural week; over 90 → natural month.
- Natural weeks are Monday–Sunday. Natural months use calendar-month boundaries.
- Week/month buckets are clipped to the selected range; clipped first/last buckets are incomplete edge chart buckets.
- A range ending today has one active bucket. Future buckets are not emitted.
- Every bucket in the selected range is emitted; buckets with no usage are represented as zero rather than omitted.
- Historical DST days produce 23 or 25 real hourly buckets; repeated local hours include their UTC offset.
