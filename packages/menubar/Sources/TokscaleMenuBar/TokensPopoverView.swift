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

    @Namespace private var tabNamespace
    @State private var selectedTab = DashboardTab.overview
    @State private var selectedProviderId: String?

    var body: some View {
        VStack(spacing: 12) {
            if let summary {
                SummaryContent(
                    summary: summary,
                    selectedTab: $selectedTab,
                    selectedProviderId: $selectedProviderId,
                    tabNamespace: tabNamespace,
                    isRefreshing: isRefreshing,
                    refreshStatus: refreshStatus
                )
            } else {
                EmptyContent(errorMessage: errorMessage)
            }

            ActionBar(
                isRefreshing: isRefreshing,
                onReload: onReload,
                onRefreshScan: onRefreshScan,
                onOpenTokensCI: onOpenTokensCI,
                onRevealCache: onRevealCache,
                onQuit: onQuit
            )
        }
        .padding(14)
        .frame(width: 420, height: 460, alignment: .top)
        .background(
            ZStack {
                Color(nsColor: .windowBackgroundColor)
                Color.accentColor.opacity(0.035)
            }
        )
    }
}

private enum DashboardTab: String, CaseIterable, Identifiable {
    case overview = "Overview"
    case providers = "Providers"
    case health = "Health"

    var id: String { rawValue }
    var icon: String {
        switch self {
        case .overview: return "chart.line.uptrend.xyaxis"
        case .providers: return "square.grid.2x2"
        case .health: return "checkmark.shield"
        }
    }
}

private struct SummaryContent: View {
    let summary: TokscaleSummary
    @Binding var selectedTab: DashboardTab
    @Binding var selectedProviderId: String?
    let tabNamespace: Namespace.ID
    let isRefreshing: Bool
    let refreshStatus: String?

    private var model: TokscaleDashboardModel {
        TokscaleDashboardModel(summary: summary)
    }

    var body: some View {
        VStack(spacing: 12) {
            HeroBand(model: model, summary: summary, isRefreshing: isRefreshing)
            TabSwitcher(selectedTab: $selectedTab, namespace: tabNamespace)

            ZStack {
                switch selectedTab {
                case .overview:
                    OverviewPane(model: model, selectedProviderId: $selectedProviderId)
                        .transition(.move(edge: .leading).combined(with: .opacity))
                case .providers:
                    ProvidersPane(model: model, selectedProviderId: $selectedProviderId)
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                case .health:
                    HealthPane(summary: summary, model: model, refreshStatus: refreshStatus)
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .animation(.spring(response: 0.28, dampingFraction: 0.86), value: selectedTab)
        }
        .onAppear {
            if selectedProviderId == nil {
                selectedProviderId = model.providers.first?.id
            }
        }
    }
}

private struct HeroBand: View {
    let model: TokscaleDashboardModel
    let summary: TokscaleSummary
    let isRefreshing: Bool

    private var leadingProvider: TokscaleDashboardModel.ProviderSummary? {
        model.providers.first
    }

    var body: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    LiveDot(stale: summary.stale, active: isRefreshing)
                    Text(isRefreshing ? "Scanning" : model.health.title)
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(isRefreshing ? .orange : (summary.stale ? .orange : .green))
                    Spacer()
                    Text(model.hero.subtitle)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.secondary)
                }

                HStack(alignment: .lastTextBaseline, spacing: 10) {
                    Text(model.hero.title)
                        .font(.system(size: 36, weight: .semibold, design: .rounded))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.75)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("today")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(.secondary)
                        Text(model.hero.progressLabel)
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }

                AnimatedUsageBar(
                    progress: model.hero.progress,
                    color: leadingProvider.map(providerColor) ?? .accentColor
                )
            }
        }
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color(nsColor: .controlBackgroundColor))
        )
        .overlay(alignment: .leading) {
            RoundedRectangle(cornerRadius: 10)
                .fill((leadingProvider.map(providerColor) ?? .accentColor).opacity(0.9))
                .frame(width: 4)
        }
    }
}

private struct TabSwitcher: View {
    @Binding var selectedTab: DashboardTab
    let namespace: Namespace.ID

