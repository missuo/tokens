import Foundation
import SwiftUI
import TokscaleMenuBarCore

struct TokensPopoverView: View {
    @ObservedObject var state: TokensMenuBarState
    let onReload: () -> Void
    let onRefreshScan: () -> Void
    let onOpenTokensCI: () -> Void
    let onRevealCache: () -> Void
    let onQuit: () -> Void

    var body: some View {
        ZStack {
            CompanionBackdrop(accent: accent)

            VStack(spacing: 10) {
                if let summary {
                    SummaryContent(
                        summary: summary,
                        isRefreshing: isRefreshing,
                        refreshStatus: refreshStatus,
                        onRefreshScan: onRefreshScan,
                        onOpenTokensCI: onOpenTokensCI,
                        onRevealCache: onRevealCache,
                        onQuit: onQuit
                    )
                } else {
                    EmptyContent(errorMessage: errorMessage)
                }
            }
            .padding(12)
        }
        .frame(width: 500, height: 680, alignment: .top)
    }

    private var summary: TokscaleSummary? {
        state.summary
    }

    private var errorMessage: String? {
        state.errorMessage
    }

    private var isRefreshing: Bool {
        state.isRefreshing
    }

    private var refreshStatus: String? {
        state.refreshStatus
    }

    private var accent: Color {
        guard let summary else {
            return .blue
        }
        let model = TokscaleDashboardModel(summary: summary)
        if let quota = model.quotaWindows.first {
            return providerColor(quota.provider)
        }
        if let provider = model.providers.first {
            return providerColor(provider.id)
        }
        return .blue
    }
}

private struct SummaryContent: View {
    let summary: TokscaleSummary
    let isRefreshing: Bool
    let refreshStatus: String?
    let onRefreshScan: () -> Void
    let onOpenTokensCI: () -> Void
    let onRevealCache: () -> Void
    let onQuit: () -> Void

    @State private var selectedProviderId: String?
    @State private var settingsVisible = false

    private var model: TokscaleDashboardModel {
        TokscaleDashboardModel(summary: summary)
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

            ScrollView(.vertical, showsIndicators: false) {
                VStack(spacing: 8) {
                    ProviderChipRow(
                        providers: model.providers,
                        selectedProviderId: selectedFocus.id,
                        onSelect: { providerId in
                            withAnimation(.spring(response: 0.26, dampingFraction: 0.86)) {
                                selectedProviderId = providerId
                            }
                        }
                    )

                    FocusHeroCard(
                        summary: summary,
                        model: model,
                        focus: selectedFocus,
                        isRefreshing: isRefreshing
                    )

                    DashboardSections(
                        summary: summary,
                        model: model,
                        focus: selectedFocus
                    )
                    .padding(.bottom, 2)
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
                    .font(.system(size: 13, weight: .semibold))
                Text(headerSubtitle)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            StatusCapsule(
                title: isRefreshing ? "Scanning" : model.health.title,
                color: isRefreshing ? .orange : (summary.stale ? .orange : .green),
                icon: isRefreshing ? "dot.radiowaves.left.and.right" : "bolt.fill"
            )
            HStack(spacing: 5) {
                HeaderIconButton(
                    systemName: isRefreshing ? "hourglass" : "arrow.clockwise",
                    tint: isRefreshing ? .orange : providerColor(focus.id),
                    active: isRefreshing,
                    disabled: isRefreshing,
                    help: isRefreshing ? "Scanning" : "Refresh scan",
                    action: onRefresh
                )
                HeaderIconButton(
                    systemName: "gearshape",
                    tint: providerColor(focus.id),
                    active: settingsVisible,
                    help: "Settings",
                    action: onToggleSettings
                )
            }
        }
        .frame(height: 30)
    }

    private var headerSubtitle: String {
        if let quota = focus.primaryQuota {
            let plan = quota.plan.map { " · \($0)" } ?? ""
            return "\(quota.provider) quota\(plan)"
        }
        return "\(focus.title) · \(focus.quotaStatus)"
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
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 7) {
                HStack(spacing: 6) {
                    ProviderDot(color: accent)
                    Text(focus.title)
                        .font(.system(size: 12, weight: .semibold))
                    Spacer(minLength: 0)
                    Text(primaryQuota?.title ?? focus.quotaStatus)
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.secondary)
                }

                Text(heroTitle)
                    .font(.system(size: 38, weight: .bold, design: .rounded))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.72)

