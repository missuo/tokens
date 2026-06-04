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
        guard let firstProvider = summary.map(TokscaleDashboardModel.init)?.providers.first else {
            return .blue
        }
        return providerColor(firstProvider.id)
    }
}

private enum CompanionPanel: String, CaseIterable, Identifiable {
    case usage = "Usage"
    case mix = "Mix"
    case health = "Health"

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .usage:
            return "gauge.with.dots.needle.67percent"
        case .mix:
            return "chart.bar.xaxis"
        case .health:
            return "checkmark.shield"
        }
    }
}

private struct SummaryContent: View {
    let summary: TokscaleSummary
    let isRefreshing: Bool
    let refreshStatus: String?

    @Namespace private var panelNamespace
    @State private var selectedPanel = CompanionPanel.usage
    @State private var selectedProviderId: String?

    private var model: TokscaleDashboardModel {
        TokscaleDashboardModel(summary: summary)
    }

    private var selectedProvider: TokscaleDashboardModel.ProviderSummary? {
        model.providers.first { $0.id == selectedProviderId } ?? model.providers.first
    }

    private var selectedDetails: TokscaleDashboardModel.ProviderDetails {
        model.providerDetails(for: selectedProvider?.id)
    }

    var body: some View {
        VStack(spacing: 8) {
            CompanionHeader(
                summary: summary,
                model: model,
                isRefreshing: isRefreshing
            )

            UsageHeroCard(
                summary: summary,
                model: model,
                provider: selectedProvider,
                details: selectedDetails,
                isRefreshing: isRefreshing
            )

            ProviderStackBar(
                providers: model.providers,
                selectedProviderId: selectedProvider?.id,
                onSelect: { provider in
                    withAnimation(.spring(response: 0.24, dampingFraction: 0.86)) {
                        selectedProviderId = provider.id
                        selectedPanel = .mix
                    }
                }
            )

            PanelSwitcher(
                selectedPanel: $selectedPanel,
                namespace: panelNamespace
            )

            DynamicDetailPane(
                panel: selectedPanel,
                summary: summary,
                model: model,
                provider: selectedProvider,
                details: selectedDetails,
                refreshStatus: refreshStatus
            )
            .transition(.opacity.combined(with: .scale(scale: 0.985)))
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
        guard !model.providers.isEmpty else {
            selectedProviderId = nil
            return
        }
        if let selectedProviderId, model.providers.contains(where: { $0.id == selectedProviderId }) {
            return
        }
        selectedProviderId = model.providers.first?.id
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
                Text(model.hero.subtitle)
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
}

private struct UsageHeroCard: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let provider: TokscaleDashboardModel.ProviderSummary?
    let details: TokscaleDashboardModel.ProviderDetails
    let isRefreshing: Bool

    private var accent: Color {
        provider.map { providerColor($0.id) } ?? .blue
    }

    var body: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 7) {
                HStack(spacing: 6) {
                    ProviderDot(color: accent)
                    Text(provider?.label ?? "All AI")
                        .font(.system(size: 12, weight: .semibold))
                    Spacer(minLength: 0)
                }

                Text(model.hero.title)
                    .font(.system(size: 38, weight: .bold, design: .rounded))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)

                Text("\(details.today) - \(model.hero.progressLabel)")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.78)

                MiniMetricRow(
                    leftTitle: "Total",
                    leftValue: details.total,
                    rightTitle: "Messages",
                    rightValue: details.messages,
                    color: accent
                )
            }

            UsageArcGauge(
                progress: model.hero.progress,
                color: accent,
                centerTitle: "\(Int((details.share * 100).rounded()))%",
                centerSubtitle: "share",
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
                .fill(accent.opacity(0.18))
                .frame(width: 92, height: 92)
                .blur(radius: 26)
                .offset(x: 20, y: -30)
        }
        .clipped()
    }

    private var heroBorder: LinearGradient {
        LinearGradient(
            colors: [
                accent.opacity(0.6),
                .green.opacity(0.18),
                .purple.opacity(0.22)
            ],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
    }
}

private struct ProviderStackBar: View {
    let providers: [TokscaleDashboardModel.ProviderSummary]
    let selectedProviderId: String?
    let onSelect: (TokscaleDashboardModel.ProviderSummary) -> Void