    var body: some View {
        HStack(spacing: 6) {
            ForEach(DashboardTab.allCases) { tab in
                Button {
                    withAnimation(.spring(response: 0.24, dampingFraction: 0.86)) {
                        selectedTab = tab
                    }
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: tab.icon)
                            .font(.system(size: 12, weight: .semibold))
                        Text(tab.rawValue)
                            .font(.system(size: 12, weight: .semibold))
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    .foregroundStyle(selectedTab == tab ? Color.primary : Color.secondary)
                    .background {
                        if selectedTab == tab {
                            RoundedRectangle(cornerRadius: 8)
                                .fill(Color(nsColor: .windowBackgroundColor))
                                .matchedGeometryEffect(id: "active-tab", in: namespace)
                                .shadow(color: .black.opacity(0.08), radius: 6, y: 2)
                        }
                    }
                }
                .buttonStyle(.plain)
            }
        }
        .padding(4)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color(nsColor: .controlBackgroundColor))
        )
    }
}

private struct OverviewPane: View {
    let model: TokscaleDashboardModel
    @Binding var selectedProviderId: String?

    private var selectedDetails: TokscaleDashboardModel.ProviderDetails {
        model.providerDetails(for: selectedProviderId)
    }

    var body: some View {
        VStack(spacing: 12) {
            HStack(spacing: 10) {
                ForEach(model.metrics, id: \.title) { metric in
                    MetricTile(panel: metric)
                }
            }

            VStack(alignment: .leading, spacing: 9) {
                SectionHeader(title: "Provider Mix", value: selectedDetails.title)
                ForEach(model.providers.prefix(4), id: \.id) { provider in
                    ProviderMixRow(
                        provider: provider,
                        selected: selectedProviderId == provider.id
                    ) {
                        withAnimation(.spring(response: 0.24, dampingFraction: 0.86)) {
                            selectedProviderId = provider.id
                        }
                    }
                }
            }

            ProviderFocusStrip(details: selectedDetails)
        }
    }
}

private struct ProvidersPane: View {
    let model: TokscaleDashboardModel
    @Binding var selectedProviderId: String?

    private var details: TokscaleDashboardModel.ProviderDetails {
        model.providerDetails(for: selectedProviderId)
    }

    var body: some View {
        VStack(spacing: 12) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(model.providers, id: \.id) { provider in
                        ProviderChip(
                            provider: provider,
                            selected: selectedProviderId == provider.id
                        ) {
                            withAnimation(.spring(response: 0.24, dampingFraction: 0.86)) {
                                selectedProviderId = provider.id
                            }
                        }
                    }
                }
                .padding(.horizontal, 1)
            }

            ProviderDetailPanel(details: details)
        }
    }
}

private struct HealthPane: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let refreshStatus: String?

    var body: some View {
        VStack(spacing: 10) {
            HealthStatusCard(
                title: "Accuracy",
                value: summary.accuracy.confidence.capitalized,
                detail: summary.accuracy.sourceKinds.first ?? "unknown",
                color: accuracyColor(summary.accuracy.confidence),
                icon: "scope"
            )
            HealthStatusCard(
                title: "Local Cache",
                value: model.health.title,
                detail: model.health.warning ?? model.health.detail,
                color: summary.stale ? .orange : .green,
                icon: "internaldrive"
            )
            HealthStatusCard(
                title: "Submit",
                value: summary.latestSubmit?.status.capitalized ?? "None",
                detail: summary.latestSubmit?.finishedAt ?? "No recent submit",
                color: .blue,
                icon: "arrow.up.circle"
            )

            if let refreshStatus {
                HStack(spacing: 8) {
                    Image(systemName: "info.circle")
                    Text(refreshStatus)
                        .lineLimit(2)
                    Spacer()
                }
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
                .padding(.top, 2)
            }
        }
    }
}

private struct ActionBar: View {
    let isRefreshing: Bool
    let onReload: () -> Void
    let onRefreshScan: () -> Void
    let onOpenTokensCI: () -> Void
    let onRevealCache: () -> Void
    let onQuit: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            ActionButton(title: "Reload", systemName: "arrow.clockwise", tint: .blue, action: onReload)
            ActionButton(
                title: isRefreshing ? "Scanning" : "Scan",
                systemName: isRefreshing ? "hourglass" : "bolt.horizontal",
                tint: .orange,
                action: onRefreshScan
            )
            .disabled(isRefreshing)
            ActionButton(title: "Web", systemName: "safari", tint: .green, action: onOpenTokensCI)
            ActionButton(title: "Cache", systemName: "folder", tint: .purple, action: onRevealCache)
            Spacer(minLength: 4)
            ActionButton(title: "Quit", systemName: "power", tint: .red, action: onQuit)
        }
        .padding(8)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color(nsColor: .controlBackgroundColor))
        )
    }
}

