# Resolution comment: Research native macOS date-range control constraints

Research completed against Apple documentation and local SDK interfaces.

- [Research asset](../research/native-macos-date-range-control-constraints.md)
- SwiftUI provides a single-date picker on macOS, but no native contiguous date-range picker; `MultiDatePicker` is unavailable on macOS.
- AppKit provides the first-class contiguous range control through `NSDatePicker` range mode and supports reporting-timezone calendar, locale, bounds, keyboard, and accessibility behavior.
- Compact selected-range text should use locale-aware Foundation interval formatting rather than fixed ISO copy.
- A compact control with an in-panel expanded editor or calendar overlay is safer than nesting another popover inside the app's transient Menu Bar popover.
- Inclusive calendar-end semantics, the exact AppKit-versus-dual-SwiftUI editor choice, and VoiceOver acceptance remain product/prototype decisions owned by the existing frontier tickets.
