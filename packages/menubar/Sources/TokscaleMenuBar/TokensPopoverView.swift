import Foundation
import SwiftUI
import TokscaleMenuBarCore

struct TokensPopoverView: View {
    @ObservedObject var model: MenuBarModel
    // Observed so changing the theme re-renders the whole popover and the accent
    // (read globally from this key) updates everywhere at once.
    @AppStorage(AppTheme.storageKey) private var themeRawValue = AppTheme.default.rawValue

    var body: some View {
        ZStack {
            CompanionBackdrop(accent: accent)

            VStack(spacing: 10) {
                if let summary, let dashboard = model.dashboard {
                    SummaryContent(
                        summary: summary,
                        dashboard: dashboard,
                        isRefreshing: isRefreshing,
                        refreshStatus: refreshStatus,
                        onRefreshScan: { model.refreshScan() },
                        onOpenTokensCI: { model.openTokensCI() },
                        onRevealCache: { model.revealCache() },
                        onQuit: { model.quit() }
                    )
                } else {
                    EmptyContent(errorMessage: errorMessage)
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 18)
            .padding(.bottom, 14)
        }
        .frame(width: 560, height: 920, alignment: .top)
        .clipShape(RoundedRectangle(cornerRadius: 26, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 26, style: .continuous)
                .stroke(companionOrange.opacity(0.28), lineWidth: 1)
        )
        .background {
            Button("Refresh") { model.refreshScan() }
                .keyboardShortcut("r", modifiers: .command)
                .hidden()
            Button("Open tokens.ci") { model.openTokensCI() }
                .keyboardShortcut("d", modifiers: .command)
                .hidden()
        }
    }

    private var summary: TokscaleSummary? {
        model.summary
    }

    private var errorMessage: String? {
        model.errorMessage
    }

    private var isRefreshing: Bool {
        model.isRefreshing
    }

    private var refreshStatus: String? {
        model.refreshStatus
    }

    private var accent: Color {
        guard let dashboard = model.dashboard else {
            return providerColor("gemini")
        }
        if let quota = dashboard.quotaWindows.first {
            return providerColor(quota.provider)
        }
        if let provider = dashboard.providers.first {
            return providerColor(provider.id)
        }
        return providerColor("gemini")
    }
}

private struct SummaryContent: View {
    let summary: TokscaleSummary
    let dashboard: TokscaleDashboardModel
    let isRefreshing: Bool
    let refreshStatus: String?
    let onRefreshScan: () -> Void
    let onOpenTokensCI: () -> Void
    let onRevealCache: () -> Void
    let onQuit: () -> Void

    @State private var selectedProviderId: String?
    @State private var settingsVisible = false
    @State private var quotaDisplayMode: TokscaleDashboardModel.QuotaDisplayMode = .remaining
    @State private var page = 0
    @AppStorage(LayoutMode.storageKey) private var layoutRawValue = LayoutMode.default.rawValue

    private var model: TokscaleDashboardModel {
        dashboard
    }

    private var layout: LayoutMode {
        LayoutMode(storedValue: layoutRawValue)
    }

    private var selectedFocus: TokscaleDashboardModel.ProviderFocus {
        model.providerFocus(for: selectedProviderId)
    }

    var body: some View {
        VStack(spacing: 8) {
            CompanionHeader(
                summary: summary,
                model: model,
                focus: selectedFocus,
                isRefreshing: isRefreshing,
                settingsVisible: settingsVisible,
                onRefresh: onRefreshScan,
                onToggleSettings: {
                    withAnimation(.spring(response: 0.24, dampingFraction: 0.88)) {
                        settingsVisible.toggle()
                    }
                }
            )

            if settingsVisible {
                CompactSettingsPanel(
                    summary: summary,
                    model: model,
                    focus: selectedFocus,
                    refreshStatus: refreshStatus,
                    onOpenTokensCI: onOpenTokensCI,
                    onRevealCache: onRevealCache,
                    onQuit: onQuit
                )
                .transition(.opacity.combined(with: .move(edge: .top)))
            }

            if layout == .paged {
                PageTabSwitcher(
                    page: page,
                    color: providerColor(selectedFocus.id),
                    onSelect: { target in
                        withAnimation(.spring(response: 0.24, dampingFraction: 0.88)) {
                            page = target
                        }
                    }
                )
            }

            ScrollView(.vertical, showsIndicators: false) {
                VStack(spacing: 8) {
                    if layout == .paged {
                        if page == 0 {
                            glanceSection
                        } else {
                            historySection
                        }
                    } else {
                        glanceSection
                        historySection
                    }
                }
                .frame(maxWidth: .infinity, alignment: .top)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
            .clipped()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .onAppear {
            syncSelectedProvider()
        }
        .onChange(of: model.providers) { _ in
            syncSelectedProvider()
        }
    }

    @ViewBuilder private var glanceSection: some View {
        QuotaBoardSection(
            summary: summary,
            model: model,
            displayMode: quotaDisplayMode,
            prominent: layout == .paged,
            onModeChange: { mode in
                withAnimation(.spring(response: 0.24, dampingFraction: 0.88)) {
                    quotaDisplayMode = mode
                }
            }
        )
    }

    @ViewBuilder private var historySection: some View {
        CompactOverviewStrip(model: model)

        HistorySection(model: model)
            .padding(.bottom, 2)

        ContributionHeatmap(days: summary.contribution)

        Text("Dollar amounts are API-equivalent value, not subscription spend.")
            .font(.system(size: 9, weight: .medium))
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.bottom, 4)
    }

    private func syncSelectedProvider() {
        if let selectedProviderId, model.providers.contains(where: { $0.id == selectedProviderId }) {
            return
        }
        selectedProviderId = model.providers.first?.id
    }
}

private struct CompanionHeader: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let focus: TokscaleDashboardModel.ProviderFocus
    let isRefreshing: Bool
    let settingsVisible: Bool
    let onRefresh: () -> Void
    let onToggleSettings: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            LiveDot(stale: summary.stale, active: isRefreshing)
            VStack(alignment: .leading, spacing: 1) {
                Text("Tokens")
                    .font(.system(size: 14, weight: .bold, design: .rounded))
                Text(headerSubtitle)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            StatusCapsule(
                title: isRefreshing ? "Scanning" : model.health.title,
                color: isRefreshing ? companionOrange : (summary.stale ? companionOrange : providerColor("codex")),
                icon: isRefreshing ? "dot.radiowaves.left.and.right" : "bolt.fill"
            )
            HStack(spacing: 5) {
                HeaderIconButton(
                    systemName: isRefreshing ? "hourglass" : "arrow.clockwise",
                    tint: isRefreshing ? companionOrange : companionOrange,
                    active: isRefreshing,
                    disabled: isRefreshing,
                    help: isRefreshing ? "Scanning" : "Refresh scan",
                    action: onRefresh
                )
                HeaderIconButton(
                    systemName: "gearshape",
                    tint: companionOrange,
                    active: settingsVisible,
                    help: "Settings",
                    action: onToggleSettings
                )
            }
        }
        .frame(height: 32)
    }

