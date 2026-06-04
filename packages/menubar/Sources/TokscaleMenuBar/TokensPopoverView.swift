import Foundation
import SwiftUI
import TokscaleMenuBarCore

struct TokensPopoverView: View {
    let summary: TokscaleSummary?
    let errorMessage: String?
    let isRefreshing: Bool
    let refreshStatus: String?
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
                        refreshStatus: refreshStatus
                    )
                } else {
                    EmptyContent(errorMessage: errorMessage)
                }

                ActionDock(
                    isRefreshing: isRefreshing,
                    onReload: onReload,
                    onRefreshScan: onRefreshScan,
                    onOpenTokensCI: onOpenTokensCI,
                    onRevealCache: onRevealCache,
                    onQuit: onQuit
                )
            }
            .padding(12)
        }
        .frame(width: 420, height: 460, alignment: .top)
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

private enum CompanionPanel: String, CaseIterable, Identifiable {
    case quota = "Quota"
    case today = "Today"
    case history = "History"
    case clients = "Clients"

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .quota:
            return "gauge.with.dots.needle.67percent"
        case .today:
            return "calendar"
        case .history:
            return "chart.bar"
        case .clients:
            return "square.grid.2x2"
        }
    }
}

private struct SummaryContent: View {
    let summary: TokscaleSummary
    let isRefreshing: Bool
    let refreshStatus: String?

    @Namespace private var panelNamespace
    @State private var selectedPanel = CompanionPanel.quota

    private var model: TokscaleDashboardModel {
        TokscaleDashboardModel(summary: summary)
    }

    var body: some View {
        VStack(spacing: 8) {
            CompanionHeader(
                summary: summary,
                model: model,
                isRefreshing: isRefreshing
            )

            FocusHeroCard(
                summary: summary,
                model: model,
                isRefreshing: isRefreshing
            )

            PanelSwitcher(
                selectedPanel: $selectedPanel,
                namespace: panelNamespace,
                quotaAvailable: !model.quotaWindows.isEmpty
            )

            DynamicDetailPane(
                panel: selectedPanel,
                summary: summary,
                model: model,
                refreshStatus: refreshStatus
            )
            .transition(.opacity.combined(with: .scale(scale: 0.985)))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .onAppear {
            if model.quotaWindows.isEmpty {
                selectedPanel = .today
            }
        }
        .onChange(of: model.quotaWindows) { quotaWindows in
            if quotaWindows.isEmpty, selectedPanel == .quota {
                selectedPanel = .today
            }
        }
    }
}

private struct CompanionHeader: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let isRefreshing: Bool

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
        }
        .frame(height: 30)
    }

    private var headerSubtitle: String {
        if let quota = model.quotaWindows.first {
            let plan = quota.plan.map { " · \($0)" } ?? ""
            return "\(quota.provider) quota\(plan)"
        }
        return "\(model.clientLabels.count) AI clients · local cache"
    }
}

