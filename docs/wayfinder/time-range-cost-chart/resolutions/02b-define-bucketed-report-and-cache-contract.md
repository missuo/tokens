# Resolution comment: Define the bucketed report and snapshot-cache contract

The human approved the candidate contract and added a 12-hour minimum for early Today charts, with a vertical dashed boundary separating prior-day context from Today.

- [Contract prototype and fixtures](../prototypes/report-cache-contract/README.md)
- [Early-Today chart screenshot](../assets/today-12h-context-boundary.png)
- One report-building seam accepts range-independent facts, a selection, and reporting now, then returns the complete selected-range report.
- Report v3 contains the canonical selection/range, every dashboard rollup, and one generic time series with explicit granularity, bounds, active state, incomplete-edge state, context membership, and unplaced totals.
- Report v3 also carries a full 7 × 24 weekday × hour grid (`weekdayHour`, ISO weekday × reporting-timezone hour of day, zero-filled) aggregated from the selected days' exact hourly facts; unplaced usage is excluded by construction. It powers the Menu Bar Advanced page heatmap.
- Today totals remain reporting-timezone 00:00 through now. Its Cost chart emits at least 12 hourly buckets; prior-day buckets are context only, visually muted, and excluded from every Today total.
- `selectionStart` anchors the vertical dashed boundary between context and Today. No boundary is drawn when all displayed buckets are inside Today.
- Layer B remains full-history and range-independent, retaining daily facts for all dashboard rollups, exact hourly facts for immediate single-day charts, and separate unplaced usage.
- Active, incomplete-edge, zero-filled, and context-only chart state is derived at report time rather than persisted.
- A range switch reuses a same-reporting-day snapshot when schema and timezone match. Timer/manual refresh replaces it from Layer A; force rescan clears both layers; day, timezone, schema, or decode mismatch rebuilds Layer B.
- Existing callers remain on report v2. New callers explicitly request v3. Existing v2 snapshots rebuild from Layer A because they lack hourly facts.
- How unplaced usage is communicated in an hourly chart remains delegated to “Decide how hourly charts handle unreliable timestamps.”
