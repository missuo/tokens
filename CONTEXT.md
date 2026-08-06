# Local Usage Analytics

This context describes how the Menu Bar app names time selection and aggregation concepts for local usage analytics.

## Language

**Reporting timezone**:
The configured timezone whose calendar boundaries define usage dates and aggregation buckets.
_Avoid_: Device timezone, local time

**Time range**:
An inclusive interval of calendar dates in the reporting timezone applied to every dashboard measure.
_Avoid_: Period, window, filter

**Preset**:
A named shortcut that resolves to a time range, such as Today, 7D, 30D, or All.
_Avoid_: Tab, period

**Custom range**:
A user-selected single date or inclusive span of dates that resolves to a time range.
_Avoid_: Custom period, date filter

**Chart bucket**:
A reporting-timezone interval that combines usage for one Cost chart mark, at hour, day, natural-week, or natural-month granularity.
_Avoid_: Point, slot

**Natural week**:
A Monday-through-Sunday chart bucket in the reporting timezone.
_Avoid_: Rolling week, seven-day bucket

**Natural month**:
A first-day-through-last-day calendar-month chart bucket in the reporting timezone.
_Avoid_: Thirty-day bucket, rolling month

**Incomplete edge chart bucket**:
A chart bucket at either end of a time range that the selected dates cover only partly.
_Avoid_: Partial point, partial bar

**Active bucket**:
The chart bucket containing the current time and therefore not yet complete.
_Avoid_: Current bar, partial point

**Empty bucket**:
A chart bucket inside the time range with no recorded usage, represented explicitly as zero.
_Avoid_: Missing bucket, gap

**Chart context bucket**:
A chart bucket outside the time range that is shown only to maintain readable chart density and is excluded from every selected-range total.
_Avoid_: Extra data, overflow bucket

**Time-range boundary marker**:
A vertical dashed chart marker separating chart context buckets from buckets inside the selected time range.
_Avoid_: Today line, cutoff line

**Unplaced usage**:
Usage included in a time range whose date is known but whose timestamp is not trustworthy enough to assign it to an hourly chart bucket.
_Avoid_: Unknown usage, missing usage

**Weekday × hour heatmap**:
A 7 × 24 grid that aggregates cost (and token/message totals) by ISO weekday and reporting-timezone hour of day over the time range, zero-filled for empty cells; unplaced usage is excluded. Shown on the Advanced page, which always charts its own fixed trailing-30-day report independent of the dashboard selection and falls back to the dashboard report until that report is ready.
_Avoid_: Activity matrix, time-of-day chart
