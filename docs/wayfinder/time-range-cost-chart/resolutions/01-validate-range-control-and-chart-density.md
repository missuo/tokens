# Resolution comment: Validate the compact range control and chart density

The human selected **B — Anchored overlay** after reviewing the three in-context variants.

- [Interactive prototype](../../../../design/menubar-ui-v1/time-range-prototype.html?variant=B)
- [Early-Today 12-hour context screenshot](../assets/today-12h-context-boundary.png)
- The Custom editor opens as a compact overlay anchored to the date control, preserving the panel's normal height while accepting that underlying dashboard content is temporarily covered.
- Custom supports both a single day and an inclusive date range: choose a start date, then an end date; choosing the same date twice represents one day.
- When closed, Custom is represented by locale-aware compact date or date-range text in the top control.
- At 420pt width, sparse labels remain legible for hourly, daily, natural-week, and natural-month Cost chart buckets; the chart title names the active granularity and hover exposes exact bucket bounds.
- Today’s Cost chart keeps a minimum of 12 hourly buckets. Prior-day context is muted, excluded from Today totals, and separated from Today by a vertical dashed midnight marker.
- Production should use the native AppKit range control for keyboard and VoiceOver semantics, move focus into the editor on open, return focus to Custom on close, and support Escape dismissal.