    private var headerSubtitle: String {
        if summary.stale {
            return "Cached quota monitor"
        }
        return "Quota monitor · history"
    }
}

private struct PageTabSwitcher: View {
    let page: Int
    let color: Color
    let onSelect: (Int) -> Void

    private let tabs = ["Glance", "History"]

    var body: some View {
        HStack(spacing: 4) {
            ForEach(Array(tabs.enumerated()), id: \.offset) { index, title in
                Button(action: { onSelect(index) }) {
                    Text(title)
                        .font(.system(size: 12, weight: .bold))
                        .frame(maxWidth: .infinity)
                        .frame(height: 30)
                        .foregroundStyle(index == page ? Color.primary : Color.secondary)
                        .background(
                            RoundedRectangle(cornerRadius: 9, style: .continuous)
                                .fill(index == page ? color.opacity(0.16) : Color.clear)
                        )
                        .overlay(
                            RoundedRectangle(cornerRadius: 9, style: .continuous)
                                .stroke(index == page ? color.opacity(0.32) : Color.clear, lineWidth: 1)
                        )
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(4)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(companionWarmGlassColor)
        )
    }
}

private struct QuotaBoardSection: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let displayMode: TokscaleDashboardModel.QuotaDisplayMode
    var prominent: Bool = false
    let onModeChange: (TokscaleDashboardModel.QuotaDisplayMode) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Quota")
                        .font(.system(size: prominent ? 27 : 22, weight: .bold, design: .rounded))
                        .foregroundStyle(
                            LinearGradient(
                                colors: [companionOrange, Color.primary],
                                startPoint: .leading,
                                endPoint: .trailing
                            )
                        )
                    Text(boardSubtitle)
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                QuotaModeToggle(mode: displayMode, onChange: onModeChange)
            }

            if let best = QuotaGlance.bestNow(in: summary.quota) {
                HStack(spacing: 5) {
                    Image(systemName: "arrow.right.circle.fill")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(providerColor(best.provider))
                    Text("Best now → \(best.provider) · \(Int(best.remainingPercent.rounded()))%")
                        .font(.system(size: 11, weight: .semibold))
                        .lineLimit(1)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
                .background(
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .fill(providerColor(best.provider).opacity(0.10))
                )
            }

            VStack(spacing: 9) {
                if model.quotaBoardProviders.isEmpty {
                    CompactEmptyMessage(
                        title: "No live quota",
                        detail: "Claude, Codex, and Gemini quota windows are unavailable in this summary.",
                        icon: "exclamationmark.triangle"
                    )
                } else {
                    ForEach(model.quotaBoardProviders, id: \.id) { focus in
                        if prominent {
                            ProviderGlanceCard(focus: focus, displayMode: displayMode)
                        } else {
                            ProviderQuotaRow(focus: focus, displayMode: displayMode)
                        }
                    }
                }
            }
        }
        .padding(15)
        .background(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(companionGlassPanelColor)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(companionOrange.opacity(0.18), lineWidth: 1)
        )
        .shadow(color: companionOrange.opacity(0.09), radius: 18, x: 0, y: 8)
    }

    private var boardSubtitle: String {
        if summary.stale {
            return "Cached data - refresh before trusting limits"
        }
        if model.quotaBoardProviders.allSatisfy({ $0.quotaWindows.isEmpty }),
           let warning = summary.health.warnings.first {
            return warning
        }
        return "Live quota windows - 5h and weekly"
    }
}

private struct QuotaModeToggle: View {
    let mode: TokscaleDashboardModel.QuotaDisplayMode
    let onChange: (TokscaleDashboardModel.QuotaDisplayMode) -> Void

    var body: some View {
        HStack(spacing: 3) {
            ForEach(TokscaleDashboardModel.QuotaDisplayMode.allCases, id: \.self) { option in
                Button(action: { onChange(option) }) {
                    Text(option.title)
                        .font(.system(size: 10, weight: .bold))
                        .lineLimit(1)
                        .frame(width: 46, height: 26)
                        .foregroundStyle(option == mode ? Color.primary : Color.secondary)
                        .background(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .fill(option == mode ? companionSelectedSurfaceColor : Color.clear)
                        )
                }
                .buttonStyle(.plain)
                .help(option == .remaining ? "Show remaining quota" : "Show used quota")
            }
        }
        .padding(3)
        .background(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(companionWarmGlassColor)
        )
    }
}

private struct ProviderQuotaRow: View {
    let focus: TokscaleDashboardModel.ProviderFocus
    let displayMode: TokscaleDashboardModel.QuotaDisplayMode
    var prominent: Bool = false

    private var color: Color {
        providerColor(focus.id)
    }

    var body: some View {
        HStack(spacing: 13) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    ProviderDot(color: color)
                    Text(focus.title)
                        .font(.system(size: prominent ? 14 : 12, weight: .bold))
                        .lineLimit(1)
                    Spacer(minLength: 0)
                }
                Text(focus.today)
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.72)
                Text("\(focus.total) · \(focus.tokens)")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.64)
                QuotaSourceBadge(status: focus.quotaStatus, color: color)
            }
            .frame(width: 116, alignment: .leading)

            VStack(spacing: 8) {
                if focus.quotaWindows.isEmpty {
                    QuotaUnavailableLine(providerColor: color)
                } else {
                    QuotaBarLine(
                        quota: focus.primaryQuota,
                        fallbackTitle: "5h",
                        displayMode: displayMode,
                        isCached: focus.quotaStatus == "Cached",
                        prominent: prominent
                    )
                    QuotaBarLine(
                        quota: focus.weeklyQuota,
                        fallbackTitle: "Week",
                        displayMode: displayMode,
                        isCached: focus.quotaStatus == "Cached",
                        prominent: prominent
                    )
                }
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 9)
        .background(
            RoundedRectangle(cornerRadius: 13, style: .continuous)
                .fill(companionWarmGlassColor)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 13, style: .continuous)
                .stroke(color.opacity(0.22), lineWidth: 1)
        )
    }
}

private struct QuotaBarLine: View {
    let quota: TokscaleDashboardModel.QuotaWindowSummary?
    let fallbackTitle: String
    let displayMode: TokscaleDashboardModel.QuotaDisplayMode
    let isCached: Bool
    var prominent: Bool = false

