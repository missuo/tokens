import SwiftUI

public enum DateRangePickerConversion {
    public enum ConversionError: Error, Equatable {
        case invalidCivilDate(String)
    }

    public static func calendar(timeZone: TimeZone) -> Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = timeZone
        return calendar
    }

    public static func date(from civilDate: String, timeZone: TimeZone) throws -> Date {
        let parts = civilDate.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3 else { throw ConversionError.invalidCivilDate(civilDate) }
        var components = DateComponents()
        components.calendar = calendar(timeZone: timeZone)
        components.timeZone = timeZone
        components.year = parts[0]
        components.month = parts[1]
        components.day = parts[2]
        components.hour = 0
        guard let date = components.date else {
            throw ConversionError.invalidCivilDate(civilDate)
        }
        return date
    }

    public static func civilDate(from date: Date, timeZone: TimeZone) -> String {
        let formatter = DateFormatter()
        formatter.calendar = calendar(timeZone: timeZone)
        formatter.timeZone = timeZone
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }

    public static func today(now: Date = Date(), timeZone: TimeZone) -> DateSelectionRange {
        let value = civilDate(from: now, timeZone: timeZone)
        return DateSelectionRange(startDate: value, endDate: value)
    }

    /// The selection a committed draft maps to: nil when the draft is unordered,
    /// `.preset(.today)` when the draft is exactly today (a single picked day
    /// jumps straight to the Today preset), otherwise `.custom(draft)` — a
    /// single non-today day stays an inclusive single-day custom range.
    public static func committedSelection(
        for draft: DateSelectionRange,
        today: DateSelectionRange
    ) -> UsageSelection? {
        guard draft.isOrdered else { return nil }
        return draft == today ? .preset(.today) : .custom(draft)
    }

    public static func range(
        for period: UsagePeriod,
        now: Date = Date(),
        timeZone: TimeZone
    ) -> DateSelectionRange? {
        let daysBack: Int
        switch period {
        case .today:
            daysBack = 0
        case .days7:
            daysBack = 6
        case .days30:
            daysBack = 29
        case .all:
            // The earliest known usage date is authoritative and only arrives
            // in the matching v3 report; it cannot be derived from the clock.
            return nil
        }

        let calendar = calendar(timeZone: timeZone)
        let end = calendar.startOfDay(for: now)
        let start = calendar.date(byAdding: .day, value: -daysBack, to: end) ?? end
        return DateSelectionRange(
            startDate: civilDate(from: start, timeZone: timeZone),
            endDate: civilDate(from: end, timeZone: timeZone)
        )
    }

    // MARK: - Month grid math

    /// Start of the month containing `date`, in `timeZone`.
    public static func monthStart(containing date: Date, timeZone: TimeZone) -> Date {
        let calendar = calendar(timeZone: timeZone)
        let components = calendar.dateComponents([.year, .month], from: date)
        return calendar.date(from: components) ?? date
    }

    /// First day of the month `months` away from `monthStart`.
    public static func shiftingMonth(
        _ monthStart: Date,
        by months: Int,
        timeZone: TimeZone
    ) -> Date {
        let calendar = calendar(timeZone: timeZone)
        return calendar.date(byAdding: .month, value: months, to: monthStart) ?? monthStart
    }

    /// One month of grid cells for the picker: a localized title, weekday
    /// headers ordered by the system's first weekday, and whole-week rows of
    /// days (leading/trailing cells spill from adjacent months).
    public static func monthGrid(
        for month: Date,
        timeZone: TimeZone,
        locale: Locale
    ) -> CalendarMonthGrid {
        var calendar = calendar(timeZone: timeZone)
        calendar.locale = locale
        let monthStart = monthStart(containing: month, timeZone: timeZone)

        let titleFormatter = DateFormatter()
        titleFormatter.calendar = calendar
        titleFormatter.timeZone = timeZone
        titleFormatter.locale = locale
        titleFormatter.dateFormat = "LLLL yyyy"

        let firstWeekday = Calendar.current.firstWeekday
        let weekdaySymbols = (0..<7).map {
            calendar.veryShortWeekdaySymbols[(firstWeekday - 1 + $0) % 7]
        }

        let monthWeekday = calendar.component(.weekday, from: monthStart)
        let leading = (monthWeekday - firstWeekday + 7) % 7
        let daysInMonth = calendar.range(of: .day, in: .month, for: monthStart)?.count ?? 30
        let totalCells = Int(ceil(Double(leading + daysInMonth) / 7.0)) * 7
        let gridStart = calendar.date(byAdding: .day, value: -leading, to: monthStart) ?? monthStart

        let days = (0..<totalCells).map { offset -> CalendarMonthGrid.Day in
            let date = calendar.date(byAdding: .day, value: offset, to: gridStart) ?? gridStart
            return CalendarMonthGrid.Day(
                civilDate: civilDate(from: date, timeZone: timeZone),
                dayNumber: calendar.component(.day, from: date),
                isInMonth: offset >= leading && offset < leading + daysInMonth
            )
        }

        return CalendarMonthGrid(
            title: titleFormatter.string(from: monthStart),
            weekdaySymbols: weekdaySymbols,
            days: days,
            monthStart: monthStart
        )
    }
}