                Text(heroSubtitle)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.78)

                HStack(spacing: 8) {
                    MiniMetric(
                        title: "Today",
                        value: formatToday(summary),
                        color: .blue
                    )
                    MiniMetric(
                        title: weeklyQuota?.title ?? "Messages",
                        value: weeklyQuota?.value ?? focus.messages,
                        color: weeklyQuota.map { providerColor($0.provider) } ?? .orange
                    )
                }
            }

            UsageArcGauge(
                progress: primaryQuota?.progress ?? focus.share,
                color: accent,
                centerTitle: gaugeTitle,
                centerSubtitle: primaryQuota == nil ? "share" : "quota",
                active: isRefreshing
            )
            .frame(width: 102, height: 102)
        }
        .padding(14)
        .frame(height: 136)
        .background(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(companionPanelColor)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(heroBorder, lineWidth: 1)
        )
        .overlay(alignment: .topTrailing) {
            Circle()
                .fill(accent.opacity(0.16))
                .frame(width: 92, height: 92)
                .blur(radius: 28)
                .offset(x: 20, y: -32)
        }
        .clipped()
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

    private var heroBorder: LinearGradient {
        LinearGradient(
            colors: [
                accent.opacity(0.55),
                Color.white.opacity(0.08),
                Color(nsColor: .separatorColor).opacity(0.2)
            ],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
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
                    .fill(color.opacity(selected ? 0.18 : 0.075))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .stroke(color.opacity(selected ? 0.55 : 0.14), lineWidth: 1)
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
                        color: .blue
                    )
                    VisualMetricPill(
                        title: focus.title,
                        value: focus.today.replacingOccurrences(of: " today", with: ""),
                        detail: focus.topModel,
                        progress: focus.share,
                        color: providerColor(focus.id)
                    )
                    VisualMetricPill(
                        title: "Tokens",
                        value: focus.tokens,
                        detail: focus.messages,
                        progress: focus.share,
                        color: .green
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
        DashboardCard(icon: "chart.bar", title: "History", color: .blue) {
            VStack(alignment: .leading, spacing: 7) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.historyPeak.map { "Peak \($0.value) on \($0.date)" } ?? "No history yet")
                        .font(.system(size: 11, weight: .semibold))
                        .lineLimit(1)
                        .minimumScaleFactor(0.8)
                }
                HistoryBars(days: model.historyTrend)
            }
        }
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
        HStack(spacing: 7) {
            SettingsStatusPill(
                title: summary.menuBarTitle,
                value: refreshStatus ?? model.health.detail,
                color: providerColor(focus.id)
            )
            ToolbarIconButton(systemName: "safari", tint: .green, help: "Open tokens.ci", action: onOpenTokensCI)
            ToolbarIconButton(systemName: "folder", tint: .purple, help: "Reveal cache", action: onRevealCache)
            ToolbarIconButton(systemName: "power", tint: .red, help: "Quit", action: onQuit)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 7)
        .background(
            RoundedRectangle(cornerRadius: 13, style: .continuous)
                .fill(companionPanelColor.opacity(0.98))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 13, style: .continuous)
                .stroke(providerColor(focus.id).opacity(0.2), lineWidth: 1)
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
                .fill(color.opacity(0.08))
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
                .background(Circle().fill(tint.opacity(active ? 0.18 : 0.1)))
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
                        .fill(tint.opacity(0.11))
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
        VStack(alignment: .leading, spacing: 7) {
            HStack(spacing: 6) {
                Image(systemName: icon)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(color)
                Text(title)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
            }
            content
        }
        .padding(9)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(panelBackground(color: color, intensity: 0.04))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(color.opacity(0.11), lineWidth: 1)
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
                .fill(color.opacity(0.075))
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
                .fill(color.opacity(0.075))
        )
    }
}

private struct HistoryBars: View {
    let days: [TokscaleDashboardModel.HistoryPoint]

    var body: some View {
        HStack(alignment: .bottom, spacing: 7) {
            ForEach(days, id: \.date) { day in
                VStack(spacing: 4) {
                    RoundedRectangle(cornerRadius: 4, style: .continuous)
                        .fill(historyColor(day.progress))
                        .frame(height: max(7, 48 * day.progress))
                    Text(String(day.date.suffix(2)))
                        .font(.system(size: 8, weight: .bold))
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: 62, alignment: .bottom)
                .help("\(day.date) · \(day.value) · \(day.messages)")
            }
        }
        .frame(height: 62)
    }