private struct ActionButton: View {
    let title: String
    let systemName: String
    let tint: Color
    let action: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: systemName)
                    .font(.system(size: 12, weight: .bold))
                Text(title)
                    .font(.system(size: 11, weight: .semibold))
            }
            .padding(.horizontal, 9)
            .padding(.vertical, 7)
            .foregroundStyle(isHovering ? Color.white : tint)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(isHovering ? tint : tint.opacity(0.12))
            )
            .scaleEffect(isHovering ? 1.04 : 1)
            .animation(.spring(response: 0.18, dampingFraction: 0.78), value: isHovering)
        }
        .buttonStyle(.plain)
        .help(title)
        .onHover { hovering in
            isHovering = hovering
        }
    }
}

private struct MetricTile: View {
    let panel: TokscaleDashboardModel.Panel

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(panel.title)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(panel.value)
                .font(.system(size: 20, weight: .semibold, design: .rounded))
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.78)
            Text(panel.detail)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color(nsColor: .controlBackgroundColor))
        )
    }
}

private struct ProviderMixRow: View {
    let provider: TokscaleDashboardModel.ProviderSummary
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 5) {
                HStack(spacing: 8) {
                    ProviderBadge(label: provider.label, color: providerColor(provider))
                    Spacer()
                    Text(provider.value)
                        .font(.system(size: 12, weight: .semibold))
                        .monospacedDigit()
                    Text(provider.detail)
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(.secondary)
                        .frame(width: 92, alignment: .trailing)
                }
                AnimatedProviderBar(
                    share: provider.share,
                    color: providerColor(provider),
                    selected: selected
                )
            }
            .padding(8)
            .background(
                RoundedRectangle(cornerRadius: 9)
                    .fill(selected ? providerColor(provider).opacity(0.12) : Color(nsColor: .controlBackgroundColor).opacity(0.55))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 9)
                    .stroke(selected ? providerColor(provider).opacity(0.45) : Color.clear, lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }
}

private struct ProviderChip: View {
    let provider: TokscaleDashboardModel.ProviderSummary
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 6) {
                    Circle()
                        .fill(providerColor(provider))
                        .frame(width: 7, height: 7)
                    Text(provider.label)
                        .font(.system(size: 12, weight: .semibold))
                }
                Text(provider.value)
                    .font(.system(size: 15, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                Text("\(Int((provider.share * 100).rounded()))% share")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            .padding(10)
            .frame(width: 116, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(selected ? providerColor(provider).opacity(0.16) : Color(nsColor: .controlBackgroundColor))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(selected ? providerColor(provider).opacity(0.55) : Color.clear, lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }
}

private struct ProviderDetailPanel: View {
    let details: TokscaleDashboardModel.ProviderDetails

    var body: some View {
        let color = providerColor(details.id)
        VStack(alignment: .leading, spacing: 13) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 4) {
                    ProviderBadge(label: details.title, color: color)
                    Text(details.model)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer()
                ShareRing(share: details.share, color: color)
            }

            HStack(spacing: 10) {
                DetailMetric(title: "Today", value: details.today, color: color)
                DetailMetric(title: "Total", value: details.total, color: .blue)
            }
            HStack(spacing: 10) {
                DetailMetric(title: "Tokens", value: details.tokens, color: .green)
                DetailMetric(title: "Messages", value: details.messages, color: .orange)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, minHeight: 205, alignment: .topLeading)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(nsColor: .controlBackgroundColor))
        )
        .overlay(alignment: .top) {
            RoundedRectangle(cornerRadius: 12)
                .fill(color.opacity(0.9))
                .frame(height: 3)
        }
    }
}

private struct ProviderFocusStrip: View {
    let details: TokscaleDashboardModel.ProviderDetails

    var body: some View {
        let color = providerColor(details.id)
        HStack(spacing: 10) {
            ShareRing(share: details.share, color: color)
                .frame(width: 44, height: 44)
            VStack(alignment: .leading, spacing: 3) {
                Text(details.title)
                    .font(.system(size: 13, weight: .semibold))
                Text(details.model)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            Text(details.today)
                .font(.system(size: 13, weight: .semibold, design: .rounded))
                .monospacedDigit()
        }
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(color.opacity(0.12))
        )
    }
}

