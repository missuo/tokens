import AppKit
import SwiftUI

public struct MenuPanelView: View {
    @ObservedObject public var store: UsageStore
    @ObservedObject public var settings: AppSettings
    /// Called whenever the panel’s ideal size changes so `NSPopover.contentSize` can shrink-wrap.
    public var onIdealSizeChange: ((CGSize) -> Void)?

    /// Intrinsic height of the scrollable body (sections only).
    @State private var bodyContentHeight: CGFloat = 0
    /// Intrinsic height of header + tabs + footer (+ error shells).
    @State private var chromeHeight: CGFloat = 0
    /// Body viewport height (snaps; no spring).
    @State private var bodyViewportHeight: CGFloat = MenuBarLayout.fallbackContentHeight
    /// CLIENT list: how many rows are visible (chevron loads more).
    @State private var clientVisibleCount: Int = MenuBarLayout.clientPageSize

    public init(
        store: UsageStore,
        settings: AppSettings,
        onIdealSizeChange: ((CGSize) -> Void)? = nil
    ) {
        self.store = store
        self.settings = settings
        self.onIdealSizeChange = onIdealSizeChange
    }

    private var panelMaxHeight: CGFloat { MenuBarLayout.panelMaxHeight() }

    private var maxBodyHeight: CGFloat {
        let chrome = chromeHeight > 0 ? chromeHeight : 140
        return max(160, panelMaxHeight - chrome)
    }

    /// Target body height for the current measurement (nil when no report body).
    private var targetBodyHeight: CGFloat? {
        guard store.report != nil, !store.binaryMissing else { return nil }
        if bodyContentHeight <= 0 {
            return MenuBarLayout.fallbackContentHeight
        }
        return min(bodyContentHeight, maxBodyHeight)
    }