    var body: some View {
        if let quota {
            let expired = resetIsExpired(quota.reset)
            let unavailable = isCached || expired
            HStack(spacing: 8) {
                Text(quota.title)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(.secondary)
                    .frame(width: 38, alignment: .leading)
                QuotaProgressBar(
                    progress: unavailable ? 0 : quota.progress(for: displayMode),
                    color: unavailable ? companionOrange : quotaHealthColor(quota)
                )
                VStack(alignment: .trailing, spacing: 1) {
                    Text(quotaValueText(quota: quota, displayMode: displayMode, isCached: isCached, expired: expired))
                        .font(.system(size: prominent ? 21 : 15, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.72)
                    Text(quotaDetailText(quota: quota, displayMode: displayMode, isCached: isCached, expired: expired))
                        .font(.system(size: prominent ? 9 : 8, weight: .medium))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .minimumScaleFactor(0.70)
                }
                .frame(width: prominent ? 106 : 92, alignment: .trailing)
            }
            .frame(height: prominent ? 34 : 28)
        } else {
            HStack(spacing: 8) {
                Text(fallbackTitle)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(.secondary)
                    .frame(width: 38, alignment: .leading)
                QuotaProgressBar(progress: 0, color: .secondary)
                Text("N/A")
                    .font(.system(size: 12, weight: .bold, design: .rounded))
                    .foregroundStyle(.secondary)
                    .frame(width: 92, alignment: .trailing)
            }
            .frame(height: 28)
        }
    }
}

// The percentage already reads as a percent; drop the trailing "left"/"used"
// word so the value shows just "29%".
private func stripQuotaWord(_ text: String) -> String {
    text
        .replacingOccurrences(of: " left", with: "")
        .replacingOccurrences(of: " used", with: "")
}

private func quotaValueText(
    quota: TokscaleDashboardModel.QuotaWindowSummary,
    displayMode: TokscaleDashboardModel.QuotaDisplayMode,
    isCached: Bool,
    expired: Bool
) -> String {
    if expired {
        return "Expired"
    }
    if isCached {
        return "Cached"
    }
    return stripQuotaWord(quota.value(for: displayMode))
}

private func quotaDetailText(
    quota: TokscaleDashboardModel.QuotaWindowSummary,
    displayMode: TokscaleDashboardModel.QuotaDisplayMode,
    isCached: Bool,
    expired: Bool
) -> String {
    if expired {
        return "refresh needed"
    }
    if isCached {
        return "refresh for live"
    }
    return stripQuotaWord(resetLabel(quota.reset) ?? quota.detail(for: displayMode))
}

// ClaudeBar-style glance card: each quota window stacked vertically with a large
// number and a full-width progress bar, instead of cramming everything into one row.
private struct ProviderGlanceCard: View {
    let focus: TokscaleDashboardModel.ProviderFocus
    let displayMode: TokscaleDashboardModel.QuotaDisplayMode

    private var color: Color {
        providerColor(focus.id)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 7) {
                ProviderDot(color: color)
                Text(focus.title)
                    .font(.system(size: 16, weight: .bold))
                    .lineLimit(1)
                Spacer(minLength: 0)
                QuotaSourceBadge(status: focus.quotaStatus, color: color)
            }

            if focus.quotaWindows.isEmpty {
                QuotaUnavailableLine(providerColor: color)
            } else {
                GlanceQuotaLine(
                    quota: focus.primaryQuota,
                    fallbackTitle: "5-hour",
                    displayMode: displayMode,
                    isCached: focus.quotaStatus == "Cached",
                    color: color
                )
                GlanceQuotaLine(
                    quota: focus.weeklyQuota,
                    fallbackTitle: "Weekly",
                    displayMode: displayMode,
                    isCached: focus.quotaStatus == "Cached",
                    color: color
                )
            }
        }
        .padding(15)
        .background(
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .fill(companionWarmGlassColor)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .stroke(color.opacity(0.22), lineWidth: 1)
        )
    }
}

private struct GlanceQuotaLine: View {
    let quota: TokscaleDashboardModel.QuotaWindowSummary?
    let fallbackTitle: String
    let displayMode: TokscaleDashboardModel.QuotaDisplayMode
    let isCached: Bool
    let color: Color

    var body: some View {
        if let quota {
            let expired = resetIsExpired(quota.reset)
            let unavailable = isCached || expired
            VStack(alignment: .leading, spacing: 7) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(quota.title)
                        .font(.system(size: 13, weight: .bold))
                        .foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                    Text(quotaValueText(quota: quota, displayMode: displayMode, isCached: isCached, expired: expired))
                        .font(.system(size: 36, weight: .heavy, design: .rounded))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.5)
                        .foregroundStyle(unavailable ? Color.secondary : Color.primary)
                }
                QuotaProgressBar(
                    progress: unavailable ? 0 : quota.progress(for: displayMode),
                    color: unavailable ? companionOrange : quotaHealthColor(quota)
                )
                .frame(height: 15)
                Text(quotaDetailText(quota: quota, displayMode: displayMode, isCached: isCached, expired: expired))
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
        } else {
            VStack(alignment: .leading, spacing: 7) {
                HStack(alignment: .firstTextBaseline) {
                    Text(fallbackTitle)
                        .font(.system(size: 13, weight: .bold))
                        .foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                    Text("N/A")
                        .font(.system(size: 36, weight: .heavy, design: .rounded))
                        .foregroundStyle(.secondary)
                }
                QuotaProgressBar(progress: 0, color: .secondary)
                    .frame(height: 15)
            }
        }
    }
}

private struct QuotaUnavailableLine: View {
    let providerColor: Color

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "lock.open.trianglebadge.exclamationmark")
                .font(.system(size: 13, weight: .bold))
                .foregroundStyle(providerColor)
            VStack(alignment: .leading, spacing: 1) {
                Text("No live quota")
                    .font(.system(size: 11, weight: .bold))
                Text("Local usage is available, official 5h/Week limits are not.")
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.72)
            }
            Spacer(minLength: 0)
        }
        .frame(height: 59)
        .padding(.horizontal, 9)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color(nsColor: .separatorColor).opacity(0.075))
        )
    }
}

private struct QuotaProgressBar: View {
    let progress: Double
    let color: Color

    var body: some View {
        GeometryReader { proxy in
            let clampedProgress = min(max(progress, 0), 1)
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: 5, style: .continuous)
                    .fill(Color(nsColor: .separatorColor).opacity(0.18))
                RoundedRectangle(cornerRadius: 5, style: .continuous)
                    .fill(color)
                    .frame(width: max(7, proxy.size.width * clampedProgress))
                    .shadow(color: color.opacity(0.16), radius: 5, x: 0, y: 0)
            }
        }
        .frame(height: 11)
    }
}

private struct QuotaSourceBadge: View {
    let status: String
    let color: Color

    var body: some View {
        Text(status)
            .font(.system(size: 8, weight: .bold))
            .lineLimit(1)
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .foregroundStyle(status == "No live quota" ? Color.secondary : color)
            .background(
                Capsule()
                    .fill((status == "No live quota" ? Color.secondary : color).opacity(0.12))
            )
    }
}

