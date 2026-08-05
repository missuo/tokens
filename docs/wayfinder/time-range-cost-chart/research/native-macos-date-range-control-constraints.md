# Research: Native macOS date-range control constraints

**Ticket:** `docs/wayfinder/time-range-cost-chart/tickets/03-research-native-date-range-controls.md`  
**Question:** Which supported macOS SwiftUI/AppKit controls and interaction patterns can provide an inclusive single-date or date-range picker in a Menu Bar `NSPopover`, with reliable keyboard navigation, locale-aware formatting, disabled future dates, and compact selected-range presentation?  
**Project deployment target:** macOS 13 (`Package.swift` → `.macOS(.v13)`)  
**Current shell:** SwiftUI content hosted in AppKit `NSPopover` (`.transient`), made key on show (`Sources/TokensMenuBar/AppMain.swift`).  
**Research date:** 2026-08-04  
**Scope:** Primary Apple sources only (official docs + local SDK headers/interfaces). No product implementation.

---

## Executive answer

| Need | Native support on macOS 13+ | Preferred API surface |
| --- | --- | --- |
| Single calendar day | Yes | SwiftUI `DatePicker` (`.date` components) or AppKit `NSDatePicker` mode `.single` |
| Inclusive contiguous date **range** | Yes, but **only AppKit** has a first-class range mode | `NSDatePicker` + `datePickerMode = .range` + `dateValue`/`timeInterval` |
| Multi discrete dates (non-contiguous) | **No on macOS** | `MultiDatePicker` is `@available(macOS, unavailable)` |
| Disable future dates | Yes (continuous max bound only) | SwiftUI `in: PartialRangeThrough` / `ClosedRange`; AppKit `maxDate` |
| Compact selected-range label | Yes (formatting, not the picker itself) | `DateIntervalFormatter` or `Date.IntervalFormatStyle` / `Range<Date>.formatted` |
| Keyboard + VoiceOver | Best when using system controls | AppKit date picker + standard accessibility; keep popover key |
| Fits Menu Bar popover | Constrained | Prefer compact/field styles or an expanded calendar region; full graphical month is large |

**Bottom line:** There is **no SwiftUI control that selects a contiguous date range on macOS**. Contiguous range selection is an **AppKit `NSDatePicker` range-mode** capability. SwiftUI `DatePicker` is excellent for **one** absolute date with range *constraints*, not for selecting start+end as one control. `MultiDatePicker` must not be planned for this app: it is unavailable on macOS.

---

## 1. Deployment target vs API availability

| API | Availability (Apple docs / SDK) | Fits macOS 13 target? |
| --- | --- | --- |
| SwiftUI `DatePicker` | macOS 10.15+ | Yes |
| `.datePickerStyle(.automatic)` / `DefaultDatePickerStyle` | macOS 10.15+ | Yes |
| `.datePickerStyle(.graphical)` | macOS 10.15+ (style); `makeBody` path notes macOS 13.0+ in interface | Yes |
| `.datePickerStyle(.compact)` | macOS 10.15.4+ | Yes |
| `.datePickerStyle(.field)` / `.stepperField` | macOS 10.15+; **macOS-only** | Yes |
| `.datePickerStyle(.wheel)` | **macOS unavailable** | No |
| SwiftUI `MultiDatePicker` | iOS 16+ / iPadOS / Mac Catalyst / visionOS; **macOS unavailable** | **No** |
| AppKit `NSDatePicker` / range mode / styles | macOS (long-standing AppKit) | Yes |
| `presentsCalendarOverlay` | macOS 10.15.4+ | Yes |
| Foundation `DateIntervalFormatter` | macOS 10.10+ | Yes |
| `Date.FormatStyle` / `Date.IntervalFormatStyle` | macOS 12.0+ | Yes |
| Foundation `DateInterval` (closed interval) | macOS 10.12+ | Yes |