    /// True when selected period and loaded report agree (safe to resize).
    private var reportMatchesPeriod: Bool {
        guard let report = store.report else { return false }
        return report.period == store.period.cliValue
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            measuredChrome {
                header
                    .padding(.horizontal, MenuBarLayout.horizontalPadding)
                    .padding(.top, 16)
                    .padding(.bottom, 4)
            }

            if store.binaryMissing {
                measuredChrome {
                    missingCLI
                        .padding(.horizontal, MenuBarLayout.horizontalPadding)
                        .padding(.vertical, 18)
                }
            } else if let error = store.lastError, store.report == nil {
                measuredChrome {
                    errorBanner(error)
                        .padding(.horizontal, MenuBarLayout.horizontalPadding)
                        .padding(.vertical, 18)
                }
            } else if let report = store.report {
                measuredChrome {
                    periodTabs
                        .padding(.horizontal, MenuBarLayout.horizontalPadding)
                        .padding(.top, 14)
                        .padding(.bottom, 6)
                }

                // Always ScrollView (even when short) so TODAY vs 30D does not
                // flip layout structure and remeasure thrash the popover height.
                reportBody(report)
                    .frame(height: bodyViewportHeight, alignment: .top)
                    .clipped()

                measuredChrome {
                    footer(report)
                        .padding(.horizontal, MenuBarLayout.horizontalPadding)
                        .padding(.top, 4)
                        .padding(.bottom, 14)
                }
            } else {
                measuredChrome {
                    VStack(spacing: 12) {
                        ProgressView()
                            .controlSize(.small)
                        Text(store.isLoading ? "SCANNING LOCAL USAGE…" : "NO DATA YET")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .tracking(0.8)
                    }
                    .frame(maxWidth: .infinity, minHeight: 160)
                    .padding(MenuBarLayout.horizontalPadding)
                }
            }
        }
        .frame(width: MenuBarLayout.panelWidth)
        .frame(maxHeight: panelMaxHeight)
        .onPreferenceChange(ChromeHeightPreferenceKey.self) { height in
            if abs(height - chromeHeight) > 0.5 {
                chromeHeight = height
            }
            syncBodyHeightAndPublish()
        }
        .onPreferenceChange(BodyHeightPreferenceKey.self) { height in
            if height > 0, abs(height - bodyContentHeight) > 0.5 {
                bodyContentHeight = height
            }
            syncBodyHeightAndPublish()
        }
        .onAppear { syncBodyHeightAndPublish() }
        .onChange(of: store.report?.generatedAt) { _ in syncBodyHeightAndPublish() }
        .onChange(of: store.period) { _ in
            // Reset CLIENT expand page; keep prior panel height until new report lands.
            clientVisibleCount = MenuBarLayout.clientPageSize
        }
        .onChange(of: store.isLoading) { _ in syncBodyHeightAndPublish() }
        .onChange(of: store.binaryMissing) { _ in syncBodyHeightAndPublish() }
        .onChange(of: store.lastError) { _ in syncBodyHeightAndPublish() }
        .onChange(of: clientVisibleCount) { _ in
            // Expanding clients grows content; remeasure after next layout pass.
            DispatchQueue.main.async { syncBodyHeightAndPublish() }
        }
    }

    @ViewBuilder
    private func reportBody(_ report: UsageReport) -> some View {
        ScrollView(.vertical, showsIndicators: bodyContentHeight > maxBodyHeight + 1) {
            bodySections(report)
                .padding(.horizontal, MenuBarLayout.horizontalPadding)
                .padding(.top, 18)
                .padding(.bottom, 12)
                .fixedSize(horizontal: false, vertical: true)
                .background(
                    GeometryReader { geo in
                        Color.clear.preference(
                            key: BodyHeightPreferenceKey.self,
                            value: geo.size.height
                        )
                    }
                )
        }
    }

    @ViewBuilder
    private func bodySections(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: MenuBarLayout.sectionSpacing) {
            totalSection(report)
            breakdownSection(report)
            clientSection(report)
            modelSection(report)
            costChartSection(report)
            if let error = store.lastError {
                errorBanner(error)
            }
        }
    }

    @ViewBuilder
    private func measuredChrome<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        content()
            .background(
                GeometryReader { geo in
                    Color.clear.preference(
                        key: ChromeHeightPreferenceKey.self,
                        value: geo.size.height
                    )
                }
            )
    }

    /// Snap body height + notify AppKit popover (coalesced there). No height spring.
    /// Skip resize while the tab is ahead of the loaded report (TODAY short vs 30D tall).
    private func syncBodyHeightAndPublish() {
        // Hold previous height until report matches selected period — prevents
        // intermediate collapse/expand when switching into TODAY.
        if store.report != nil, !reportMatchesPeriod {
            return
        }

        if let target = targetBodyHeight, abs(target - bodyViewportHeight) > 0.5 {
            bodyViewportHeight = target
        }

        let chrome = chromeHeight > 0 ? chromeHeight : 140
        let body = targetBodyHeight ?? (store.report == nil ? 0 : bodyViewportHeight)
        let ideal = min(chrome + body, panelMaxHeight)
        let height = max(ideal, 120)
        onIdealSizeChange?(CGSize(width: MenuBarLayout.panelWidth, height: height))
    }

    // MARK: - Header

    private var header: some View {
        HStack(alignment: .firstTextBaseline) {
            Text("TOKENS")
                .font(.system(size: 11, weight: .medium, design: .monospaced))
                .tracking(1.6)
            Spacer()
            if store.isLoading {
                ProgressView()
                    .controlSize(.small)
            } else {
                Text("usage · local")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - Period tabs

    private var periodTabs: some View {
        HStack(spacing: 0) {
            ForEach(UsagePeriod.allCases) { period in
                // Use onTapGesture (not Button) so the first click in an NSPopover
                // is not eaten by button activation / animation transaction.
                Text(period.monoTitle)
                    .font(.system(size: 11, design: .monospaced))
                    .tracking(0.4)
                    .foregroundStyle(store.period == period ? Color.primary : Color.secondary)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    .contentShape(Rectangle())
                    .overlay(alignment: .bottom) {
                        Rectangle()
                            .fill(store.period == period ? Color.primary : Color.clear)
                            .frame(height: 2)
                    }
                    .onTapGesture {
                        store.setPeriod(period)
                    }
                    .accessibilityAddTraits(.isButton)
                    .accessibilityAddTraits(store.period == period ? .isSelected : [])
                    .accessibilityLabel(period.monoTitle)
            }
        }
    }

    // MARK: - TOTAL

    private func totalSection(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            sectionLabel("TOTAL")
            Text(Formatting.compactTokens(report.summary.totalTokens))
                .font(.system(size: 36, weight: .medium, design: .monospaced))
                .tracking(-1.4)
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.6)
                .padding(.top, 4)

            HStack(alignment: .top, spacing: 16) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("COST")
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .tracking(0.8)
                    Text(Formatting.cost(report.summary.totalCost))
                        .font(.system(size: 18, design: .monospaced))
                        .monospacedDigit()
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                VStack(alignment: .leading, spacing: 4) {
                    Text("MESSAGES")
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .tracking(0.8)
                    Text("\(report.summary.messages)")
                        .font(.system(size: 18, design: .monospaced))
                        .monospacedDigit()
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.top, 16)

            Text("\(report.dateRange.start) — \(report.dateRange.end)")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
                .padding(.top, 12)
        }
    }

    // MARK: - BREAKDOWN

    private func breakdownSection(_ report: UsageReport) -> some View {
        let b = report.tokenBreakdown
        let items: [(String, Int64)] = [
            ("in", b.input),
            ("out", b.output),
            ("cache", b.cacheRead),
            ("reason", b.reasoning),
        ]
        let accents: [Double] = [1, 0.72, 0.48, 0.28]

        return VStack(alignment: .leading, spacing: 12) {
            sectionLabel("BREAKDOWN")
            HStack(spacing: 6) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    breakdownCard(
                        label: item.0,
                        value: item.1,
                        topAccent: Color.primary.opacity(accents[index])
                    )
                }
            }
        }
    }

    private func breakdownCard(label: String, value: Int64, topAccent: Color) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
            Text(Formatting.compactTokens(value))
                .font(.system(size: 12, weight: .bold, design: .monospaced))
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 8)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(Color.primary.opacity(0.04))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(Color.primary.opacity(0.12), lineWidth: 1)
        )
        .overlay(alignment: .top) {
            Rectangle()
                .fill(topAccent)
                .frame(height: 2)
                .clipShape(
                    UnevenRoundedRectangle(
                        topLeadingRadius: 8,
                        bottomLeadingRadius: 0,
                        bottomTrailingRadius: 0,
                        topTrailingRadius: 8
                    )
                )
        }
    }

    // MARK: - CLIENT

    private func clientSection(_ report: UsageReport) -> some View {
        let all = report.byClient
        let visible = Array(all.prefix(max(clientVisibleCount, 0)))
        let hasMore = all.count > visible.count

        return VStack(alignment: .leading, spacing: 12) {
            sectionLabel("CLIENT")
            if all.isEmpty {
                emptyHint("No client data")
            } else {
                clientRows(visible)
                if hasMore {
                    clientExpandChevron(remaining: all.count - visible.count)
                }
            }
        }
    }

    private func clientRows(_ clients: [ClientUsage]) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(clients) { client in
                clientRow(client)
            }
        }
    }

    /// Centered down-chevron: load another page of clients (no nested scrollbar).
    private func clientExpandChevron(remaining: Int) -> some View {
        Button {
            clientVisibleCount = min(
                clientVisibleCount + MenuBarLayout.clientPageSize,
                clientVisibleCount + remaining
            )
        } label: {
            Image(systemName: "chevron.down")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 6)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Show more clients")
        .accessibilityHint("\(remaining) more")
    }

    private func clientRow(_ client: ClientUsage) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(client.client)
                    .font(.system(size: 12, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
                Text("\(Formatting.compactTokens(client.tokens)) · \(Formatting.percent(client.share))")
                    .font(.system(size: 12, design: .monospaced))
                    .monospacedDigit()
                    .foregroundStyle(.primary)
                    .layoutPriority(1)
            }
            shareBar(share: client.share)
        }
    }

    // MARK: - MODEL

    private func modelSection(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            sectionLabel("MODEL")
            if report.byModel.isEmpty {
                emptyHint("No model data")
            } else if report.byModel.count > MenuBarLayout.nestedListThreshold {
                nestedListScroll {
                    modelRows(report.byModel)
                }
            } else {
                modelRows(report.byModel)
            }
        }
    }

    private func modelRows(_ models: [ModelUsage]) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(models) { model in
                modelRow(model)
            }
        }
    }

    private func modelRow(_ model: ModelUsage) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            HStack(spacing: 4) {
                Text(model.modelId)
                    .font(.system(size: 12, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text("/")
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(.secondary)
                Text(model.providerId)
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .layoutPriority(1)
            }
            .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)

            Text(Formatting.compactTokens(model.tokens))
                .font(.system(size: 12, design: .monospaced))
                .monospacedDigit()
                .layoutPriority(2)
        }
    }

    // MARK: - COST chart

    private func costChartSection(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline) {
                sectionLabel("COST · 14 DAYS")
                Spacer()
                Text("Y = $ · X = date")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            CostChartView(days: report.byDay, periodRawValue: store.period.rawValue)
        }
    }

    // MARK: - Footer

    private func footer(_ report: UsageReport) -> some View {
        HStack(spacing: 10) {
            Text(
                "UPDATED \(Formatting.relativeTime(fromISO8601: report.generatedAt).uppercased()) · \(report.scan.mode.uppercased())"
            )
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .minimumScaleFactor(0.65)
            .truncationMode(.tail)
            .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)

            footerButton("REFRESH", disabled: store.isLoading) {
                store.manualRefresh()
            }
            footerButton("SETTINGS") {
                store.showSettings = true
            }
            footerButton("QUIT") {
                store.quit()
            }
        }
        .font(.system(size: 11, design: .monospaced))
        .tracking(0.4)
    }

    private func footerButton(
        _ title: String,
        disabled: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Text(title)
                .foregroundStyle(disabled ? Color.secondary.opacity(0.5) : Color.primary)
        }
        .buttonStyle(.plain)
        .disabled(disabled)
    }

    // MARK: - Shared

    /// Nested CLIENT/MODEL list with top/bottom edge fades (IX-B).
    private func nestedListScroll<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        let fade = Color(nsColor: .windowBackgroundColor)
        return ScrollView {
            content()
        }
        .frame(maxHeight: MenuBarLayout.nestedListMaxHeight)
        .overlay(alignment: .top) {
            LinearGradient(
                colors: [fade, fade.opacity(0)],
                startPoint: .top,
                endPoint: .bottom
            )
            .frame(height: 16)
            .allowsHitTesting(false)
        }
        .overlay(alignment: .bottom) {
            LinearGradient(
                colors: [fade.opacity(0), fade],
                startPoint: .top,
                endPoint: .bottom
            )
            .frame(height: 16)
            .allowsHitTesting(false)
        }
    }

    private func sectionLabel(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(.secondary)
            .tracking(1.0)
            .textCase(.uppercase)
    }

    private func shareBar(share: Double) -> some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Rectangle().fill(Color.secondary.opacity(0.15))
                Rectangle()
                    .fill(Color.primary)
                    .frame(width: max(2, geo.size.width * CGFloat(min(max(share, 0), 1))))
            }
        }
        .frame(height: MenuBarLayout.shareBarHeight)
    }

    private func emptyHint(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(.secondary)
    }

    private var missingCLI: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("TOKENS CLI NOT FOUND")
                .font(.system(size: 12, weight: .semibold, design: .monospaced))
                .tracking(0.6)
            Text("Install or build the Menu Bar-capable CLI, then Recheck.\n\nbrew install owo-network/brew/tokens\n# or build this repo and link ~/.local/bin/tokens")
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            HStack(spacing: 14) {
                footerButton("RECHECK") {
                    store.resolveBinary()
                    store.manualRefresh()
                }
                footerButton("SETTINGS") {
                    store.showSettings = true
                }
                Spacer()
                footerButton("QUIT") {
                    store.quit()
                }
            }
            .font(.system(size: 11, design: .monospaced))
            .tracking(0.4)
        }
        .padding(.vertical, 8)
    }

    private func errorBanner(_ message: String) -> some View {
        Text(message)
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(.red)
            .fixedSize(horizontal: false, vertical: true)
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 4)
                    .fill(Color.red.opacity(0.08))
            )
    }
}