private struct CompactOverviewStrip: View {
    let model: TokscaleDashboardModel

    var body: some View {
        HStack(spacing: 7) {
            ForEach(Array(model.spendHighlights.enumerated()), id: \.offset) { index, item in
                VisualMetricPill(
                    title: item.title,
                    value: item.value,
                    detail: item.detail,
                    progress: spendProgress(index: index),
                    color: spendColor(index: index)
                )
            }
        }
    }

    private func spendProgress(index: Int) -> Double {
        switch index {
        case 0:
            return model.hero.progress
        case 1:
            return 1
        default:
            let current = model.currentWeekTrend.reduce(0) { $0 + $1.costUsd }
            let previous = model.previousWeekTrend.reduce(0) { $0 + $1.costUsd }
            guard max(current, previous) > 0 else {
                return 0
            }
            return min(current / max(current, previous), 1)
        }
    }

    private func spendColor(index: Int) -> Color {
        switch index {
        case 0:
            return providerColor("gemini")
        case 1:
            return companionOrange
        default:
            return providerColor("codex")
        }
    }
}

private struct FocusHeroCard: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let focus: TokscaleDashboardModel.ProviderFocus
    let isRefreshing: Bool

    private var primaryQuota: TokscaleDashboardModel.QuotaWindowSummary? {
        focus.primaryQuota
    }

    private var weeklyQuota: TokscaleDashboardModel.QuotaWindowSummary? {
        focus.weeklyQuota
    }

    private var accent: Color {
        providerColor(focus.id)
    }

    var body: some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 6) {
                    ProviderDot(color: accent)
                    Text(focus.title)
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                }

                VStack(alignment: .leading, spacing: 1) {
                    Text(heroTitle)
                        .font(.system(size: 40, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                        .foregroundStyle(
                            LinearGradient(
                                colors: [.primary, .primary.opacity(0.85)],
                                startPoint: .top,
                                endPoint: .bottom
                            )
                        )

                    Text(heroSubtitle)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }

                Spacer(minLength: 2)

                HStack(spacing: 12) {
                    MiniMetric(
                        title: "Today",
                        value: formatToday(summary),
                        color: providerColor("gemini")
                    )
                    MiniMetric(
                        title: weeklyQuota?.title ?? "Messages",
                        value: weeklyQuota?.value ?? focus.messages,
                        color: weeklyQuota.map { providerColor($0.provider) } ?? providerColor("openclaw")
                    )
                }
            }

            UsageArcGauge(
                progress: primaryQuota?.progress ?? focus.share,
                color: accent,
                centerTitle: gaugeTitle,
                centerSubtitle: primaryQuota == nil ? "SHARE" : "QUOTA",
                active: isRefreshing
            )
            .frame(width: 104, height: 104)
        }
        .padding(16)
        .frame(height: 142)
        .background(
            LinearGradient(
                colors: [
                    companionPanelColor,
                    accent.opacity(0.055)
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(
                    LinearGradient(
                        colors: [accent.opacity(0.25), Color.primary.opacity(0.05)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ),
                    lineWidth: 1
                )
        )
        .shadow(color: Color.black.opacity(0.08), radius: 12, x: 0, y: 6)
    }

    private var heroTitle: String {
        primaryQuota?.value ?? focus.today
    }

    private var heroSubtitle: String {
        if let primaryQuota {
            let reset = resetLabel(primaryQuota.reset).map { " · reset \($0)" } ?? ""
            return "\(primaryQuota.detail)\(reset)"
        }
        return focus.topModel
    }

    private var gaugeTitle: String {
        if let primaryQuota {
            return "\(Int((primaryQuota.progress * 100).rounded()))%"
        }
        return "\(Int((focus.share * 100).rounded()))%"
    }
}

private struct ProviderChipRow: View {
    let providers: [TokscaleDashboardModel.ProviderSummary]
    let selectedProviderId: String?
    let onSelect: (String) -> Void

    var body: some View {
        HStack(spacing: 7) {
            if providers.isEmpty {
                ProviderChipPlaceholder()
            } else {
                ForEach(providers.prefix(5), id: \.id) { provider in
                    ProviderChip(
                        provider: provider,
                        selected: provider.id == selectedProviderId,
                        onSelect: { onSelect(provider.id) }
                    )
                }
            }
        }
        .frame(height: 64)
    }
}

private struct ProviderChip: View {
    let provider: TokscaleDashboardModel.ProviderSummary
    let selected: Bool
    let onSelect: () -> Void

    private var color: Color {
        providerColor(provider.id)
    }

    var body: some View {
        Button(action: onSelect) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 5) {
                    ProviderDot(color: color)
                    Text(provider.label)
                        .font(.system(size: 11, weight: .semibold))
                        .lineLimit(1)
                        .minimumScaleFactor(0.74)
                }
                Text(provider.value)
                    .font(.system(size: 11, weight: .bold, design: .rounded))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.76)
                ProgressBar(progress: provider.share, color: color)
                Text(provider.detail)
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 9)
            .padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(color.opacity(selected ? 0.14 : 0.055))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .stroke(color.opacity(selected ? 0.34 : 0.10), lineWidth: 1)
            )
            .scaleEffect(selected ? 1.015 : 1)
            .animation(.spring(response: 0.2, dampingFraction: 0.86), value: selected)
        }
        .buttonStyle(.plain)
        .help(provider.label)
    }
}

private struct ProviderChipPlaceholder: View {
    var body: some View {
        HStack(spacing: 8) {
            ProviderDot(color: .secondary)
            VStack(alignment: .leading, spacing: 3) {
                Text("No provider data")
                    .font(.system(size: 11, weight: .semibold))
                Text("Run a refresh scan to populate clients")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(panelBackground(color: .secondary, intensity: 0.04))
    }
}

private struct DashboardSections: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let focus: TokscaleDashboardModel.ProviderFocus

    var body: some View {
        VStack(spacing: 7) {
            OverviewSection(summary: summary, model: model, focus: focus)
            LimitsSection(focus: focus)
            HistorySection(model: model)
        }
        .frame(maxWidth: .infinity, alignment: .top)
    }
}

private struct OverviewSection: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let focus: TokscaleDashboardModel.ProviderFocus

    var body: some View {
        DashboardCard(icon: "chart.pie", title: "Overview", color: providerColor(focus.id)) {
            VStack(spacing: 7) {
                HStack(spacing: 7) {
                    VisualMetricPill(
                        title: "Today",
                        value: formatToday(summary),
                        detail: model.hero.progressLabel,
                        progress: model.hero.progress,
                        color: providerColor("gemini")
                    )
                    VisualMetricPill(
                        title: focus.title,
                        value: focus.today.replacingOccurrences(of: " today", with: ""),
                        detail: focus.topModel,
                        progress: focus.share,
                        color: providerColor(focus.id)
                    )
                }

                ProviderShareMeter(
                    title: "Provider share",
                    value: "\(Int((focus.share * 100).rounded()))%",
                    progress: focus.share,
                    color: providerColor(focus.id)
                )
            }
        }
    }
}

