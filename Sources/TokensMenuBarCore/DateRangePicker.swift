import AppKit
import Foundation
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

    public static func pickerValues(
        for range: DateSelectionRange,
        timeZone: TimeZone
    ) throws -> (dateValue: Date, timeInterval: TimeInterval) {
        let start = try date(from: range.startDate, timeZone: timeZone)
        let end = try date(from: range.endDate, timeZone: timeZone)
        guard end >= start else { throw ConversionError.invalidCivilDate(range.endDate) }
        return (start, end.timeIntervalSince(start))
    }

    public static func selection(
        dateValue: Date,
        timeInterval: TimeInterval,
        timeZone: TimeZone
    ) -> DateSelectionRange {
        let calendar = calendar(timeZone: timeZone)
        let start = calendar.startOfDay(for: dateValue)
        let endInstant = dateValue.addingTimeInterval(max(0, timeInterval))
        let end = calendar.startOfDay(for: endInstant)
        return DateSelectionRange(
            startDate: civilDate(from: start, timeZone: timeZone),
            endDate: civilDate(from: max(start, end), timeZone: timeZone)
        )
    }

    public static func today(now: Date = Date(), timeZone: TimeZone) -> DateSelectionRange {
        let value = civilDate(from: now, timeZone: timeZone)
        return DateSelectionRange(startDate: value, endDate: value)
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

    public static func maximumDate(now: Date = Date(), timeZone: TimeZone) -> Date? {
        let calendar = calendar(timeZone: timeZone)
        let start = calendar.startOfDay(for: now)
        guard let tomorrow = calendar.date(byAdding: .day, value: 1, to: start) else { return nil }
        return tomorrow.addingTimeInterval(-1)
    }
}

/// Native contiguous AppKit date range selection hosted in SwiftUI.
public struct AppKitDateRangePicker: NSViewRepresentable {
    @Binding private var selection: DateSelectionRange
    @Binding private var requestFocus: Bool
    private let timeZone: TimeZone
    private let locale: Locale
    private let maximumDate: Date

    public init(
        selection: Binding<DateSelectionRange>,
        requestFocus: Binding<Bool>,
        timeZone: TimeZone,
        locale: Locale = .current,
        maximumDate: Date = Date()
    ) {
        _selection = selection
        _requestFocus = requestFocus
        self.timeZone = timeZone
        self.locale = locale
        self.maximumDate = maximumDate
    }

    public func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    public func makeNSView(context: Context) -> NSDatePicker {
        let picker = NSDatePicker()
        picker.datePickerStyle = .clockAndCalendar
        picker.datePickerMode = .range
        picker.datePickerElements = .yearMonthDay
        picker.calendar = DateRangePickerConversion.calendar(timeZone: timeZone)
        picker.timeZone = timeZone
        picker.locale = locale
        picker.maxDate = maximumDate
        picker.target = context.coordinator
        picker.action = #selector(Coordinator.selectionChanged(_:))
        apply(selection, to: picker)
        picker.setAccessibilityLabel("Custom usage date range")
        return picker
    }

    public func updateNSView(_ picker: NSDatePicker, context: Context) {
        context.coordinator.parent = self
        picker.calendar = DateRangePickerConversion.calendar(timeZone: timeZone)
        picker.timeZone = timeZone
        picker.locale = locale
        picker.maxDate = maximumDate

        let current = DateRangePickerConversion.selection(
            dateValue: picker.dateValue,
            timeInterval: picker.timeInterval,
            timeZone: timeZone
        )
        if current != selection {
            apply(selection, to: picker)
        }

        if requestFocus, picker.window?.firstResponder !== picker {
            context.coordinator.requestFocus(in: picker)
        }
    }

    private func apply(_ range: DateSelectionRange, to picker: NSDatePicker) {
        guard let values = try? DateRangePickerConversion.pickerValues(
            for: range,
            timeZone: timeZone
        ) else { return }
        picker.dateValue = values.dateValue
        picker.timeInterval = values.timeInterval
    }

    public final class Coordinator: NSObject {
        fileprivate var parent: AppKitDateRangePicker
        private var focusAttemptScheduled = false

        fileprivate init(parent: AppKitDateRangePicker) {
            self.parent = parent
        }

        fileprivate func requestFocus(in picker: NSDatePicker, remainingAttempts: Int = 20) {
            guard parent.requestFocus, !focusAttemptScheduled else { return }
            focusAttemptScheduled = true
            DispatchQueue.main.async { [weak self, weak picker] in
                guard let self, let picker else { return }
                self.focusAttemptScheduled = false
                guard self.parent.requestFocus else { return }
                if let window = picker.window, window.makeFirstResponder(picker) {
                    self.parent.requestFocus = false
                } else if remainingAttempts > 0 {
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.01) { [weak self, weak picker] in
                        guard let self, let picker else { return }
                        self.requestFocus(
                            in: picker,
                            remainingAttempts: remainingAttempts - 1
                        )
                    }
                }
            }
        }

        @objc fileprivate func selectionChanged(_ sender: NSDatePicker) {
            parent.selection = DateRangePickerConversion.selection(
                dateValue: sender.dateValue,
                timeInterval: sender.timeInterval,
                timeZone: parent.timeZone
            )
        }
    }
}