private struct FocusHeroCard: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let isRefreshing: Bool

    private var primaryQuota: TokscaleDashboardModel.QuotaWindowSummary? {
        model.quotaWindows.first { $0.title.lowercased() == "session" } ?? model.quotaWindows.first
    }

    private var weeklyQuota: TokscaleDashboardModel.QuotaWindowSummary? {
        model.quotaWindows.first { $0.title.lowercased() == "weekly" }
    }

    private var accent: Color {
        if let primaryQuota {
            return providerColor(primaryQuota.provider)
        }
        return .blue
    }

    var body: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 7) {
                HStack(spacing: 6) {
                    ProviderDot(color: accent)
                    Text(primaryQuota?.provider ?? "Today")
                        .font(.system(size: 12, weight: .semibold))
                    Spacer(minLength: 0)
                    Text(primaryQuota?.title ?? "Local usage")
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
                        value: weeklyQuota?.value ?? "\(summary.today.messages)",
                        color: weeklyQuota.map { providerColor($0.provider) } ?? .orange
                    )
                }
            }

            UsageArcGauge(
                progress: primaryQuota?.progress ?? model.hero.progress,
                color: accent,
                centerTitle: gaugeTitle,
                centerSubtitle: primaryQuota == nil ? "daily avg" : "quota",
                active: isRefreshing
            )
            .frame(width: 102, height: 102)
        }
        .padding(14)
        .frame(height: 122)
        .background(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(Color(nsColor: .controlBackgroundColor).opacity(0.9))
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
        primaryQuota?.value ?? formatToday(summary)
    }

    private var heroSubtitle: String {
        if let primaryQuota {
            let reset = resetLabel(primaryQuota.reset).map { " · reset \($0)" } ?? ""
            return "\(primaryQuota.detail)\(reset)"
        }
        return "\(formatTokens(summary.today.tokens)) tokens · \(summary.today.messages) messages"
    }

    private var gaugeTitle: String {
        if let primaryQuota {
            return "\(Int((primaryQuota.progress * 100).rounded()))%"
        }
        return "\(Int((model.hero.progress * 100).rounded()))%"
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

private struct PanelSwitcher: View {
    @Binding var selectedPanel: CompanionPanel
    let namespace: Namespace.ID
    let quotaAvailable: Bool

    private var panels: [CompanionPanel] {
        quotaAvailable ? CompanionPanel.allCases : [.today, .history, .clients]
    }

    var body: some View {
        HStack(spacing: 4) {
            ForEach(panels) { panel in
                Button {
                    withAnimation(.spring(response: 0.24, dampingFraction: 0.86)) {
                        selectedPanel = panel
                    }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: panel.icon)
                            .font(.system(size: 10, weight: .semibold))
                        Text(panel.rawValue)
                            .font(.system(size: 10, weight: .semibold))
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 7)
                    .foregroundStyle(selectedPanel == panel ? Color.primary : Color.secondary)
                    .background {
                        if selectedPanel == panel {
                            Capsule()
                                .fill(Color(nsColor: .windowBackgroundColor).opacity(0.94))
                                .matchedGeometryEffect(id: "panel-pill", in: namespace)
                                .shadow(color: .black.opacity(0.08), radius: 7, y: 2)
                        }
                    }
                }
                .buttonStyle(.plain)
            }
        }
        .padding(4)
        .background(
            Capsule()
                .fill(Color(nsColor: .controlBackgroundColor).opacity(0.72))
        )
        .frame(height: 34)
    }
}

private struct DynamicDetailPane: View {
    let panel: CompanionPanel
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let refreshStatus: String?

    var body: some View {
        ZStack {
            switch panel {
            case .quota:
                QuotaPane(model: model)
            case .today:
                TodayPane(summary: summary, model: model)
            case .history:
                HistoryPane(model: model)
            case .clients:
                ClientsPane(model: model)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 150, maxHeight: 160, alignment: .top)
        .animation(.spring(response: 0.28, dampingFraction: 0.86), value: panel)
    }
}

private struct QuotaPane: View {
    let model: TokscaleDashboardModel

    var body: some View {
        VStack(spacing: 8) {
            if model.quotaWindows.isEmpty {
                EmptyPaneMessage(
                    title: "No quota data",
                    detail: "Run Scan after Claude login to fetch 5h and weekly limits.",
                    icon: "gauge.with.dots.needle.67percent"
                )
            } else {
                ForEach(Array(model.quotaWindows.prefix(2).enumerated()), id: \.offset) { _, quota in
                    QuotaWindowRow(quota: quota)
                }
            }
        }
    }
}

private struct TodayPane: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 8) {
                CompactStatTile(
                    title: "Cost",
                    value: formatToday(summary),
                    detail: model.hero.progressLabel,
                    color: .blue
                )
                CompactStatTile(
                    title: "Tokens",
                    value: formatTokens(summary.today.tokens),
                    detail: "\(summary.today.messages) messages",
                    color: .green
                )
            }

            HStack(spacing: 8) {
                SignalChip(
                    title: "Top client",
                    value: summary.top.client ?? "none",
                    color: providerColor(summary.top.client ?? "")
                )
                SignalChip(
                    title: "Top model",
                    value: summary.top.model ?? "No model",
                    color: .orange
                )
            }
        }
    }
}

private struct HistoryPane: View {
    let model: TokscaleDashboardModel

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Last 7 days")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(.secondary)
                    Text(model.historyPeak.map { "Peak \($0.value) on \($0.date)" } ?? "No history yet")
                        .font(.system(size: 13, weight: .semibold))
                        .lineLimit(1)
                        .minimumScaleFactor(0.8)
                }
                Spacer()
            }

            HStack(alignment: .bottom, spacing: 7) {
                ForEach(model.historyTrend, id: \.date) { day in
                    VStack(spacing: 5) {
                        RoundedRectangle(cornerRadius: 4, style: .continuous)
                            .fill(Color.blue.opacity(day.progress > 0 ? 0.72 : 0.16))
                            .frame(height: max(8, 74 * day.progress))
                        Text(String(day.date.suffix(2)))
                            .font(.system(size: 9, weight: .semibold))
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, maxHeight: 92, alignment: .bottom)
                    .help("\(day.date) · \(day.value) · \(day.messages)")
                }
            }
        }
        .padding(11)
        .background(panelBackground(color: .blue, intensity: 0.06))
    }
}