private struct LimitsSection: View {
    let focus: TokscaleDashboardModel.ProviderFocus

    var body: some View {
        DashboardCard(icon: "gauge.with.dots.needle.67percent", title: "Limits", color: providerColor(focus.id)) {
            if focus.quotaWindows.isEmpty {
                CompactEmptyMessage(
                    title: "No official quota",
                    detail: "\(focus.title) has local usage data, but no 5h or weekly limit in cache.",
                    icon: "gauge.with.dots.needle.67percent"
                )
            } else {
                HStack(spacing: 7) {
                    ForEach(Array(focus.quotaWindows.prefix(2).enumerated()), id: \.offset) { _, quota in
                        LimitMiniCard(quota: quota)
                    }
                }
            }
        }
    }
}

private struct HistorySection: View {
    let model: TokscaleDashboardModel

    var body: some View {
        VStack(alignment: .leading, spacing: 11) {
            HStack(spacing: 7) {
                Image(systemName: "chart.bar.fill")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(companionOrange)
                Text("History")
                    .font(.system(size: 13, weight: .bold))
                Spacer(minLength: 0)
                Text("14d · est. API value")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }

            HStack(spacing: 8) {
                HistoryStatPill(
                    title: "All-time",
                    value: model.spendHighlights[safe: 1]?.value ?? "$0.00",
                    detail: model.spendHighlights[safe: 1]?.detail ?? "0 tokens"
                )
                HistoryStatPill(
                    title: "7d",
                    value: model.spendHighlights[safe: 2]?.value ?? "$0.00",
                    detail: model.spendHighlights[safe: 2]?.detail ?? "No prior data"
                )
                HistoryStatPill(
                    title: "Peak",
                    value: model.historyPeak?.value ?? "$0.00",
                    detail: model.historyPeak?.date ?? "No history"
                )
            }

            HistoryBars(previousDays: model.previousWeekTrend, currentDays: model.currentWeekTrend)
        }
        .padding(15)
        .background(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(companionGlassPanelColor)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(companionOrange.opacity(0.16), lineWidth: 1)
        )
    }

}

private struct HistoryStatPill: View {
    let title: String
    let value: String
    let detail: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(size: 20, weight: .bold, design: .rounded))
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.70)
            Text(detail)
                .font(.system(size: 8, weight: .medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .minimumScaleFactor(0.72)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 9)
        .padding(.vertical, 7)
        .background(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(companionWarmGlassColor)
        )
    }
}

private struct QuotaWindowRow: View {
    let quota: TokscaleDashboardModel.QuotaWindowSummary

    private var color: Color {
        providerColor(quota.provider)
    }

    var body: some View {
        VStack(spacing: 6) {
            HStack(spacing: 8) {
                ProviderDot(color: color)
                Text(quota.title)
                    .font(.system(size: 12, weight: .semibold))
                Spacer()
                Text(quota.value)
                    .font(.system(size: 13, weight: .bold, design: .rounded))
                    .monospacedDigit()
            }
            ProgressBar(progress: quota.progress, color: color)
            HStack {
                Text(quota.detail)
                Spacer()
                Text(resetLabel(quota.reset) ?? quota.plan ?? quota.provider)
            }
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(.secondary)
            .lineLimit(1)
        }
        .padding(10)
        .background(panelBackground(color: color, intensity: 0.06))
    }
}

private struct CompactSettingsPanel: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let focus: TokscaleDashboardModel.ProviderFocus
    let refreshStatus: String?
    let onOpenTokensCI: () -> Void
    let onRevealCache: () -> Void
    let onQuit: () -> Void

    var body: some View {
        VStack(spacing: 7) {
            HStack(spacing: 7) {
                SettingsStatusPill(
                    title: summary.menuBarTitle,
                    value: refreshStatus ?? model.health.warning ?? model.health.detail,
                    color: providerColor(focus.id)
                )
                ToolbarIconButton(systemName: "safari", tint: providerColor("codex"), help: "Open tokens.ci", action: onOpenTokensCI)
                ToolbarIconButton(systemName: "folder", tint: providerColor("openclaw"), help: "Reveal cache", action: onRevealCache)
                ToolbarIconButton(systemName: "power", tint: providerColor("claude"), help: "Quit", action: onQuit)
            }
            LayoutModeRow(color: providerColor(focus.id))
            RefreshCadenceRow(color: providerColor(focus.id))
            AutoRefreshRow(color: providerColor(focus.id))
            ThemeRow(color: providerColor(focus.id))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 7)
        .background(
            RoundedRectangle(cornerRadius: 13, style: .continuous)
                .fill(companionPanelColor.opacity(0.98))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 13, style: .continuous)
                .stroke(providerColor(focus.id).opacity(0.14), lineWidth: 1)
        )
    }
}

private struct LayoutModeRow: View {
    let color: Color
    @AppStorage(LayoutMode.storageKey) private var layoutRawValue = LayoutMode.default.rawValue

    private var layout: LayoutMode {
        LayoutMode(storedValue: layoutRawValue)
    }

