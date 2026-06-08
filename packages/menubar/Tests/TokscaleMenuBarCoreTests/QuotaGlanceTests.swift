import XCTest

@testable import TokscaleMenuBarCore

final class QuotaGlanceTests: XCTestCase {
    private func day(_ date: String, _ cost: Double) -> TokscaleSummary.HistoryDay {
        TokscaleSummary.HistoryDay(date: date, costUsd: cost, tokens: 0, messages: 0)
    }

    private func window(_ label: String, remaining: Double, resetsAt: String? = nil)
        -> TokscaleSummary.QuotaWindow
    {
        TokscaleSummary.QuotaWindow(
            label: label,
            usedPercent: 100 - remaining,
            remainingPercent: remaining,
            remainingLabel: nil,
            resetsAt: resetsAt
        )
    }

    private func provider(_ name: String, _ windows: [TokscaleSummary.QuotaWindow])
        -> TokscaleSummary.QuotaProvider
    {
        TokscaleSummary.QuotaProvider(provider: name, plan: nil, windows: windows)
    }

    private func iso(_ value: String) throws -> Date {
        try XCTUnwrap(ISO8601DateFormatter().date(from: value))
    }

    func testRecentSpendSumsLastNDays() {
        let history = [
            day("2026-05-30", 10), day("2026-05-31", 20), day("2026-06-01", 30),
            day("2026-06-02", 40), day("2026-06-03", 50), day("2026-06-04", 60),
            day("2026-06-05", 70), day("2026-06-06", 80),
        ]
        XCTAssertEqual(QuotaGlance.recentSpend(history, days: 7), 350, accuracy: 0.001)
        XCTAssertEqual(QuotaGlance.recentSpend(history, days: 100), 360, accuracy: 0.001)
        XCTAssertEqual(QuotaGlance.recentSpend([], days: 7), 0, accuracy: 0.001)
    }

    func testUrgencyThresholds() {
        // Aligned with ClaudeBar: >50 healthy, 20-50 warning, <20 critical, 0 depleted (by remaining).
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 60), .healthy)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 51), .healthy)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 50), .warning)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 20), .warning)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 19), .critical)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 1), .critical)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 0), .depleted)
    }

    func testResetCountdownFormats() throws {
        let now = try iso("2026-06-05T00:00:00Z")
        XCTAssertEqual(QuotaGlance.resetCountdown(from: "2026-06-05T00:30:00Z", now: now), "30m")
        XCTAssertEqual(QuotaGlance.resetCountdown(from: "2026-06-05T02:00:00Z", now: now), "2h")
        XCTAssertEqual(QuotaGlance.resetCountdown(from: "2026-06-06T12:00:00Z", now: now), "1d")
        XCTAssertEqual(QuotaGlance.resetCountdown(from: "2026-06-05T00:00:30Z", now: now), "1m")
        XCTAssertNil(QuotaGlance.resetCountdown(from: "2026-06-04T23:00:00Z", now: now))
        XCTAssertNil(QuotaGlance.resetCountdown(from: nil, now: now))
        XCTAssertNil(QuotaGlance.resetCountdown(from: "not-a-date", now: now))
    }

    func testMostConstrainedPicksGlobalLowestRemaining() {
        let providers = [
            provider("Claude", [window("Session", remaining: 28), window("Weekly", remaining: 59)]),
            provider("Codex", [window("Session", remaining: 12), window("Weekly", remaining: 70)]),
        ]
        let result = QuotaGlance.mostConstrained(in: providers)
        XCTAssertEqual(result?.provider, "Codex")
        XCTAssertEqual(result?.remainingPercent, 12)
        XCTAssertNil(QuotaGlance.mostConstrained(in: []))
    }

    func testBestNowPicksProviderWithHighestMinRemaining() {
        let providers = [
            provider("Claude", [window("Session", remaining: 28), window("Weekly", remaining: 59)]),
            provider("Codex", [window("Session", remaining: 12), window("Weekly", remaining: 70)]),
            provider("Gemini", [window("Session", remaining: 80), window("Weekly", remaining: 40)]),
        ]
        let result = QuotaGlance.bestNow(in: providers)
        XCTAssertEqual(result?.provider, "Gemini")
        XCTAssertEqual(result?.remainingPercent, 40)
    }

    func testProvidersByUrgencySortsAndDropsEmpty() {
        let providers = [
            provider("Claude", [window("Session", remaining: 28)]),
            provider("Codex", [window("Session", remaining: 12)]),
            provider("Empty", []),
            provider("Gemini", [window("Session", remaining: 80)]),
        ]
        XCTAssertEqual(QuotaGlance.providersByUrgency(providers), ["Codex", "Claude", "Gemini"])
    }

    func testAlertsFireOnWorseningIntoWarningOrCritical() {
        let healthy = [provider("Claude", [window("Session", remaining: 60)])]
        let (a1, l1) = QuotaGlance.alerts(previous: [:], quota: healthy)
        XCTAssertTrue(a1.isEmpty)
        XCTAssertEqual(l1["Claude|Session"], .healthy)

        let critical = [provider("Claude", [window("Session", remaining: 15)])]
        let (a2, l2) = QuotaGlance.alerts(previous: l1, quota: critical)
        XCTAssertEqual(a2.count, 1)
        XCTAssertEqual(a2.first?.provider, "Claude")
        XCTAssertEqual(a2.first?.level, .critical)
        XCTAssertEqual(l2["Claude|Session"], .critical)

        let (a3, _) = QuotaGlance.alerts(previous: l2, quota: critical)
        XCTAssertTrue(a3.isEmpty, "staying critical should not re-alert")

        let (a4, l4) = QuotaGlance.alerts(previous: l2, quota: healthy)
        XCTAssertTrue(a4.isEmpty, "recovery should not alert")
        XCTAssertEqual(l4["Claude|Session"], .healthy)

        let warning = [provider("Claude", [window("Session", remaining: 40)])]
        let (a5, _) = QuotaGlance.alerts(previous: l4, quota: warning)
        XCTAssertEqual(a5.count, 1)
        XCTAssertEqual(a5.first?.level, .warning)
    }

    func testAlertsTreatMissingBaselineAsHealthy() {
        let critical = [provider("Codex", [window("Weekly", remaining: 12)])]
        let (alerts, _) = QuotaGlance.alerts(previous: [:], quota: critical)
        XCTAssertEqual(alerts.count, 1)
        XCTAssertEqual(alerts.first?.level, .critical)
    }
}
