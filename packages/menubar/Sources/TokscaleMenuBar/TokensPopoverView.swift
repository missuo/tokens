import AppKit
import SwiftUI
import TokscaleMenuBarCore

struct TokensPopoverView: View {
    @ObservedObject var model: MenuBarModel
    @State private var settingsVisible = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            header
            if settingsVisible {
                settingsPanel
            }
            if let summary = model.summary {
                quotaRows(summary)
                footer(summary)
            } else {
                Text(model.errorMessage ?? "No data yet. Run `tokens submit` once to generate it.")
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(14)
        .frame(width: 300, alignment: .leading)
    }

    private var header: some View {
        HStack(spacing: 8) {
            LiveDot(stale: model.summary?.stale ?? true, active: model.isRefreshing)
            Text("Tokens")
                .font(.system(size: 14, weight: .bold, design: .rounded))
            Spacer()
            HeaderIconButton(
                systemName: model.isRefreshing ? "hourglass" : "arrow.clockwise",
                tint: companionOrange,
                active: model.isRefreshing,
                disabled: model.isRefreshing,
                help: "Refresh scan"
            ) { model.refreshScan() }
            HeaderIconButton(
                systemName: "gearshape",
                tint: companionOrange,
                active: settingsVisible,
                help: "Settings"
            ) { settingsVisible.toggle() }
        }
    }

    private var settingsPanel: some View {
        VStack(spacing: 7) {
            HStack(spacing: 7) {
                ToolbarIconButton(systemName: "safari", tint: providerColor("codex"), help: "Open tokens.ci") { model.openTokensCI() }
                ToolbarIconButton(systemName: "folder", tint: providerColor("openclaw"), help: "Reveal cache") { model.revealCache() }
                ToolbarIconButton(systemName: "power", tint: providerColor("claude"), help: "Quit") { model.quit() }
                Spacer(minLength: 0)
            }
            RefreshCadenceRow(color: providerColor("codex"))
        }
    }

    @ViewBuilder
    private func quotaRows(_ summary: TokscaleSummary) -> some View {
        let order = QuotaGlance.providersByUrgency(summary.quota)
        let constrained = QuotaGlance.mostConstrained(in: summary.quota)?.provider
        if order.isEmpty {
            Text("No live quota windows available.")
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
        } else {
            VStack(spacing: 6) {
                ForEach(order, id: \.self) { name in
                    if let provider = summary.quota.first(where: { $0.provider == name }) {
                        GlanceQuotaRow(provider: provider, isMostConstrained: name == constrained)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func footer(_ summary: TokscaleSummary) -> some View {
        let sevenDay = QuotaGlance.recentSpend(summary.history, days: 7)
        VStack(alignment: .leading, spacing: 3) {
            Text("Today \(usd(summary.today.costUsd)) · 7d \(usd(sevenDay))")
                .font(.system(size: 11, weight: .semibold))
            if let best = QuotaGlance.bestNow(in: summary.quota) {
                Text("Best now → \(best.provider) \(Int(best.remainingPercent.rounded()))%")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func usd(_ value: Double) -> String {
        "$" + String(Int(value.rounded()))
    }
}

private struct GlanceQuotaRow: View {
    let provider: TokscaleSummary.QuotaProvider
    let isMostConstrained: Bool

    private var color: Color { providerColor(provider.provider) }

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            Text(provider.provider)
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(color)
                .frame(width: 54, alignment: .leading)
            VStack(spacing: 3) {
                ForEach(provider.windows, id: \.label) { window in
                    windowRow(window)
                }
            }
        }
        .padding(.vertical, 5)
        .padding(.horizontal, 8)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(isMostConstrained ? color.opacity(0.12) : Color.clear)
        )
    }

    private func windowRow(_ window: TokscaleSummary.QuotaWindow) -> some View {
        HStack(spacing: 5) {
            Text(shortLabel(window.label))
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(width: 18, alignment: .leading)
            QuotaBar(usedPercent: window.usedPercent, color: color)
            Text("\(Int(window.remainingPercent.rounded()))%")
                .font(.system(size: 9, weight: .medium))
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(width: 32, alignment: .trailing)
            Text(countdownLabel(window.resetsAt))
                .font(.system(size: 9))
                .foregroundStyle(.secondary)
                .frame(width: 34, alignment: .trailing)
        }
    }

    private func shortLabel(_ label: String) -> String {
        label.lowercased().contains("week") ? "wk" : "5h"
    }

    private func countdownLabel(_ resetsAt: String?) -> String {
        guard let countdown = QuotaGlance.resetCountdown(from: resetsAt) else { return "" }
        return "⏱" + countdown
    }
}

private struct QuotaBar: View {
    let usedPercent: Double
    let color: Color

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(color.opacity(0.16))
                Capsule()
                    .fill(color)
                    .frame(width: geo.size.width * fraction)
            }
        }
        .frame(height: 6)
    }

    private var fraction: Double {
        min(max(usedPercent / 100, 0), 1)
    }
}

private struct LiveDot: View {
    let stale: Bool
    let active: Bool
    @State private var pulse = false

    var body: some View {
        ZStack {
            Circle()
                .stroke(dotColor.opacity(active ? 0.35 : 0), lineWidth: 2)
                .frame(width: pulse ? 19 : 9, height: pulse ? 19 : 9)
                .opacity(pulse ? 0 : 1)
            Circle()
                .fill(dotColor)
                .frame(width: 8, height: 8)
        }
        .frame(width: 19, height: 19)
        .onAppear { updatePulse() }
        .onChange(of: active) { _ in updatePulse() }
    }

    private var dotColor: Color {
        active ? providerColor("openclaw") : (stale ? providerColor("claude") : providerColor("codex"))
    }

    private func updatePulse() {
        guard active else {
            pulse = false
            return
        }
        pulse = false
        withAnimation(.easeInOut(duration: 0.9).repeatForever(autoreverses: false)) {
            pulse = true
        }
    }
}

private struct HeaderIconButton: View {
    let systemName: String
    let tint: Color
    var active = false
    var disabled = false
    let help: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: 12, weight: .bold))
                .frame(width: 26, height: 26)
                .foregroundStyle(disabled ? .secondary.opacity(0.5) : tint)
                .background(Circle().fill(tint.opacity(active ? 0.14 : 0.075)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .help(help)
    }
}

private struct ToolbarIconButton: View {
    let systemName: String
    let tint: Color
    let help: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: 12, weight: .bold))
                .frame(width: 28, height: 28)
                .foregroundStyle(tint)
                .background(
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .fill(tint.opacity(0.075))
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
    }
}