    var body: some View {
        HStack(spacing: 7) {
            HStack(spacing: 6) {
                Image(systemName: "rectangle.split.1x2")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(color)
                Text("Layout")
                    .font(.system(size: 10, weight: .bold))
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            SettingsSegment(
                titles: LayoutMode.allCases.map(\.title),
                selectedIndex: LayoutMode.allCases.firstIndex(of: layout) ?? 0,
                onSelect: { layoutRawValue = LayoutMode.allCases[$0].rawValue },
                help: "Single scrolling dashboard, or a two-page glance + history"
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

private struct AutoRefreshRow: View {
    let color: Color
    @AppStorage(AutoRefresh.storageKey) private var autoRawValue = AutoRefresh.default.rawValue

    private var auto: AutoRefresh {
        AutoRefresh(storedValue: autoRawValue)
    }

    var body: some View {
        HStack(spacing: 7) {
            HStack(spacing: 6) {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(color)
                Text("Auto-refresh")
                    .font(.system(size: 10, weight: .bold))
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            SettingsSegment(
                titles: AutoRefresh.allCases.map(\.title),
                selectedIndex: AutoRefresh.allCases.firstIndex(of: auto) ?? 0,
                onSelect: { autoRawValue = AutoRefresh.allCases[$0].rawValue },
                help: "Background quota auto-refresh interval"
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

private struct ThemeRow: View {
    let color: Color
    @AppStorage(AppTheme.storageKey) private var themeRawValue = AppTheme.default.rawValue

    private var theme: AppTheme {
        AppTheme(storedValue: themeRawValue)
    }

    var body: some View {
        HStack(spacing: 7) {
            HStack(spacing: 6) {
                Image(systemName: "paintpalette.fill")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(color)
                Text("Theme")
                    .font(.system(size: 10, weight: .bold))
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            SettingsSegment(
                titles: AppTheme.allCases.map(\.title),
                selectedIndex: AppTheme.allCases.firstIndex(of: theme) ?? 0,
                onSelect: { themeRawValue = AppTheme.allCases[$0].rawValue },
                help: "Accent color theme (applies on reopen)"
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

private struct SettingsSegment: View {
    let titles: [String]
    let selectedIndex: Int
    let onSelect: (Int) -> Void
    let help: String

    var body: some View {
        HStack(spacing: 3) {
            ForEach(Array(titles.enumerated()), id: \.offset) { index, title in
                Button(action: { onSelect(index) }) {
                    Text(title)
                        .font(.system(size: 10, weight: .bold))
                        .lineLimit(1)
                        .frame(width: 44, height: 24)
                        .foregroundStyle(index == selectedIndex ? Color.primary : Color.secondary)
                        .background(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .fill(index == selectedIndex ? companionSelectedSurfaceColor : Color.clear)
                        )
                }
                .buttonStyle(.plain)
                .help(help)
            }
        }
        .padding(3)
        .background(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(companionWarmGlassColor)
        )
    }
}

private struct SettingsStatusPill: View {
    let title: String
    let value: String
    let color: Color

    var body: some View {
        HStack(spacing: 7) {
            ProviderDot(color: color)
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.system(size: 10, weight: .bold))
                    .lineLimit(1)
                Text(value)
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 9)
        .padding(.vertical, 5)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(color.opacity(0.055))
        )
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

private struct DashboardCard<Content: View>: View {
    let icon: String
    let title: String
    let color: Color
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 6) {
                Image(systemName: icon)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(color)
                Text(title.uppercased())
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
            }
            content
        }
        .padding(12)
        .background(
            panelBackground(color: color, intensity: 0.02)
                .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(Color.primary.opacity(0.04), lineWidth: 1)
        )
    }
}

private struct VisualMetricPill: View {
    let title: String
    let value: String
    let detail: String
    let progress: Double
    let color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(size: 14, weight: .bold, design: .rounded))
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.68)
            ProgressBar(progress: progress, color: color)
            Text(detail)
                .font(.system(size: 8, weight: .medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .minimumScaleFactor(0.65)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(color.opacity(0.045))
        )
    }
}

private struct ProviderShareMeter: View {
    let title: String
    let value: String
    let progress: Double
    let color: Color

    var body: some View {
        HStack(spacing: 8) {
            Text(title)
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(.secondary)
            ProgressBar(progress: progress, color: color)
            Text(value)
                .font(.system(size: 10, weight: .bold, design: .rounded))
                .monospacedDigit()
                .frame(width: 34, alignment: .trailing)
        }
        .frame(height: 12)
    }
}

private struct CompactEmptyMessage: View {
    let title: String
    let detail: String
    let icon: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
                .font(.system(size: 13, weight: .bold))
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.system(size: 10, weight: .bold))
                Text(detail)
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.72)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(Color(nsColor: .separatorColor).opacity(0.07))
        )
    }
}

private struct LimitMiniCard: View {
    let quota: TokscaleDashboardModel.QuotaWindowSummary

    private var color: Color {
        providerColor(quota.provider)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack {
                Text(quota.title)
                    .font(.system(size: 10, weight: .bold))
                Spacer()
                Text(quota.value)
                    .font(.system(size: 11, weight: .bold, design: .rounded))
                    .monospacedDigit()
            }
            ProgressBar(progress: quota.progress, color: color)
            Text(resetLabel(quota.reset) ?? quota.detail)
                .font(.system(size: 9, weight: .medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .minimumScaleFactor(0.72)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 8)
        .padding(.vertical, 7)
        .background(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(color.opacity(0.045))
        )
    }
}

private struct HistoryBars: View {
    let previousDays: [TokscaleDashboardModel.HistoryPoint]
    let currentDays: [TokscaleDashboardModel.HistoryPoint]

    private let lastWeekColor = Color.gray.opacity(0.55)

    var body: some View {
        let all = previousDays + currentDays
        let maxCost = max(all.map(\.costUsd).max() ?? 0, 1)
        let byDate = Dictionary(all.map { ($0.date, $0) }, uniquingKeysWith: { first, _ in first })
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 12) {
                legend(color: companionOrange, label: "This week")
                legend(color: lastWeekColor, label: "Last week")
                Spacer(minLength: 0)
            }
            HStack(alignment: .bottom, spacing: 7) {
                ForEach(currentDays, id: \.date) { day in
                    let previous = priorWeekDate(day.date).flatMap { byDate[$0] }
                    VStack(spacing: 4) {
                        HStack(alignment: .bottom, spacing: 2) {
                            bar(previous?.costUsd ?? 0, maxCost: maxCost, color: lastWeekColor)
                            bar(day.costUsd, maxCost: maxCost, color: companionOrange)
                        }
                        .frame(maxHeight: 96, alignment: .bottom)
                        Text(String(day.date.suffix(2)))
                            .font(.system(size: 9, weight: .bold))
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, maxHeight: 116, alignment: .bottom)
                    .help(historyHelp(day: day, previous: previous))
                }
            }
        }
    }

    private func bar(_ cost: Double, maxCost: Double, color: Color) -> some View {
        RoundedRectangle(cornerRadius: 3, style: .continuous)
            .fill(color)
            .frame(width: 13, height: max(3, 96 * min(max(cost / maxCost, 0), 1)))
    }

    private func legend(color: Color, label: String) -> some View {
        HStack(spacing: 4) {
            RoundedRectangle(cornerRadius: 2, style: .continuous)
                .fill(color)
                .frame(width: 9, height: 9)
            Text(label)
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.secondary)
        }
    }

    private func priorWeekDate(_ dateString: String) -> String? {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC") ?? .current
        let formatter = DateFormatter()
        formatter.calendar = calendar
        formatter.timeZone = calendar.timeZone
        formatter.dateFormat = "yyyy-MM-dd"
        guard let date = formatter.date(from: dateString),
            let prior = calendar.date(byAdding: .day, value: -7, to: date)
        else {
            return nil
        }
        return formatter.string(from: prior)
    }

    private func historyHelp(
        day: TokscaleDashboardModel.HistoryPoint,
        previous: TokscaleDashboardModel.HistoryPoint?
    ) -> String {
        guard let previous else {
            return "\(day.date) · \(day.value) · \(day.messages)"
        }
        return "\(day.date) · \(day.value) · last wk \(previous.value)"
    }
}

private struct ContributionHeatmap: View {
    let days: [TokscaleSummary.ContributionDay]

    private let cell: CGFloat = 9
    private let spacing: CGFloat = 2

    var body: some View {
        let grid = Self.buildGrid(days)
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 7) {
                Image(systemName: "square.grid.3x3.fill")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(companionOrange)
                Text("Activity")
                    .font(.system(size: 13, weight: .bold))
                Spacer(minLength: 0)
                Text("\(days.count) active days")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            if grid.isEmpty {
                CompactEmptyMessage(
                    title: "No activity yet",
                    detail: "Run `tokens submit` to build your contribution history.",
                    icon: "square.grid.3x3"
                )
            } else {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(alignment: .top, spacing: spacing) {
                        ForEach(Array(grid.enumerated()), id: \.offset) { _, week in
                            VStack(spacing: spacing) {
                                ForEach(Array(week.enumerated()), id: \.offset) { _, level in
                                    RoundedRectangle(cornerRadius: 2, style: .continuous)
                                        .fill(color(level))
                                        .frame(width: cell, height: cell)
                                }
                            }
                        }
                    }
                    HStack(spacing: 4) {
                        Text("Less").font(.system(size: 8)).foregroundStyle(.secondary)
                        ForEach(0...4, id: \.self) { level in
                            RoundedRectangle(cornerRadius: 2, style: .continuous)
                                .fill(color(level))
                                .frame(width: 8, height: 8)
                        }
                        Text("More").font(.system(size: 8)).foregroundStyle(.secondary)
                    }
                }
            }
        }
        .padding(15)
        .background(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(companionGlassPanelColor)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(companionOrange.opacity(0.16), lineWidth: 1)
        )
    }

    private func color(_ level: Int) -> Color {
        switch level {
        case 1: return companionOrange.opacity(0.42)
        case 2: return companionOrange.opacity(0.64)
        case 3: return companionOrange.opacity(0.82)
        case 4: return companionOrange.opacity(1.0)
        case -1: return Color.clear
        default: return Color.gray.opacity(0.20)
        }
    }

    // Calendar grid as columns of 7 (Sun..Sat). -1 pads days outside the data range,
    // 0 is an in-range day with no usage, 1-4 are intensity buckets.
    static func buildGrid(_ days: [TokscaleSummary.ContributionDay]) -> [[Int]] {
        guard let firstDay = days.first, let lastDay = days.last else {
            return []
        }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC") ?? .current
        let formatter = DateFormatter()
        formatter.calendar = calendar
        formatter.timeZone = calendar.timeZone
        formatter.dateFormat = "yyyy-MM-dd"

        let byDate = Dictionary(days.map { ($0.date, $0.costUsd) }, uniquingKeysWith: { first, _ in first })
        // Bucket by quartiles of active-day cost so shades spread evenly instead of
        // clumping at the faintest level under a few outlier days.
        let sortedCosts = days.map(\.costUsd).filter { $0 > 0 }.sorted()
        func bucket(_ cost: Double) -> Int {
            guard cost > 0 else { return 0 }
            guard !sortedCosts.isEmpty else { return 1 }
            func threshold(_ p: Double) -> Double {
                sortedCosts[min(sortedCosts.count - 1, Int((Double(sortedCosts.count) - 1) * p))]
            }
            if cost <= threshold(0.25) { return 1 }
            if cost <= threshold(0.50) { return 2 }
            if cost <= threshold(0.75) { return 3 }
            return 4
        }
        guard let first = formatter.date(from: firstDay.date),
            let last = formatter.date(from: lastDay.date)
        else {
            return []
        }

        let firstWeekday = calendar.component(.weekday, from: first) - 1
        guard let gridStart = calendar.date(byAdding: .day, value: -firstWeekday, to: first) else {
            return []
        }

        var columns: [[Int]] = []
        var current: [Int] = []
        var day = gridStart
        while day <= last {
            let weekday = calendar.component(.weekday, from: day) - 1
            let level = day < first ? -1 : bucket(byDate[formatter.string(from: day)] ?? 0)
            current.append(level)
            if weekday == 6 {
                columns.append(current)
                current = []
            }
            guard let next = calendar.date(byAdding: .day, value: 1, to: day) else {
                break
            }
            day = next
        }
        if !current.isEmpty {
            while current.count < 7 {
                current.append(-1)
            }
            columns.append(current)
        }
        return columns
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        guard indices.contains(index) else {
            return nil
        }
        return self[index]
    }
}

private struct UsageArcGauge: View {
    let progress: Double
    let color: Color
    let centerTitle: String
    let centerSubtitle: String
    let active: Bool

    @State private var visibleProgress = 0.0
    @State private var pulse = false

    var body: some View {
        ZStack {
            Circle()
                .trim(from: 0.08, to: 0.92)
                .stroke(color.opacity(0.12), style: StrokeStyle(lineWidth: 9, lineCap: .round))
                .rotationEffect(.degrees(90))

            Circle()
                .trim(from: 0.08, to: 0.08 + 0.84 * visibleProgress)
                .stroke(color, style: StrokeStyle(lineWidth: 9, lineCap: .round))
                .rotationEffect(.degrees(90))
                .shadow(color: color.opacity(active ? 0.18 : 0.10), radius: active ? 10 : 5)

            Circle()
                .fill(color.opacity(active ? 0.09 : 0.045))
                .frame(width: pulse && active ? 82 : 64, height: pulse && active ? 82 : 64)
                .animation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true), value: pulse)

            VStack(spacing: 1) {
                Text(centerTitle)
                    .font(.system(size: 18, weight: .bold, design: .rounded))
                    .monospacedDigit()
                Text(centerSubtitle)
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
        }
        .onAppear {
            withAnimation(.spring(response: 0.65, dampingFraction: 0.82)) {
                visibleProgress = min(max(progress, 0), 1)
            }
            pulse = true
        }
        .onChange(of: progress) { newValue in
            withAnimation(.spring(response: 0.42, dampingFraction: 0.86)) {
                visibleProgress = min(max(newValue, 0), 1)
            }
        }
    }
}

private struct ProgressBar: View {
    let progress: Double
    let color: Color