/// One month of calendar cells for the range picker grid. Pure data so the
/// grid math is unit-testable without a view.
public struct CalendarMonthGrid: Equatable {
    public struct Day: Equatable, Identifiable {
        public let civilDate: String
        public let dayNumber: Int
        public let isInMonth: Bool
        public var id: String { civilDate }
    }

    public let title: String
    public let weekdaySymbols: [String]
    public let days: [Day]
    /// First day of the visible month, start of day in the picker's time zone.
    public let monthStart: Date
}

/// Two-click range selection cycle: the first click places the start point, the
/// second click places the end point (an earlier day swaps the endpoints so the
/// range stays ordered), and any click after a completed range starts over with
/// a new start point. Pure mapping so it can be unit-tested without a view.
public enum DateRangeSelectionCycle {
    public enum Phase: Equatable {
        /// Only a start point is in place; the next click finishes the range.
        case awaitingEnd
        /// A start+end range is in place; the next click starts over.
        case complete
    }

    public struct Result: Equatable {
        /// Selection to store after the click.
        public let selection: DateSelectionRange
        /// Phase the cycle is in after the click.
        public let phase: Phase
    }

    public static func initialPhase(for selection: DateSelectionRange) -> Phase {
        selection.startDate == selection.endDate ? .awaitingEnd : .complete
    }

    /// Maps a clicked day into the two-click cycle.
    /// - `clicked`: civil date (yyyy-MM-dd) of the day the user tapped.
    /// - `previous`: the selection before this click; in `.awaitingEnd` its
    ///   `startDate` is the anchor the range extends from.
    /// - `phase`: the cycle phase before this click.
    public static func reduce(
        clicked: String,
        previous: DateSelectionRange,
        phase: Phase
    ) -> Result {
        switch phase {
        case .awaitingEnd:
            let anchor = previous.startDate
            return Result(
                selection: DateSelectionRange(
                    startDate: min(anchor, clicked),
                    endDate: max(anchor, clicked)
                ),
                phase: .complete
            )
        case .complete:
            return Result(
                selection: DateSelectionRange(startDate: clicked, endDate: clicked),
                phase: .awaitingEnd
            )
        }
    }
}

/// Month-grid date range picker: tap once for the start day, again for the end
/// day, and a third tap starts over. Clicking outside the picker applies the
/// draft (handled by the panel).
public struct RangeCalendarPicker: View {
    @Binding private var selection: DateSelectionRange
    private let timeZone: TimeZone
    private let locale: Locale
    /// Inclusive maximum selectable day (yyyy-MM-dd); later days are disabled.
    private let maximumCivilDate: String

    @State private var visibleMonth: Date
    @State private var phase: DateRangeSelectionCycle.Phase
    @FocusState private var isFocused: Bool

    private static let cellWidth: CGFloat = 44
    private static let cellHeight: CGFloat = 34
    private static let gridSpacing: CGFloat = 2

    public init(
        selection: Binding<DateSelectionRange>,
        timeZone: TimeZone,
        locale: Locale = .current,
        maximumCivilDate: String
    ) {
        _selection = selection
        self.timeZone = timeZone
        self.locale = locale
        self.maximumCivilDate = maximumCivilDate
        let anchor = (try? DateRangePickerConversion.date(
            from: selection.wrappedValue.endDate,
            timeZone: timeZone
        )) ?? Date()
        _visibleMonth = State(initialValue: DateRangePickerConversion.monthStart(
            containing: anchor,
            timeZone: timeZone
        ))
        _phase = State(initialValue: DateRangeSelectionCycle.initialPhase(
            for: selection.wrappedValue
        ))
    }