private struct ClientsPane: View {
    let model: TokscaleDashboardModel

    var body: some View {
        VStack(spacing: 7) {
            ForEach(model.providers.prefix(4), id: \.id) { provider in
                ClientUsageRow(provider: provider)
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

private struct ClientUsageRow: View {
    let provider: TokscaleDashboardModel.ProviderSummary

    private var color: Color {
        providerColor(provider.id)
    }

    var body: some View {
        VStack(spacing: 5) {
            HStack(spacing: 8) {
                ProviderDot(color: color)
                Text(provider.label)
                    .font(.system(size: 12, weight: .semibold))
                Spacer()
                Text(provider.value)
                    .font(.system(size: 12, weight: .bold, design: .rounded))
                    .monospacedDigit()
                Text("\(Int((provider.share * 100).rounded()))%")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 34, alignment: .trailing)
            }
            ProgressBar(progress: provider.share, color: color)
        }
        .padding(9)
        .background(panelBackground(color: color, intensity: 0.045))
    }
}

private struct ActionDock: View {
    let isRefreshing: Bool
    let onReload: () -> Void
    let onRefreshScan: () -> Void
    let onOpenTokensCI: () -> Void
    let onRevealCache: () -> Void
    let onQuit: () -> Void

    var body: some View {
        HStack(spacing: 7) {
            DockButton(title: "Reload", systemName: "arrow.clockwise", tint: .blue, action: onReload)
            DockButton(
                title: isRefreshing ? "Scanning" : "Scan",
                systemName: isRefreshing ? "hourglass" : "bolt.horizontal",
                tint: .orange,
                disabled: isRefreshing,
                action: onRefreshScan
            )
            DockButton(title: "Web", systemName: "safari", tint: .green, action: onOpenTokensCI)
            DockButton(title: "Cache", systemName: "folder", tint: .purple, action: onRevealCache)
            Spacer(minLength: 4)
            DockButton(title: "Quit", systemName: "power", tint: .red, action: onQuit)
        }
        .padding(7)
        .frame(height: 50)
        .background(
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .fill(Color(nsColor: .controlBackgroundColor).opacity(0.88))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .stroke(Color.white.opacity(0.1), lineWidth: 1)
        )
    }
}

private struct DockButton: View {
    let title: String
    let systemName: String
    let tint: Color
    var disabled = false
    let action: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            VStack(spacing: 3) {
                Image(systemName: systemName)
                    .font(.system(size: 13, weight: .bold))
                Text(title)
                    .font(.system(size: 9, weight: .semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
            }
            .frame(width: 48, height: 35)
            .foregroundStyle(buttonForeground)
            .background(
                RoundedRectangle(cornerRadius: 11, style: .continuous)
                    .fill(buttonBackground)
            )
            .scaleEffect(isHovering && !disabled ? 1.05 : 1)
            .animation(.spring(response: 0.18, dampingFraction: 0.78), value: isHovering)
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .help(title)
        .onHover { hovering in
            isHovering = hovering
        }
    }

    private var buttonForeground: Color {
        if disabled {
            return .secondary.opacity(0.45)
        }
        return isHovering ? tint : .secondary
    }

    private var buttonBackground: Color {
        if disabled {
            return Color(nsColor: .separatorColor).opacity(0.08)
        }
        return isHovering ? tint.opacity(0.13) : Color(nsColor: .separatorColor).opacity(0.08)
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
            Color(nsColor: .windowBackgroundColor)

            LinearGradient(
                colors: [
                    accent.opacity(0.13),
                    Color(nsColor: .controlBackgroundColor).opacity(0.52),
                    Color(nsColor: .windowBackgroundColor).opacity(0.96)
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        }
    }
}

private func panelBackground(color: Color, intensity: Double) -> some ShapeStyle {
    LinearGradient(
        colors: [
            Color(nsColor: .controlBackgroundColor).opacity(0.9),
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