    var visibleProviders: [TokscaleDashboardModel.ProviderSummary] {
        Array(providers.prefix(6))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack {
                Text("Provider mix")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.secondary)
                Spacer()
                Text(selectedProviderLabel)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            GeometryReader { proxy in
                let spacing: CGFloat = 3
                let minimumWidth: CGFloat = 10
                let totalSpacing = spacing * CGFloat(max(visibleProviders.count - 1, 0))
                let availableWidth = max(1, proxy.size.width - totalSpacing)
                let reservedWidth = minimumWidth * CGFloat(visibleProviders.count)
                let weightedWidth = max(1, availableWidth - reservedWidth)
                let visibleShare = max(0.0001, visibleProviders.reduce(0) { $0 + $1.share })

                HStack(spacing: spacing) {
                    ForEach(visibleProviders, id: \.id) { provider in
                        ProviderSegment(
                            provider: provider,
                            selected: provider.id == selectedProviderId,
                            width: minimumWidth + weightedWidth * (provider.share / visibleShare),
                            action: { onSelect(provider) }
                        )
                    }
                }
            }
            .frame(height: 18)
        }
        .padding(.horizontal, 2)
        .frame(height: 45)
    }

    private var selectedProviderLabel: String {
        guard let selected = visibleProviders.first(where: { $0.id == selectedProviderId }) else {
            return "\(providers.count) clients"
        }
        return "\(selected.label) - \(Int((selected.share * 100).rounded()))%"
    }
}

private struct ProviderSegment: View {
    let provider: TokscaleDashboardModel.ProviderSummary
    let selected: Bool
    let width: CGFloat
    let action: () -> Void

    @State private var isHovering = false
    @State private var visibleWidth: CGFloat = 0

    var body: some View {
        Button(action: action) {
            RoundedRectangle(cornerRadius: 7, style: .continuous)
                .fill(providerColor(provider.id))
                .frame(width: visibleWidth, height: selected ? 18 : 14)
                .overlay(
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .stroke(Color.white.opacity(selected ? 0.55 : 0), lineWidth: 1)
                )
                .shadow(
                    color: providerColor(provider.id).opacity(selected || isHovering ? 0.28 : 0),
                    radius: 7,
                    y: 2
                )
                .scaleEffect(y: selected || isHovering ? 1.12 : 1, anchor: .center)
        }
        .buttonStyle(.plain)
        .help("\(provider.label) \(provider.value)")
        .onHover { hovering in
            withAnimation(.spring(response: 0.18, dampingFraction: 0.78)) {
                isHovering = hovering
            }
        }
        .onAppear {
            withAnimation(.spring(response: 0.54, dampingFraction: 0.82)) {
                visibleWidth = width
            }
        }
        .onChange(of: width) { newValue in
            withAnimation(.spring(response: 0.3, dampingFraction: 0.86)) {
                visibleWidth = newValue
            }
        }
    }
}

private struct PanelSwitcher: View {
    @Binding var selectedPanel: CompanionPanel
    let namespace: Namespace.ID