private struct HealthStatusCard: View {
    let title: String
    let value: String
    let detail: String
    let color: Color
    let icon: String

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: icon)
                .font(.system(size: 15, weight: .bold))
                .foregroundStyle(color)
                .frame(width: 28, height: 28)
                .background(Circle().fill(color.opacity(0.13)))
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.secondary)
                Text(detail)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            Text(value)
                .font(.system(size: 13, weight: .semibold))
                .lineLimit(1)
        }
        .padding(11)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color(nsColor: .controlBackgroundColor))
        )
    }
}

private struct AnimatedUsageBar: View {
    let progress: Double
    let color: Color
    @State private var visibleProgress = 0.0

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color(nsColor: .separatorColor).opacity(0.35))
                Capsule()
                    .fill(color)
                    .frame(width: max(8, proxy.size.width * visibleProgress))
                    .shadow(color: color.opacity(0.35), radius: 6, y: 1)
            }
        }
        .frame(height: 8)
        .onAppear {
            withAnimation(.spring(response: 0.7, dampingFraction: 0.82)) {
                visibleProgress = progress
            }
        }
        .onChange(of: progress) { newValue in
            withAnimation(.spring(response: 0.45, dampingFraction: 0.85)) {
                visibleProgress = newValue
            }
        }
    }
}

private struct AnimatedProviderBar: View {
    let share: Double
    let color: Color
    let selected: Bool
    @State private var visibleShare = 0.0

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color(nsColor: .separatorColor).opacity(0.28))
                Capsule()
                    .fill(color)
                    .frame(width: max(6, proxy.size.width * visibleShare))
                    .shadow(color: selected ? color.opacity(0.32) : .clear, radius: 5)
            }
        }
        .frame(height: selected ? 7 : 5)
        .animation(.spring(response: 0.22, dampingFraction: 0.75), value: selected)
        .onAppear {
            withAnimation(.spring(response: 0.55, dampingFraction: 0.82)) {
                visibleShare = share
            }
        }
        .onChange(of: share) { newValue in
            withAnimation(.spring(response: 0.45, dampingFraction: 0.85)) {
                visibleShare = newValue
            }
        }
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
                .frame(width: pulse ? 18 : 8, height: pulse ? 18 : 8)
                .opacity(pulse ? 0 : 1)
            Circle()
                .fill(dotColor)
                .frame(width: 8, height: 8)
        }
        .frame(width: 18, height: 18)
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

private struct ShareRing: View {
    let share: Double
    let color: Color
    @State private var visibleShare = 0.0

    var body: some View {
        ZStack {
            Circle()
                .stroke(Color(nsColor: .separatorColor).opacity(0.35), lineWidth: 5)
            Circle()
                .trim(from: 0, to: visibleShare)
                .stroke(color, style: StrokeStyle(lineWidth: 5, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Text("\(Int((share * 100).rounded()))%")
                .font(.system(size: 10, weight: .bold))
                .monospacedDigit()
        }
        .frame(width: 52, height: 52)
        .onAppear {
            withAnimation(.spring(response: 0.6, dampingFraction: 0.82)) {
                visibleShare = share
            }
        }
        .onChange(of: share) { newValue in
            withAnimation(.spring(response: 0.45, dampingFraction: 0.85)) {
                visibleShare = newValue
            }
        }
    }
}

private struct ProviderBadge: View {
    let label: String
    let color: Color

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(color)
                .frame(width: 8, height: 8)
            Text(label)
                .font(.system(size: 12, weight: .semibold))
                .lineLimit(1)
        }
    }
}

private struct DetailMetric: View {
    let title: String
    let value: String
    let color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title)
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(size: 13, weight: .semibold, design: .rounded))
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.72)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 9)
                .fill(color.opacity(0.11))
        )
    }
}

private struct SectionHeader: View {
    let title: String
    let value: String

    var body: some View {
        HStack {
            Text(title)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
        }
    }
}

private struct EmptyContent: View {
    let errorMessage: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Tokens")
                .font(.system(size: 28, weight: .semibold, design: .rounded))
            Text(errorMessage ?? "No local summary yet.")
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
            Text("Run tokens companion-summary --refresh once, then reload this view.")
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

private func providerColor(_ provider: TokscaleDashboardModel.ProviderSummary) -> Color {
    providerColor(provider.id)
}

private func providerColor(_ id: String) -> Color {
    switch id.lowercased() {
    case "claude":
        return .orange
    case "codex":
        return .blue
    case "gemini":
        return .green
    case "openclaw":
        return .purple
    default:
        return .accentColor
    }
}

private func accuracyColor(_ value: String) -> Color {
    switch value.lowercased() {
    case "high":
        return .green
    case "medium":
        return .orange
    default:
        return .red
    }
}