private struct RefreshCadenceRow: View {
    let color: Color
    @AppStorage(RefreshCadence.storageKey) private var cadenceRawValue = RefreshCadence.default.rawValue

    private var cadence: RefreshCadence {
        RefreshCadence(storedValue: cadenceRawValue)
    }

    var body: some View {
        HStack(spacing: 7) {
            HStack(spacing: 6) {
                Image(systemName: "clock.arrow.circlepath")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(color)
                Text("Refresh on open")
                    .font(.system(size: 10, weight: .bold))
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            RefreshCadenceToggle(
                selected: cadence,
                onChange: { cadenceRawValue = $0.rawValue }
            )
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 5)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(color.opacity(0.055))
        )
    }
}

private struct RefreshCadenceToggle: View {
    let selected: RefreshCadence
    let onChange: (RefreshCadence) -> Void

    var body: some View {
        HStack(spacing: 3) {
            ForEach(RefreshCadence.allCases, id: \.self) { option in
                Button(action: { onChange(option) }) {
                    Text(option.title)
                        .font(.system(size: 10, weight: .bold))
                        .lineLimit(1)
                        .frame(width: 46, height: 24)
                        .foregroundStyle(option == selected ? Color.primary : Color.secondary)
                        .background(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .fill(option == selected ? companionSelectedSurfaceColor : Color.clear)
                        )
                }
                .buttonStyle(.plain)
                .help("Quota refresh cadence when the menu opens")
            }
        }
        .padding(3)
        .background(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(companionWarmGlassColor)
        )
    }
}

private let companionOrange = Color(hue: 0.065, saturation: 0.96, brightness: 0.98)

private let companionSelectedSurfaceColor = Color(
    nsColor: NSColor(name: nil) { appearance in
        if appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua {
            return NSColor(calibratedRed: 0.245, green: 0.158, blue: 0.110, alpha: 1.0)
        }
        return NSColor(calibratedRed: 1.0, green: 0.895, blue: 0.800, alpha: 1.0)
    }
)

private let companionWarmGlassColor = Color(
    nsColor: NSColor(name: nil) { appearance in
        if appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua {
            return NSColor(calibratedRed: 0.235, green: 0.172, blue: 0.125, alpha: 0.58)
        }
        return NSColor(calibratedRed: 1.0, green: 0.910, blue: 0.830, alpha: 0.58)
    }
)

private func providerColor(_ id: String) -> Color {
    switch id.lowercased() {
    case "claude":
        return companionOrange
    case "codex":
        return Color(hue: 0.40, saturation: 0.76, brightness: 0.84)
    case "gemini":
        return Color(hue: 0.60, saturation: 0.76, brightness: 0.96)
    case "openclaw":
        return Color(hue: 0.74, saturation: 0.90, brightness: 0.96)
    case "copilot":
        return Color(hue: 0.48, saturation: 0.88, brightness: 0.86)
    case "antigravity":
        return Color(hue: 0.84, saturation: 0.86, brightness: 0.95)
    default:
        return .accentColor
    }
}
