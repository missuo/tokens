# Resolution comment: Define canonical time-range and granularity rules

The logic prototype established one date-based rule set for every preset and Custom selection.

- [Logic prototype](../prototypes/time-range-logic/README.md)
- Today resolves to reporting today; 7D resolves to today minus 6 days through today; 30D resolves to today minus 29 days through today. All boundaries are inclusive civil dates in the reporting timezone.
- All resolves from the earliest known usage date through today. With no known usage, All resolves to Today.
- Custom accepts any non-inverted historical range, including dates before the first known usage record. Future dates are rejected rather than clamped.
- Automatic granularity follows the inclusive span: 1 day → hour; 2–14 days → day; 15–90 days → natural week; over 90 days → natural month.
- Historical hourly charts use real reporting-timezone hours. DST spring days contain 23 buckets; DST fall days contain 25, with repeated clock hours distinguished by UTC offset.
- Natural-week and natural-month buckets retain their calendar identity but are clipped to the selected range. Clipped first or last buckets are incomplete edge chart buckets.
- A range ending today emits one active bucket through reporting now and no future buckets.
- Today totals remain 00:00 through reporting now. Its Cost chart shows at least 12 hourly buckets; before 11:00 it prepends prior-day context hours that are excluded from Today totals and separated from Today by a vertical dashed midnight marker.
- Every bucket inside the selected range is emitted; an empty bucket has zero usage rather than disappearing.
