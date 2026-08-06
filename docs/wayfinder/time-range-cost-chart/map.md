---
title: Make time ranges first-class across the dashboard and Cost chart
tracker: local-markdown
kind: wayfinder-map
status: closed
labels:
  - wayfinder:map
---

## Destination

An implementation-ready product and data specification for a first-class dashboard time range and a Cost chart that follows it, with no unresolved user-facing, aggregation, caching, or acceptance decisions before implementation planning begins.

## Notes

- This map was planning-only: it produced decisions, prototypes, and research findings. The feature implementation landed in PR #8.
- Implementation pointers: `cli/tokens-cli/src/commands/usage_report_v3.rs`, `cli/tokens-cli/src/commands/usage_snapshot.rs`, `Sources/TokensMenuBarCore/{UsageStore,UsageService,DateRangePicker,CostChartView}.swift`; tests in `cli/tokens-cli/tests/usage_v3_cli.rs` and `Tests/TokensMenuBarTests/{ReportV3,DateRangePicker,CostChart,UsageStore,UsageService}Tests.swift`.
- All repository artifacts for this effort must be created in a new worktree, never directly on `main`.
- Consult `grilling` and `domain-modeling` for product decisions, `prototype` for interaction or contract prototypes, `dataviz` for chart behavior, and `research` for external platform facts.
- Locked destination constraints from the initiating conversation:
  - Visible presets remain Today / 7D / 30D / All.
  - A calendar control provides Custom single-day or inclusive date-range selection; there is no Yesterday preset.
  - Future dates are unavailable. When Custom is active, the calendar control becomes a compact date or date-range label.
  - Every app launch starts on Today rather than restoring the previous selection.
  - The selected range applies consistently to totals, breakdowns, client/project/model sections, and the Cost chart.
  - Cost chart granularity is automatic: 1 day → hour; 2–14 days → day; 15–90 days → natural week; over 90 days → natural month.
  - A natural week runs Monday through Sunday; a natural month follows calendar-month boundaries, both in the reporting timezone.
  - All boundaries use the configured reporting timezone. Today totals run from 00:00 through now; its Cost chart shows at least 12 hourly buckets, using clearly separated prior-day context when needed. Future buckets are never drawn; incomplete edge chart buckets and the active bucket are identified.
  - Local tickets live in `tickets/`; each filename is its stable identity, and `blocked_by` lists blocker filenames.
  - Single-day switching must feel immediate, including the hourly chart.
  - Presets and Custom resolve to one canonical inclusive date range; the data layer returns a generic bucketed series and the UI renders it.

## Decisions so far

- [Research native macOS date-range control constraints](tickets/03-research-native-date-range-controls.md) — macOS has no SwiftUI contiguous-range picker; AppKit supplies the native range control, with locale-aware compact formatting and in-panel editing preferred for the transient Menu Bar popover.
- [Validate the compact range control and chart density](tickets/01-validate-range-control-and-chart-density.md) — use the anchored-overlay variant with one AppKit range control; Custom supports both one day and inclusive ranges, and all four automatic chart granularities remain legible at Menu Bar width.
- [Define canonical time-range and granularity rules](tickets/02-define-unified-range-report-contract.md) — all selections resolve to inclusive reporting-timezone dates; actual span selects hour/day/natural-week/natural-month buckets, including real DST hours, clipped edges, explicit zeroes, no future buckets, and a 12-hour minimum context for early Today charts.
- [Define the bucketed report and snapshot-cache contract](tickets/02b-define-bucketed-report-and-cache-contract.md) — one v3 report keeps all totals on the selected range while a generic series may add explicitly excluded Today context; a full-history daily/hourly snapshot makes switching immediate without changing v2 callers.

## Not yet specified

<!-- none yet — residual fog will graduate from closed frontier tickets -->

## Out of scope

- Time-of-day selection or arbitrary sub-day Custom ranges.
- A user-selectable Hour / Day / Week / Month granularity control.
- Additional visible presets, including Yesterday.
- A timezone picker or changes to the existing reporting-timezone policy.
- Redesigning unrelated dashboard sections, pricing logic, scanning cadence, or data sources.
- Feature implementation, rollout, or release work inside this Wayfinder map.