    var body: some View {
        GeometryReader { proxy in
            let clampedProgress = min(max(progress, 0), 1)
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color(nsColor: .separatorColor).opacity(0.24))
                Capsule()
                    .fill(color.opacity(0.78))
                    .frame(width: max(6, proxy.size.width * clampedProgress))
            }
        }
        .frame(height: 6)
    }
}

private struct CompactStatTile: View {
    let title: String
    let value: String
    let detail: String
    let color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 5) {
                ProviderDot(color: color)
                Text(title)
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
            Text(value)
                .font(.system(size: 19, weight: .bold, design: .rounded))
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.72)
            Text(detail)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .minimumScaleFactor(0.72)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background(panelBackground(color: color, intensity: 0.055))
    }
}

private struct SignalChip: View {
    let title: String
    let value: String
    let color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(size: 11, weight: .semibold))
                .lineLimit(1)
                .minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(panelBackground(color: color, intensity: 0.05))
    }
}

private struct MiniMetric: View {
    let title: String
    let value: String
    let color: Color

    var body: some View {
        HStack(spacing: 5) {
            Circle()
                .fill(color)
                .frame(width: 5, height: 5)
            Text(title)
                .foregroundStyle(.secondary)
            Text(value)
                .fontWeight(.semibold)
                .monospacedDigit()
        }
        .font(.system(size: 10, weight: .medium))
        .lineLimit(1)
        .minimumScaleFactor(0.75)
    }
}