/// Additive height of non-scroll chrome pieces (header, tabs, footer, error shells).
private struct ChromeHeightPreferenceKey: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value += nextValue()
    }
}

/// Natural height of the report body sections (uncapped).
private struct BodyHeightPreferenceKey: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

// MARK: - Settings (IX-C Minimal Mono)

public struct SettingsView: View {
    @ObservedObject public var store: UsageStore
    @ObservedObject public var settings: AppSettings
    @Environment(\.dismiss) private var dismiss

    public init(store: UsageStore, settings: AppSettings) {
        self.store = store
        self.settings = settings
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            settingsHeader
                .padding(.bottom, 18)

            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 20) {
                    menuBarSection
                    scanningSection
                    cliSection
                }
            }

            settingsFooter
                .padding(.top, 16)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 18)
        .frame(width: 420, height: 360)
    }

    // MARK: Header

    private var settingsHeader: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                Text("SETTINGS")
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .tracking(1.6)
                Text("tokens menu bar")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button {
                dismiss()
            } label: {
                Text("DONE")
                    .font(.system(size: 11, design: .monospaced))
                    .tracking(0.6)
                    .foregroundStyle(.primary)
            }
            .buttonStyle(.plain)
            .keyboardShortcut(.cancelAction)
        }
    }

    // MARK: Menu Bar

    private var menuBarSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            settingsSectionLabel("MENU BAR")

            HStack(alignment: .firstTextBaseline) {
                Text("DISPLAY")
                    .font(.system(size: 11, design: .monospaced))
                    .tracking(0.4)
                Spacer(minLength: 12)
                displaySegmentedControl
            }

            Text("Title in status item: tokens only / cost only / both")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var displaySegmentedControl: some View {
        HStack(spacing: 0) {
            ForEach(MenuBarDisplayMode.allCases) { mode in
                Button {
                    settings.displayMode = mode
                    store.updateStatusTitle()
                } label: {
                    Text(mode.title.uppercased())
                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                        .tracking(0.4)
                        .foregroundStyle(settings.displayMode == mode ? Color(nsColor: .windowBackgroundColor) : Color.primary)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                        .background(
                            Rectangle()
                                .fill(settings.displayMode == mode ? Color.primary : Color.clear)
                        )
                }
                .buttonStyle(.plain)
            }
        }
        .overlay(
            Rectangle()
                .strokeBorder(Color.primary.opacity(0.35), lineWidth: 1)
        )
    }

    // MARK: Scanning

    private var scanningSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            settingsSectionLabel("SCANNING")

            HStack(alignment: .center) {
                Text("INTERVAL")
                    .font(.system(size: 11, design: .monospaced))
                    .tracking(0.4)
                Spacer(minLength: 12)
                Picker("", selection: $settings.scanInterval) {
                    ForEach(ScanIntervalOption.allCases) { option in
                        Text(option.title.uppercased()).tag(option)
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .font(.system(size: 11, design: .monospaced))
                .onChange(of: settings.scanInterval) { _ in store.restartTimer() }
            }

            Button {
                store.fullRescan()
                dismiss()
            } label: {
                HStack(alignment: .center, spacing: 12) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("FULL RESCAN NOW")
                            .font(.system(size: 11, weight: .medium, design: .monospaced))
                            .tracking(0.4)
                            .foregroundStyle(.primary)
                        Text("Ignore caches · rebuild snapshot")
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 8)
                    Text("RUN")
                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                        .tracking(0.6)
                        .foregroundStyle(store.isLoading || store.binaryMissing ? Color.secondary.opacity(0.5) : Color.secondary)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(
                    RoundedRectangle(cornerRadius: 4)
                        .fill(Color.primary.opacity(0.04))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 4)
                        .strokeBorder(Color.primary.opacity(0.18), lineWidth: 1)
                )
            }
            .buttonStyle(.plain)
            .disabled(store.isLoading || store.binaryMissing)
        }
    }

    // MARK: CLI

    private var cliSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            settingsSectionLabel("CLI")

            VStack(alignment: .leading, spacing: 6) {
                Text("RESOLVED PATH")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .tracking(0.8)

                Text(store.binaryPath ?? "not found")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(store.binaryMissing ? Color.red : Color.primary)
                    .textSelection(.enabled)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(
                        RoundedRectangle(cornerRadius: 4)
                            .fill(Color.primary.opacity(0.04))
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 4)
                            .strokeBorder(Color.primary.opacity(0.12), lineWidth: 1)
                    )
            }

            if let error = store.lastError {
                Text(error)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    // MARK: Footer

    private var settingsFooter: some View {
        HStack(spacing: 10) {
            Button {
                store.resolveBinary()
            } label: {
                Text("RECHECK CLI")
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .tracking(0.6)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    .foregroundStyle(.primary)
                    .overlay(
                        RoundedRectangle(cornerRadius: 4)
                            .strokeBorder(Color.primary.opacity(0.35), lineWidth: 1)
                    )
            }
            .buttonStyle(.plain)

            Button {
                dismiss()
            } label: {
                Text("DONE")
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .tracking(0.6)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    .foregroundStyle(Color(nsColor: .windowBackgroundColor))
                    .background(
                        RoundedRectangle(cornerRadius: 4)
                            .fill(Color.primary)
                    )
            }
            .buttonStyle(.plain)
            .keyboardShortcut(.defaultAction)
        }
    }

    private func settingsSectionLabel(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(.secondary)
            .tracking(1.0)
            .textCase(.uppercase)
    }
}