    private func historyColor(_ progress: Double) -> Color {
        Color.blue.opacity(progress > 0 ? 0.32 + 0.48 * progress : 0.14)
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
                .stroke(Color(nsColor: .separatorColor).opacity(0.28), style: StrokeStyle(lineWidth: 9, lineCap: .round))
                .rotationEffect(.degrees(90))

            Circle()
                .trim(from: 0.08, to: 0.08 + 0.84 * visibleProgress)
                .stroke(color, style: StrokeStyle(lineWidth: 9, lineCap: .round))
                .rotationEffect(.degrees(90))
                .shadow(color: color.opacity(0.25), radius: active ? 10 : 5)

            Circle()
                .fill(color.opacity(active ? 0.13 : 0.07))
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
    @State private var visibleProgress = 0.0

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color(nsColor: .separatorColor).opacity(0.24))
                Capsule()
                    .fill(color.opacity(0.78))
                    .frame(width: max(6, proxy.size.width * visibleProgress))
            }
        }
        .frame(height: 6)
        .onAppear {
            withAnimation(.spring(response: 0.5, dampingFraction: 0.84)) {
                visibleProgress = min(max(progress, 0), 1)
            }
        }
        .onChange(of: progress) { newValue in
            withAnimation(.spring(response: 0.32, dampingFraction: 0.86)) {
                visibleProgress = min(max(newValue, 0), 1)
            }
        }
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
            withAnimation(.easeInOut(duration: 0.9).repeatForever(autoreverses: false)) {
                pulse = true
            }
        }
    }

    private var dotColor: Color {
        active ? .orange : (stale ? .orange : .green)
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
        .background(panelBackground(color: .blue, intensity: 0.04))
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
            .background(panelBackground(color: .orange, intensity: 0.06))

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
                    accent.opacity(0.13),
                    companionPanelColor.opacity(0.62),
                    companionSurfaceColor
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        }
    }
}

private let companionSurfaceColor = Color(
    nsColor: NSColor(name: nil) { appearance in
        if appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua {
            return NSColor(calibratedRed: 0.095, green: 0.095, blue: 0.10, alpha: 1)
        }
        return NSColor(calibratedRed: 0.965, green: 0.965, blue: 0.955, alpha: 1)
    }
)

private let companionPanelColor = Color(
    nsColor: NSColor(name: nil) { appearance in
        if appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua {
            return NSColor(calibratedRed: 0.135, green: 0.132, blue: 0.128, alpha: 1)
        }
        return NSColor(calibratedRed: 0.995, green: 0.995, blue: 0.985, alpha: 1)
    }
)

private let companionSelectedSurfaceColor = Color(
    nsColor: NSColor(name: nil) { appearance in
        if appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua {
            return NSColor(calibratedRed: 0.19, green: 0.185, blue: 0.18, alpha: 1)
        }
        return NSColor(calibratedRed: 1, green: 1, blue: 0.995, alpha: 1)
    }
)

private func panelBackground(color: Color, intensity: Double) -> some ShapeStyle {
    LinearGradient(
        colors: [
            companionPanelColor,
            color.opacity(intensity)
        ],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
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

private func parseISODate(_ value: String) -> Date? {
    let fractional = ISO8601DateFormatter()
    fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = fractional.date(from: value) {
        return date
    }
    return ISO8601DateFormatter().date(from: value)
}

private func providerColor(_ id: String) -> Color {
    switch id.lowercased() {
    case "claude":
        return Color(red: 0.86, green: 0.43, blue: 0.17)
    case "codex":
        return Color(red: 0.20, green: 0.43, blue: 0.92)
    case "gemini":
        return Color(red: 0.16, green: 0.62, blue: 0.40)
    case "openclaw":
        return Color(red: 0.50, green: 0.32, blue: 0.82)
    case "copilot":
        return Color(red: 0.06, green: 0.52, blue: 0.56)
    case "antigravity":
        return Color(red: 0.76, green: 0.28, blue: 0.48)
    default:
        return .accentColor
    }
}