private struct StatusCapsule: View {
    let title: String
    let color: Color
    let icon: String

    var body: some View {
        HStack(spacing: 5) {
            Image(systemName: icon)
                .font(.system(size: 10, weight: .bold))
            Text(title)
                .font(.system(size: 10, weight: .bold))
                .lineLimit(1)
        }
        .foregroundStyle(color)
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
        .background(Capsule().fill(color.opacity(0.12)))
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
        .onAppear {
            updatePulse()
        }
        .onChange(of: active) { _ in
            updatePulse()
        }
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

private struct ProviderDot: View {
    let color: Color

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 7, height: 7)
            .shadow(color: color.opacity(0.25), radius: 3)
    }
}

private struct EmptyPaneMessage: View {
    let title: String
    let detail: String
    let icon: String

    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: icon)
                .font(.system(size: 20, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(title)
                .font(.system(size: 13, weight: .semibold))
            Text(detail)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(12)
        .background(panelBackground(color: providerColor("gemini"), intensity: 0.04))
    }
}

private struct EmptyContent: View {
    let errorMessage: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 10) {
                LiveDot(stale: true, active: false)
                VStack(alignment: .leading, spacing: 1) {
                    Text("Tokens")
                        .font(.system(size: 14, weight: .semibold))
                    Text("Local companion")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(.secondary)
                }
            }

            Spacer(minLength: 12)

            VStack(alignment: .leading, spacing: 8) {
                Text("No summary")
                    .font(.system(size: 34, weight: .bold, design: .rounded))
                Text(errorMessage ?? "Run a companion summary refresh once, then reload this view.")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(16)
            .background(panelBackground(color: providerColor("claude"), intensity: 0.06))

            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

private struct CompanionBackdrop: View {
    let accent: Color

    var body: some View {
        ZStack {
            companionSurfaceColor

            LinearGradient(
                colors: [
                    companionOrange.opacity(0.18),
                    accent.opacity(0.06),
                    companionPanelColor.opacity(0.44),
                    companionSurfaceColor
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        }
    }
}

private var companionOrange: Color {
    let theme = AppTheme(storedValue: UserDefaults.standard.string(forKey: AppTheme.storageKey))
    let hsb = theme.accentHSB
    return Color(hue: hsb.hue, saturation: hsb.saturation, brightness: hsb.brightness)
}

private let companionSurfaceColor = Color(
    nsColor: NSColor(name: nil) { appearance in
        if appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua {
            return NSColor(calibratedRed: 0.075, green: 0.062, blue: 0.052, alpha: 1.0)
        }
        return NSColor(calibratedRed: 0.985, green: 0.955, blue: 0.920, alpha: 1.0)
    }
)

private let companionPanelColor = Color(
    nsColor: NSColor(name: nil) { appearance in
        if appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua {
            return NSColor(calibratedRed: 0.135, green: 0.112, blue: 0.095, alpha: 1.0)
        }
        return NSColor(calibratedRed: 1.0, green: 0.975, blue: 0.945, alpha: 1.0)
    }
)

private let companionSelectedSurfaceColor = Color(
    nsColor: NSColor(name: nil) { appearance in
        if appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua {
            return NSColor(calibratedRed: 0.245, green: 0.158, blue: 0.110, alpha: 1.0)
        }
        return NSColor(calibratedRed: 1.0, green: 0.895, blue: 0.800, alpha: 1.0)
    }
)

private let companionGlassPanelColor = Color(
    nsColor: NSColor(name: nil) { appearance in
        if appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua {
            return NSColor(calibratedRed: 0.155, green: 0.130, blue: 0.112, alpha: 0.92)
        }
        return NSColor(calibratedRed: 1.0, green: 0.972, blue: 0.938, alpha: 0.92)
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

private func panelBackground(color: Color, intensity: Double) -> some View {
    ZStack {
        companionPanelColor
        color.opacity(intensity)
    }
}

private func formatToday(_ summary: TokscaleSummary) -> String {
    formatUSD(summary.today.costUsd)
}

private func formatUSD(_ value: Double) -> String {
    if abs(value) >= 1_000 {
        return String(format: "$%.1fK", value / 1_000)
    }
    return String(format: "$%.2f", value)
}

private func formatTokens(_ value: Int64) -> String {
    if value >= 1_000_000_000 {
        return compact(Double(value) / 1_000_000_000, suffix: "B")
    }
    if value >= 1_000_000 {
        return compact(Double(value) / 1_000_000, suffix: "M")
    }
    if value >= 1_000 {
        return compact(Double(value) / 1_000, suffix: "K")
    }
    return "\(value)"
}

private func compact(_ value: Double, suffix: String) -> String {
    let formatted = String(format: "%.1f", value)
    if formatted.hasSuffix(".0") {
        return "\(formatted.dropLast(2))\(suffix)"
    }
    return "\(formatted)\(suffix)"
}

private func resetLabel(_ value: String?) -> String? {
    guard let value else {
        return nil
    }
    let date = parseISODate(value)
    guard let date else {
        return value
    }
    let seconds = Int(date.timeIntervalSinceNow.rounded())
    if seconds <= 0 {
        return "now"
    }
    let hours = seconds / 3600
    let minutes = (seconds % 3600) / 60
    if hours > 0 {
        return "in \(hours)h \(minutes)m"
    }
    return "in \(max(minutes, 1))m"
}

private func resetIsExpired(_ value: String?) -> Bool {
    guard let value, let date = parseISODate(value) else {
        return false
    }
    return date.timeIntervalSinceNow <= 0
}

private func parseISODate(_ value: String) -> Date? {
    let fractional = ISO8601DateFormatter()
    fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = fractional.date(from: value) {
        return date
    }
    return ISO8601DateFormatter().date(from: value)
}

private func quotaHealthColor(_ quota: TokscaleDashboardModel.QuotaWindowSummary) -> Color {
    if quota.remainingPercent <= 0 {
        return .secondary
    }
    if quota.remainingPercent < 10 {
        return Color(hue: 0.01, saturation: 0.92, brightness: 0.96)
    }
    if quota.remainingPercent < 20 {
        return companionOrange
    }
    if quota.remainingPercent < 50 {
        return Color(hue: 0.12, saturation: 0.96, brightness: 0.98)
    }
    return Color(hue: 0.39, saturation: 0.88, brightness: 0.86)
}

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