    var body: some View {
        HStack(spacing: 5) {
            ForEach(CompanionPanel.allCases) { panel in
                Button {
                    withAnimation(.spring(response: 0.24, dampingFraction: 0.86)) {
                        selectedPanel = panel
                    }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: panel.icon)
                            .font(.system(size: 11, weight: .semibold))
                        Text(panel.rawValue)
                            .font(.system(size: 11, weight: .semibold))
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 7)
                    .foregroundStyle(selectedPanel == panel ? Color.primary : Color.secondary)
                    .background {
                        if selectedPanel == panel {
                            Capsule()
                                .fill(Color(nsColor: .windowBackgroundColor).opacity(0.92))
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
                .fill(Color(nsColor: .controlBackgroundColor).opacity(0.74))
        )
        .frame(height: 34)
    }
}

private struct DynamicDetailPane: View {
    let panel: CompanionPanel
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let provider: TokscaleDashboardModel.ProviderSummary?
    let details: TokscaleDashboardModel.ProviderDetails
    let refreshStatus: String?

    var body: some View {
        ZStack {
            switch panel {
            case .usage:
                UsageDetailPane(summary: summary, model: model)
            case .mix:
                ProviderDetailPane(provider: provider, details: details)
            case .health:
                HealthDetailPane(summary: summary, model: model, refreshStatus: refreshStatus)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 104, maxHeight: 112, alignment: .top)
        .animation(.spring(response: 0.28, dampingFraction: 0.86), value: panel)
    }
}

private struct UsageDetailPane: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 8) {
                CompactStatTile(
                    title: "Today",
                    value: model.metrics[0].value,
                    detail: model.metrics[0].detail,
                    color: .blue
                )
                CompactStatTile(
                    title: "Total",
                    value: model.metrics[1].value,
                    detail: model.metrics[1].detail,
                    color: .purple
                )
            }

            HStack(spacing: 8) {
                SignalChip(
                    title: "Top",
                    value: summary.top.client ?? summary.top.model ?? "none",
                    color: providerColor(summary.top.client ?? "")
                )
                SignalChip(
                    title: "Models",
                    value: "\(summary.totals.models)",
                    color: .green
                )
                SignalChip(
                    title: "Accuracy",
                    value: summary.accuracy.confidence.capitalized,
                    color: accuracyColor(summary.accuracy.confidence)
                )
            }
        }
    }
}

private struct ProviderDetailPane: View {
    let provider: TokscaleDashboardModel.ProviderSummary?
    let details: TokscaleDashboardModel.ProviderDetails

    private var color: Color {
        provider.map { providerColor($0.id) } ?? providerColor(details.id)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            HStack(spacing: 10) {
                ShareRing(share: details.share, color: color)
                    .frame(width: 46, height: 46)

                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        ProviderDot(color: color)
                        Text(details.title)
                            .font(.system(size: 14, weight: .semibold))
                    }
                    Text(details.model)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }

                Spacer()

                Text(provider?.value ?? details.total)
                    .font(.system(size: 20, weight: .bold, design: .rounded))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.72)
            }

            HStack(spacing: 8) {
                SignalChip(title: "Today", value: details.today, color: color)
                SignalChip(title: "Tokens", value: details.tokens, color: .green)
                SignalChip(title: "Messages", value: details.messages, color: .orange)
            }
        }
        .padding(11)
        .background(panelBackground(color: color))
    }
}

private struct HealthDetailPane: View {
    let summary: TokscaleSummary
    let model: TokscaleDashboardModel
    let refreshStatus: String?

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 8) {
                HealthSignal(
                    title: "Cache",
                    value: model.health.title,
                    detail: model.health.warning ?? model.health.detail,
                    color: summary.stale ? .orange : .green,
                    icon: "internaldrive"
                )
                HealthSignal(
                    title: "Submit",
                    value: summary.latestSubmit?.status.capitalized ?? "None",
                    detail: summary.latestSubmit?.finishedAt ?? "No recent submit",
                    color: .blue,
                    icon: "arrow.up.circle"
                )
            }

            HealthSignal(
                title: "Accuracy",
                value: summary.accuracy.confidence.capitalized,
                detail: refreshStatus ?? summary.accuracy.sourceKinds.first ?? "unknown",
                color: accuracyColor(summary.accuracy.confidence),
                icon: "scope"
            )
        }
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
                .stroke(Color.white.opacity(0.11), lineWidth: 1)
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
            .foregroundStyle(isHovering && !disabled ? Color.white : tint.opacity(disabled ? 0.45 : 1))
            .background(
                RoundedRectangle(cornerRadius: 11, style: .continuous)
                    .fill(isHovering && !disabled ? tint : tint.opacity(disabled ? 0.06 : 0.12))
            )
            .scaleEffect(isHovering && !disabled ? 1.07 : 1)
            .animation(.spring(response: 0.18, dampingFraction: 0.76), value: isHovering)
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .help(title)
        .onHover { hovering in
            isHovering = hovering
        }
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
                .stroke(
                    AngularGradient(
                        colors: [color, .green, .purple, color],
                        center: .center
                    ),
                    style: StrokeStyle(lineWidth: 9, lineCap: .round)
                )
                .rotationEffect(.degrees(90))
                .shadow(color: color.opacity(0.34), radius: active ? 11 : 6)

            Circle()
                .fill(color.opacity(active ? 0.16 : 0.08))
                .frame(width: pulse && active ? 82 : 64, height: pulse && active ? 82 : 64)
                .animation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true), value: pulse)

            VStack(spacing: 1) {
                Text(centerTitle)
                    .font(.system(size: 19, weight: .bold, design: .rounded))
                    .monospacedDigit()
                Text(centerSubtitle)
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
        }
        .onAppear {
            withAnimation(.spring(response: 0.65, dampingFraction: 0.82)) {
                visibleProgress = progress
            }
            pulse = true
        }
        .onChange(of: progress) { newValue in
            withAnimation(.spring(response: 0.42, dampingFraction: 0.86)) {
                visibleProgress = newValue
            }
        }
    }
}