Sources: [DatePicker](https://developer.apple.com/documentation/swiftui/datepicker), [MultiDatePicker](https://developer.apple.com/documentation/swiftui/multidatepicker), style pages (Graphical/Compact/Field/StepperField/Wheel), [DateIntervalFormatter](https://developer.apple.com/documentation/foundation/dateintervalformatter), [Date.IntervalFormatStyle](https://developer.apple.com/documentation/foundation/date/intervalformatstyle), MacOSX SDK `SwiftUI.swiftmodule` (`@available(macOS, unavailable)` on `MultiDatePicker` / `WheelDatePickerStyle`), `Package.swift` platforms.

---

## 2. Supported control inventory

### 2.1 SwiftUI `DatePicker` (single absolute date)

**What it is:** “A control for selecting an absolute date.” Binds to one `Date`. Optional time via `displayedComponents`.

**Range parameters mean constraints, not multi-day selection.** Initializers accept:

- `ClosedRange<Date>`
- `PartialRangeFrom<Date>`
- `PartialRangeThrough<Date>`

Apple’s docs: you can limit the picker so selections are only before/after a date or between two dates. That is how **future dates are disabled** (`in: ...Date()` or through end-of-today in the reporting calendar/time zone).

**Styles on macOS (SDK + docs):**

| Style | Role | Notes for popover |
| --- | --- | --- |
| `.automatic` | System default | Fine for row chrome |
| `.compact` | Compact textual format | Good collapsed Custom label; expands system UI to edit |
| `.graphical` | Interactive calendar/clock | Best “calendar control” look; tall/wide |
| `.field` | Editable field (macOS-only) | Dense; keyboard-oriented |
| `.stepperField` | Field + stepper (macOS-only) | Dense; segment stepping |
| `.wheel` | Wheel columns | **Not available on macOS** |

**Elements:** `DatePickerComponents.date` and `.hourAndMinute` on macOS. Map product “calendar day only” to `displayedComponents: [.date]` (time-of-day Custom is out of scope per map).

**Locale / calendar / time zone:** Prefer SwiftUI environment (`locale`, `calendar`, `timeZone`) so the picker and reporting boundary share one calendar. MultiDatePicker docs (iOS) explicitly show environment overrides; the same environment keys apply to date handling views on macOS.

**Inclusive single day:** One selected `Date` is a point in time. Product “inclusive calendar day” must be derived with `Calendar` start/end-of-day in the **reporting** time zone—not assumed from the picker’s absolute `Date` alone.

Sources: [DatePicker](https://developer.apple.com/documentation/swiftui/datepicker), [datePickerStyle(_:)](https://developer.apple.com/documentation/swiftui/view/datepickerstyle(_:)), style symbol pages, SDK `DatePicker` extensions, [EnvironmentValues.locale](https://developer.apple.com/documentation/swiftui/environmentvalues/locale) / [calendar](https://developer.apple.com/documentation/swiftui/environmentvalues/calendar).

### 2.2 SwiftUI `MultiDatePicker` — unavailable on macOS

DocC platforms list iOS/iPadOS/Mac Catalyst/visionOS only (no macOS). Local `arm64e-apple-macos.swiftinterface`:

```text
@available(iOS 16.0, *)
@available(macOS, unavailable)
...
struct MultiDatePicker<Label> ...
```

Selection type is `Set<DateComponents>` (discrete multi-select), not a contiguous range API. Even if it were available, it would be the wrong model for “inclusive start…end range” unless the product reduced a set to min/max and forbade holes.

**Decision implication:** Do not design Custom range around `MultiDatePicker`.

Source: [MultiDatePicker](https://developer.apple.com/documentation/swiftui/multidatepicker), MacOSX 27 SDK SwiftUI interface.

### 2.3 AppKit `NSDatePicker` — only first-class range control

From SDK headers (`NSDatePicker.h` / `NSDatePickerCell.h`) and DocC:

**Styles (`NSDatePicker.Style`):**

- `textFieldAndStepper`
- `clockAndCalendar` (graphical calendar/clock)
- `textField`

**Modes (`NSDatePicker.Mode`):**

- `single` — one date
- `range` — “a range of dates”

**Value model for range mode:**

- `dateValue` — “When selecting a date range, this property represents the time interval’s **starting date**.”
- `timeInterval` — “The time interval that represents the receiver’s date range. The date range begins at the date returned by `dateValue`.” Returns `0` when not in range mode.

So the native range is **start + duration**, not an inclusive end-date property. Mapping product inclusive calendar end → `timeInterval` is an application concern (see §6).

**Elements (`NSDatePicker.ElementFlags`):** combine date/time flags; for day-only Custom use date flags such as `yearMonthDay` (and omit hour/minute flags).

**Bounds:**

- `minDate` / `maxDate` — “minimum/maximum value that the date picker allows as input”; `nil` = no bound. Use `maxDate` to block future instants.

**Locale presentation:**

- `calendar`, `locale`, `timeZone` properties on the control/cell.

**Validation hook:**

- `NSDatePickerCellDelegate.datePickerCell(_:validateProposedDateValue:timeInterval:)` — can clamp/reject proposed values beyond continuous min/max if needed.

**Text-field calendar overlay:**

- `presentsCalendarOverlay` (macOS 10.15.4+): graphical calendar overlay when editing a calendar element in a **text-field style** picker. Default `NO` per docs. Useful if Custom stays compact in-popover and expands a calendar without embedding a full month permanently.

**Accessibility:** `NSDatePicker` conforms to `NSAccessibilityElementProtocol` / `NSAccessibilityProtocol` (DocC relationships). Apple’s accessibility guidance: prefer standard AppKit controls for built-in accessibility; customize via properties/protocols when needed.

Sources: SDK `NSDatePicker.h`, `NSDatePickerCell.h`; [NSDatePicker](https://developer.apple.com/documentation/appkit/nsdatepicker); [Mode](https://developer.apple.com/documentation/appkit/nsdatepicker/mode); [dateValue](https://developer.apple.com/documentation/appkit/nsdatepicker/datevalue); [timeInterval](https://developer.apple.com/documentation/appkit/nsdatepicker/timeinterval); [minDate](https://developer.apple.com/documentation/appkit/nsdatepicker/mindate) / [maxDate](https://developer.apple.com/documentation/appkit/nsdatepicker/maxdate); [presentsCalendarOverlay](https://developer.apple.com/documentation/appkit/nsdatepicker/presentscalendaroverlay); [NSDatePickerCellDelegate](https://developer.apple.com/documentation/appkit/nsdatepickercelldelegate); [Integrating accessibility into your app](https://developer.apple.com/documentation/accessibility/integrating-accessibility-into-your-app).

---

## 3. Patterns that can satisfy the product interaction

Map requirements (from `map.md`): Custom single-day **or** inclusive date-range; future dates unavailable; when Custom is active the control becomes a **compact date or date-range label**; lives in the Menu Bar dashboard popover.

### Pattern A — AppKit range calendar (native range)

1. Host `NSDatePicker` (via `NSViewRepresentable` if the panel stays SwiftUI).
2. `datePickerStyle = .clockAndCalendar`
3. `datePickerMode = .range`
4. `datePickerElements = .yearMonthDay` (date-only)
5. `maxDate` = end of “today” in reporting calendar/time zone (and optional `minDate` if All/history needs a floor)
6. Read `dateValue` + `timeInterval`; convert to inclusive reporting dates.

**Pros:** One control, true range selection, system keyboard/accessibility for the control, no dual-picker consistency bugs.  
**Cons:** Graphical size fights a dense 400pt popover; range semantics are start+duration (not inclusive end date); SwiftUI bridging required.

### Pattern B — Two SwiftUI single `DatePicker`s (start + end)

1. Two pickers, each `displayedComponents: [.date]`.
2. Constrain both with `in:` so neither can exceed today; additionally keep `start ≤ end` in app state (swap/clamp on change).
3. Styles: `.compact` for collapsed rows, or `.graphical` if expanding an editor region; macOS `.field` / `.stepperField` for denser keyboard entry.

**Pros:** Pure SwiftUI; easy max-date; clear single-day mode (one picker or start==end).  
**Cons:** Not one inclusive range gesture; must implement range integrity and VO labeling (“Start”, “End”) yourself; two graphical calendars are heavier than one.

### Pattern C — Compact label + expandable editor (matches “becomes a compact label”)

Collapsed Custom chrome:

- Show locale-aware compact string (see §5), not a live multi-month UI.

Expanded editor (in-panel disclosure, nested popover, or sheet):

- Pattern A or B underneath.

For compact **editing** without a permanent calendar, AppKit text-field style + `presentsCalendarOverlay = true`, or SwiftUI `.compact` / `.field`.

**Pros:** Aligns with map’s compact Custom presentation; protects popover density.  
**Cons:** Extra interaction state (collapsed vs editing); nested UI must respect transient popover dismissal rules (§4).

### Pattern D — Rejected for macOS product path

- `MultiDatePicker` (unavailable).
- Wheel style (unavailable on macOS).
- Custom-drawn calendar without AppKit/SwiftUI date controls (loses free keyboard/VO unless reimplemented via accessibility protocols—explicitly heavier; Apple recommends subclassing real controls when possible).

---

## 4. `NSPopover` constraints (keyboard, focus, dismissal)

### Current app behavior (repo fact)

- `NSPopover.behavior = .transient`
- On show: `popover.contentViewController?.view.window?.makeKey()` so the panel can receive keyboard focus.
- Content is `NSHostingController` (SwiftUI).

### Platform rules that matter

From [NSPopover](https://developer.apple.com/documentation/appkit/nspopover) and behavior cases:

| Behavior | Closing rule (docs) | Implication for date UI |
| --- | --- | --- |
| `.transient` (current) | Closed when user interacts outside; **menus or panels that become key only when needed will not cause close**; exact interactions not fully specified | Compact date UIs that open system panels/menus are relatively safer; arbitrary outside clicks still dismiss the whole dashboard |
| `.semitransient` | Closes when interacting with UI in the positioning view’s window; **cannot** be shown relative to views in other popovers/child windows | Poor fit if Custom opens another popover |
| `.applicationDefined` | App closes it; docs suggest implementing `-cancel:` for Escape | Use if Custom editing must survive interactions that would kill a transient popover |

**Keyboard:** Making the popover window key (already done) is necessary for field/stepper date editing and standard key bindings. `NSDatePicker` participates in AppKit responder/key-binding machinery (`NSStandardKeyBindingResponding` appears in control conformance graphs for AppKit controls). Escape dismissal is behavior-dependent; application-defined behavior documents considering `-cancel:`.

**VoiceOver:** Prefer stock `DatePicker` / `NSDatePicker` so roles/values come from the system. For two-picker or custom chrome, set accessibility labels on start/end and on the compact Custom summary control. Apple: standard controls provide default accessibility; customize properties when the default is insufficient. Test with Accessibility Inspector + VoiceOver (Apple accessibility sample/guidance).

**Nested presentation risk:** Expanding a large graphical calendar inside the same transient popover changes `contentSize` (app already shrink-wraps height). Opening a second popover from inside a popover is fragile under semi-transient rules and can surprise transient dismissal—prefer **in-panel expansion** or text-field calendar overlay over popover-from-popover.

Sources: [NSPopover](https://developer.apple.com/documentation/appkit/nspopover), [Behavior.transient](https://developer.apple.com/documentation/appkit/nspopover/behavior-swift.enum/transient), [applicationDefined](https://developer.apple.com/documentation/appkit/nspopover/behavior-swift.enum/applicationdefined), [semitransient](https://developer.apple.com/documentation/appkit/nspopover/behavior-swift.enum/semitransient), `AppMain.swift`.

---

## 5. Locale-aware compact formatting (collapsed Custom label)

Product needs a compact single date or date-range string when Custom is active—not hard-coded `YYYY-MM-DD — YYYY-MM-DD` for user-facing chrome (internal/ISO can remain for data).

### 5.1 `DateIntervalFormatter` (macOS 10.10+)

- Produces user-readable strings of the form *start* `-` *end* using locale/language.
- Configure `dateStyle`, `timeStyle` (use `.none` for day-only), `calendar`, `locale`, `timeZone`.
- `string(from:to:)`: both endpoints appear only when the difference warrants both; e.g. with short date style, dates need to be at least one day apart to show both.

### 5.2 `Date.IntervalFormatStyle` (macOS 12+)

- Modern FormatStyle API; docs recommend it from the `DateIntervalFormatter` page for Swift.
- Overview: strings of form `<start> - <end>`; use `Range<Date>.formatted(date:time:)` with date/time style presets (`numeric` / `abbreviated` / `omitted` time for compact day ranges).
- Single-day labels: `Date.FormatStyle` / `date.formatted(...)` (macOS 12+), still locale-driven.

### 5.3 Mapping note for inclusive calendar end

Formatters take absolute `Date` pairs / ranges. If the product stores inclusive end **calendar days**, pass formatter dates that match user intent (typically start-of-day start and start-of-day end, or start-of-day start and exclusive end converted carefully). `DateInterval` is a **closed** `[start, end]` span—compatible with inclusive endpoints if both are start-of-day instants for single-day-resolution ranges, but duration math still differs from “number of calendar days.”

Sources: [DateIntervalFormatter](https://developer.apple.com/documentation/foundation/dateintervalformatter), [string(from:to:)](https://developer.apple.com/documentation/foundation/dateintervalformatter/string(from:to:)), [Date.IntervalFormatStyle](https://developer.apple.com/documentation/foundation/date/intervalformatstyle), [Date.FormatStyle](https://developer.apple.com/documentation/foundation/date/formatstyle), [DateInterval](https://developer.apple.com/documentation/foundation/dateinterval).

---

## 6. Disabling future dates & inclusive range semantics

### Continuous max bound only

Both stacks disable **continuous** ranges of invalid instants:

- SwiftUI: `in: PartialRangeThrough(todayCap)` or `ClosedRange`
- AppKit: `maxDate`

There is **no** first-party AppKit/SwiftUI date-picker API (on macOS) for sparse disablement (e.g. random holes) in these controls. Product only needs “no future,” which matches continuous `maxDate` / upper partial range.

### “Today” cap must use reporting calendar/time zone

Map: all boundaries use configured reporting timezone; today stops at current hour for charting, but Custom is day-resolution (no time-of-day Custom). For the picker max:

- Day-resolution Custom: cap at **start of tomorrow** exclusive or **end of today** inclusive in the reporting calendar—pick one mapping and use it for both UI disablement and query bounds.
- Do not use device-local calendar if reporting TZ differs.

### Inclusive product range vs native models

| Model | Representation | Inclusive calendar end? |
| --- | --- | --- |
| Product (map) | Inclusive start day…end day | Yes by definition |
| `DateInterval` | Closed `[start, end]` | Yes for the two instants stored |
| `NSDatePicker` range | `dateValue` + `timeInterval` duration | End instant = start + duration; **not** an inclusive end-day field |
| Two `DatePicker`s | Two absolute dates | App must enforce order and define whether each date is start-of-day |

**Research conclusion:** Native controls will not automatically honor “inclusive end calendar date” without an explicit conversion layer. Prototype must lock:

1. Storage type (pair of civil dates vs `DateInterval` vs start+duration).
2. Conversion into `NSDatePicker.timeInterval` if Pattern A is chosen (especially single-day → `timeInterval == 0` per docs when range collapses to a point).
3. Whether single-day Custom is mode `.single` / one picker, or range with zero duration / start==end.

---

## 7. Keyboard & VoiceOver expectations (what primary sources guarantee)

**Guaranteed by using system controls:**

- `NSDatePicker` is a real `NSControl` with accessibility protocol conformance documented on the symbol.
- SwiftUI `DatePicker` is the supported SwiftUI date control on macOS; styles `.field` / `.stepperField` exist specifically for macOS editable component fields (keyboard-oriented).
- AppKit accessibility model: standard controls implement defaults; apps assign accessibility properties when needed; test with Accessibility Inspector and VoiceOver.

**Not fully specified in docs (prototype must verify):**

- Exact VoiceOver utterance for `NSDatePicker` **range** mode (start vs interval).
- Tab order when embedding `NSViewRepresentable` date pickers among SwiftUI buttons/chips inside a transient status-item popover.
- Whether `.compact` SwiftUI date presentation’s external UI interacts cleanly with `.transient` dismissal in all locales.
- Graphical calendar keyboard grid navigation details (not spelled out on the DatePicker symbol pages).

These gaps are **verification** items, not alternate-API requirements.

Sources: NSDatePicker relationships (accessibility protocols); [NSAccessibilityProtocol](https://developer.apple.com/documentation/appkit/nsaccessibilityprotocol); [Integrating accessibility into your app](https://developer.apple.com/documentation/accessibility/integrating-accessibility-into-your-app); Field/StepperField style pages.

---

## 8. Fit inside this Menu Bar popover

Repo constraints relevant to control choice:

- Panel width ~400pt; height shrink-wrapped and capped (~80% of presentation screen).
- Dashboard already dense (totals, chart, breakdowns).
- Transient popover + key window on show.

**Control sizing implications:**

- Full `.graphical` / `clockAndCalendar` month UI is the right **editor**, not the permanent collapsed Custom chip.
- Collapsed state should be compact text (formatter) or `.compact`/text-field control.
- Prefer one expanded calendar (Pattern A) over two graphical calendars (Pattern B graphical×2) if range is edited visually.

---

## 9. Recommendation matrix for the parent ticket

| Approach | Single day | Inclusive range | Future disabled | Compact label | Keyboard/VO | Popover fit | macOS 13 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A. `NSDatePicker` range + calendar | Yes (`timeInterval` 0 / single mode) | **Native** | `maxDate` | Separate formatter label | Strong (AppKit) | Expand in-panel | Yes |
| B. Dual SwiftUI `DatePicker` | Yes | App-enforced | `in:` | Formatter or compact styles | Good if labeled | Better with compact/field | Yes |
| C. Compact + expand (A or B inside) | Yes | Depends on child | Yes | **Best match to map** | Depends on child | **Best** | Yes |
| MultiDatePicker | N/A | Wrong model | N/A | N/A | N/A | N/A | **No** |

**Research recommendation (non-binding product choice):** Pattern **C** with expanded editor = **Pattern A** if a single inclusive range gesture is required; or Pattern **B** with `.compact`/`.field` if pure SwiftUI and start/end fields are acceptable. Do not depend on `MultiDatePicker`.

---

## 10. Decisions the parent ticket still needs

1. **Range interaction model:** native one-control range (`NSDatePicker.mode.range`) vs explicit Start/End dual pickers.
2. **Collapsed Custom chrome:** pure text button (formatter only) vs always-visible compact/field picker vs chip that toggles expansion.
3. **Expanded editor placement:** in-popover disclosure vs settings-style window vs accepting transient-dismissal tradeoffs of nested UI.
4. **Single-day representation:** dedicated single-date UI vs range with zero duration / identical endpoints.
5. **Civil-date mapping:** exact conversion among inclusive end day, `DateInterval`, and `dateValue+timeInterval`, all in reporting time zone.
6. **Today cap instant:** end-of-today vs start-of-tomorrow exclusive for `maxDate` / `in:`.
7. **Popover behavior during Custom edit:** stay `.transient` or switch to `.applicationDefined` while the calendar editor is open.
8. **Accessibility acceptance:** required VO phrases for Custom summary, start, end, and disabled future days (prototype/VoiceOver pass).
9. **Whether compact label may drop one endpoint** when `DateIntervalFormatter` collapses nearly-equal dates (docs behavior)—force two-day display or accept system collapse for single-day.

---

## 11. Source index (primary)

### Apple documentation

- [NSDatePicker](https://developer.apple.com/documentation/appkit/nsdatepicker)
- [NSDatePicker.Mode](https://developer.apple.com/documentation/appkit/nsdatepicker/mode) (single / range)
- [dateValue](https://developer.apple.com/documentation/appkit/nsdatepicker/datevalue), [timeInterval](https://developer.apple.com/documentation/appkit/nsdatepicker/timeinterval)
- [minDate](https://developer.apple.com/documentation/appkit/nsdatepicker/mindate), [maxDate](https://developer.apple.com/documentation/appkit/nsdatepicker/maxdate)
- [presentsCalendarOverlay](https://developer.apple.com/documentation/appkit/nsdatepicker/presentscalendaroverlay)
- [NSDatePickerCell](https://developer.apple.com/documentation/appkit/nsdatepickercell), [NSDatePickerCellDelegate](https://developer.apple.com/documentation/appkit/nsdatepickercelldelegate)
- [SwiftUI DatePicker](https://developer.apple.com/documentation/swiftui/datepicker)
- [MultiDatePicker](https://developer.apple.com/documentation/swiftui/multidatepicker) (no macOS platform)
- [datePickerStyle(_:)](https://developer.apple.com/documentation/swiftui/view/datepickerstyle(_:))
- [GraphicalDatePickerStyle](https://developer.apple.com/documentation/swiftui/graphicaldatepickerstyle), [CompactDatePickerStyle](https://developer.apple.com/documentation/swiftui/compactdatepickerstyle), [FieldDatePickerStyle](https://developer.apple.com/documentation/swiftui/fielddatepickerstyle), [StepperFieldDatePickerStyle](https://developer.apple.com/documentation/swiftui/stepperfielddatepickerstyle), [WheelDatePickerStyle](https://developer.apple.com/documentation/swiftui/wheeldatepickerstyle)
- [NSPopover](https://developer.apple.com/documentation/appkit/nspopover) and [Behavior](https://developer.apple.com/documentation/appkit/nspopover/behavior-swift.enum) cases
- [DateIntervalFormatter](https://developer.apple.com/documentation/foundation/dateintervalformatter), [string(from:to:)](https://developer.apple.com/documentation/foundation/dateintervalformatter/string(from:to:))
- [Date.IntervalFormatStyle](https://developer.apple.com/documentation/foundation/date/intervalformatstyle), [Date.FormatStyle](https://developer.apple.com/documentation/foundation/date/formatstyle)
- [DateInterval](https://developer.apple.com/documentation/foundation/dateinterval)
- [Integrating accessibility into your app](https://developer.apple.com/documentation/accessibility/integrating-accessibility-into-your-app)
- [NSAccessibilityProtocol](https://developer.apple.com/documentation/appkit/nsaccessibilityprotocol)

### Local SDK / project

- `MacOSX27.0.sdk/.../AppKit.framework/Headers/NSDatePicker.h`
- `MacOSX27.0.sdk/.../AppKit.framework/Headers/NSDatePickerCell.h`
- `MacOSX27.0.sdk/.../SwiftUI.framework/Modules/SwiftUI.swiftmodule/arm64e-apple-macos.swiftinterface` (`DatePicker`, `MultiDatePicker`, date picker styles)
- `Package.swift` — `.macOS(.v13)`
- `Sources/TokensMenuBar/AppMain.swift` — transient `NSPopover` + `makeKey()`

### Wayfinder inputs

- `docs/wayfinder/time-range-cost-chart/map.md` (Custom inclusive range, no future dates, compact Custom label)
- `docs/wayfinder/time-range-cost-chart/tickets/03-research-native-date-range-controls.md`