    public var body: some View {
        let grid = DateRangePickerConversion.monthGrid(
            for: visibleMonth,
            timeZone: timeZone,
            locale: locale
        )
        VStack(spacing: 8) {
            monthHeader(grid)
            weekdayHeader(grid)
            dayGrid(grid)
        }
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color(nsColor: .windowBackgroundColor))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .stroke(Color.primary.opacity(0.15), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.25), radius: 10, y: 3)
        // Swallow taps on padding/background so the panel's commit-tap only
        // fires for clicks truly outside the picker.
        .contentShape(Rectangle())
        .onTapGesture { }
        .focusable(true)
        .focused($isFocused)
        .onAppear { isFocused = true }
        .accessibilityLabel("Custom usage date range")
    }

    // MARK: - Sections

    private func monthHeader(_ grid: CalendarMonthGrid) -> some View {
        HStack {
            Button { shiftMonth(by: -1) } label: {
                Image(systemName: "chevron.left")
            }
            .accessibilityLabel("Previous month")

            Text(grid.title)
                .font(.system(size: 13, weight: .medium, design: .monospaced))
                .frame(maxWidth: .infinity)

            Button { shiftMonth(by: 1) } label: {
                Image(systemName: "chevron.right")
            }
            .disabled(!canShiftForward(grid))
            .opacity(canShiftForward(grid) ? 1 : 0.3)
            .accessibilityLabel("Next month")
        }
        .buttonStyle(.plain)
        .font(.system(size: 12, weight: .semibold))
        .frame(width: gridWidth)
    }

    private func weekdayHeader(_ grid: CalendarMonthGrid) -> some View {
        LazyVGrid(columns: gridColumns, spacing: Self.gridSpacing) {
            ForEach(Array(grid.weekdaySymbols.enumerated()), id: \.offset) { _, symbol in
                Text(symbol)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .frame(width: Self.cellWidth, height: 16)
            }
        }
    }

    private func dayGrid(_ grid: CalendarMonthGrid) -> some View {
        LazyVGrid(columns: gridColumns, spacing: Self.gridSpacing) {
            ForEach(grid.days) { day in
                dayCell(day)
            }
        }
    }

    private func dayCell(_ day: CalendarMonthGrid.Day) -> some View {
        let isFuture = day.civilDate > maximumCivilDate
        let hasRange = selection.isOrdered && selection.startDate != selection.endDate
        let inRange = selection.isOrdered
            && day.civilDate >= selection.startDate
            && day.civilDate <= selection.endDate
        let isStart = inRange && day.civilDate == selection.startDate
        let isEnd = inRange && day.civilDate == selection.endDate
        let isToday = day.civilDate == todayCivilDate

        return Button {
            pick(day)
        } label: {
            ZStack {
                // Range band: full width for days inside the range, half width
                // at the endpoints so the band connects across cells.
                HStack(spacing: 0) {
                    bandHalf(hasRange && inRange && !isStart)
                    bandHalf(hasRange && inRange && !isEnd)
                }
                Text("\(day.dayNumber)")
                    .font(.system(size: 13, design: .monospaced))
                    .monospacedDigit()
                    .foregroundStyle(cellForeground(
                        isEndpoint: isStart || isEnd,
                        isFuture: isFuture,
                        isInMonth: day.isInMonth
                    ))
                    .frame(width: 30, height: 30)
                    .background {
                        if isStart || isEnd {
                            Circle().fill(Color.primary)
                        } else if isToday {
                            Circle().stroke(Color.primary.opacity(0.45), lineWidth: 1)
                        }
                    }
            }
            .frame(width: Self.cellWidth, height: Self.cellHeight)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(isFuture)
        .accessibilityLabel(day.civilDate)
        .accessibilityAddTraits((isStart || isEnd) ? .isSelected : [])
    }

    // MARK: - Interaction

    private func pick(_ day: CalendarMonthGrid.Day) {
        let result = DateRangeSelectionCycle.reduce(
            clicked: day.civilDate,
            previous: selection,
            phase: phase
        )
        selection = result.selection
        phase = result.phase
        // Tapping an adjacent-month spillover cell navigates to that month.
        if !day.isInMonth,
           let date = try? DateRangePickerConversion.date(from: day.civilDate, timeZone: timeZone) {
            visibleMonth = DateRangePickerConversion.monthStart(
                containing: date,
                timeZone: timeZone
            )
        }
    }

    private func shiftMonth(by delta: Int) {
        visibleMonth = DateRangePickerConversion.shiftingMonth(
            visibleMonth,
            by: delta,
            timeZone: timeZone
        )
    }

    private func canShiftForward(_ grid: CalendarMonthGrid) -> Bool {
        guard let cap = try? DateRangePickerConversion.date(
            from: maximumCivilDate,
            timeZone: timeZone
        ) else { return true }
        return DateRangePickerConversion.monthStart(containing: cap, timeZone: timeZone)
            > grid.monthStart
    }

    // MARK: - Presentation helpers

    private var gridColumns: [GridItem] {
        Array(repeating: GridItem(.fixed(Self.cellWidth), spacing: Self.gridSpacing), count: 7)
    }

    private var gridWidth: CGFloat {
        Self.cellWidth * 7 + Self.gridSpacing * 6
    }

    private var todayCivilDate: String {
        DateRangePickerConversion.today(timeZone: timeZone).startDate
    }

    private func bandHalf(_ filled: Bool) -> some View {
        Rectangle()
            .fill(filled ? Color.primary.opacity(0.12) : Color.clear)
            .frame(height: 30)
    }

    private func cellForeground(
        isEndpoint: Bool,
        isFuture: Bool,
        isInMonth: Bool
    ) -> Color {
        if isEndpoint {
            return Color(nsColor: .windowBackgroundColor)
        }
        if isFuture {
            return Color.secondary.opacity(0.35)
        }
        return isInMonth ? Color.primary : Color.secondary.opacity(0.6)
    }
}