private struct ShareRing: View {
    let share: Double
    let color: Color
    @State private var visibleShare = 0.0

    var body: some View {
        ZStack {
            Circle()
                .stroke(Color(nsColor: .separatorColor).opacity(0.28), lineWidth: 5)
            Circle()
                .trim(from: 0, to: visibleShare)
                .stroke(color, style: StrokeStyle(lineWidth: 5, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Text("\(Int((share * 100).rounded()))%")
                .font(.system(size: 10, weight: .bold))
                .monospacedDigit()
        }
        .onAppear {
            withAnimation(.spring(response: 0.55, dampingFraction: 0.82)) {
                visibleShare = share
            }
        }
        .onChange(of: share) { newValue in
            withAnimation(.spring(response: 0.35, dampingFraction: 0.86)) {
                visibleShare = newValue
            }
        }
    }
}

private struct MiniMetricRow: View {
    let leftTitle: String
    let leftValue: String
    let rightTitle: String
    let rightValue: String
    let color: Color

    var body: some View {
        HStack(spacing: 8) {
            MiniMetric(title: leftTitle, value: leftValue, color: color)
            MiniMetric(title: rightTitle, value: rightValue, color: .orange)
        }
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
        .background(panelBackground(color: color))
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
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(color.opacity(0.11))
        )
    }
}

private struct HealthSignal: View {
    let title: String
    let value: String
    let detail: String
    let color: Color
    let icon: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
                .font(.system(size: 13, weight: .bold))
                .foregroundStyle(color)
                .frame(width: 25, height: 25)
                .background(Circle().fill(color.opacity(0.13)))

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Text(title)
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.secondary)
                    Text(value)
                        .font(.system(size: 11, weight: .semibold))
                }
                Text(detail)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.72)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(8)
        .background(panelBackground(color: color))
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
            .shadow(color: color.opacity(0.38), radius: 4)
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
            .background(panelBackground(color: .orange))

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
                    accent.opacity(0.16),
                    .green.opacity(0.08),
                    .purple.opacity(0.1),
                    Color(nsColor: .windowBackgroundColor).opacity(0.92)
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )

            Circle()
                .fill(.orange.opacity(0.08))
                .frame(width: 180, height: 180)
                .blur(radius: 34)
                .offset(x: -176, y: 130)

            Circle()
                .fill(.blue.opacity(0.07))
                .frame(width: 220, height: 220)
                .blur(radius: 40)
                .offset(x: 168, y: -160)
        }
    }
}

private func panelBackground(color: Color) -> some ShapeStyle {
    LinearGradient(
        colors: [
            Color(nsColor: .controlBackgroundColor).opacity(0.9),
            color.opacity(0.1)
        ],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
}

private func providerColor(_ provider: TokscaleDashboardModel.ProviderSummary) -> Color {
    providerColor(provider.id)
}

private func providerColor(_ id: String) -> Color {
    switch id.lowercased() {
    case "claude":
        return Color(red: 0.93, green: 0.45, blue: 0.18)
    case "codex":
        return Color(red: 0.19, green: 0.43, blue: 0.95)
    case "gemini":
        return Color(red: 0.16, green: 0.68, blue: 0.42)
    case "openclaw":
        return Color(red: 0.54, green: 0.32, blue: 0.92)
    case "copilot":
        return Color(red: 0.05, green: 0.58, blue: 0.62)
    case "antigravity":
        return Color(red: 0.85, green: 0.28, blue: 0.54)
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
