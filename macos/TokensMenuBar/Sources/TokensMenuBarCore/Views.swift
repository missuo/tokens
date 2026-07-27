import SwiftUI

public struct MenuPanelView: View {
    @ObservedObject public var store: UsageStore
    @ObservedObject public var settings: AppSettings

    public init(store: UsageStore, settings: AppSettings) {
        self.store = store
        self.settings = settings
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
                .padding(.horizontal, MenuBarLayout.horizontalPadding)
                .padding(.top, 16)
                .padding(.bottom, 4)

            if store.binaryMissing {
                missingCLI
                    .padding(.horizontal, MenuBarLayout.horizontalPadding)
                    .padding(.vertical, 18)
            } else if let error = store.lastError, store.report == nil {
                errorBanner(error)
                    .padding(.horizontal, MenuBarLayout.horizontalPadding)
                    .padding(.vertical, 18)
            } else if let report = store.report {
                periodTabs
                    .padding(.horizontal, MenuBarLayout.horizontalPadding)
                    .padding(.top, 14)
                    .padding(.bottom, 6)

                ScrollView(.vertical, showsIndicators: true) {
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
                    .padding(.horizontal, MenuBarLayout.horizontalPadding)
                    .padding(.top, 18)
                    .padding(.bottom, 12)
                }
                .frame(maxHeight: MenuBarLayout.contentMaxHeight)

                footer(report)
                    .padding(.horizontal, MenuBarLayout.horizontalPadding)
                    .padding(.top, 4)
                    .padding(.bottom, 14)
            } else {
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
        .frame(width: MenuBarLayout.panelWidth)
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
                Button {
                    store.setPeriod(period)
                } label: {
                    Text(period.monoTitle)
                        .font(.system(size: 11, design: .monospaced))
                        .tracking(0.4)
                        .foregroundStyle(store.period == period ? Color.primary : Color.secondary)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                        .overlay(alignment: .bottom) {
                            Rectangle()
                                .fill(store.period == period ? Color.primary : Color.clear)
                                .frame(height: 2)
                        }
                }
                .buttonStyle(.plain)
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
        VStack(alignment: .leading, spacing: 12) {
            sectionLabel("CLIENT")
            if report.byClient.isEmpty {
                emptyHint("No client data")
            } else if report.byClient.count > MenuBarLayout.nestedListThreshold {
                ScrollView {
                    clientRows(report.byClient)
                }
                .frame(maxHeight: MenuBarLayout.nestedListMaxHeight)
            } else {
                clientRows(report.byClient)
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

    private func clientRow(_ client: ClientUsage) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(client.client)
                    .font(.system(size: 12, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                Text("\(Formatting.compactTokens(client.tokens)) · \(Formatting.percent(client.share))")
                    .font(.system(size: 12, design: .monospaced))
                    .monospacedDigit()
                    .foregroundStyle(.primary)
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
                ScrollView {
                    modelRows(report.byModel)
                }
                .frame(maxHeight: MenuBarLayout.nestedListMaxHeight)
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
            }
            Spacer(minLength: 8)
            Text(Formatting.compactTokens(model.tokens))
                .font(.system(size: 12, design: .monospaced))
                .monospacedDigit()
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
            CostChartView(days: report.byDay)
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
            .minimumScaleFactor(0.7)

            Spacer(minLength: 8)

            footerButton("REFRESH", disabled: store.isLoading) {
                store.manualRefresh()
            }
            footerButton("SETTINGS") {
                store.showSettings = true
            }
            footerButton("TOKENS.CI") {
                store.openTokensSite()
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

// MARK: - Settings (Task 4 — structure left as-is)

public struct SettingsView: View {
    @ObservedObject public var store: UsageStore
    @ObservedObject public var settings: AppSettings
    @Environment(\.dismiss) private var dismiss

    public init(store: UsageStore, settings: AppSettings) {
        self.store = store
        self.settings = settings
    }

    public var body: some View {
        Form {
            Section("Menu Bar") {
                Picker("Display", selection: $settings.displayMode) {
                    ForEach(MenuBarDisplayMode.allCases) { mode in
                        Text(mode.title).tag(mode)
                    }
                }
                .onChange(of: settings.displayMode) { _ in store.updateStatusTitle() }
            }

            Section("Scanning") {
                Picker("Interval", selection: $settings.scanInterval) {
                    ForEach(ScanIntervalOption.allCases) { option in
                        Text(option.title).tag(option)
                    }
                }
                .onChange(of: settings.scanInterval) { _ in store.restartTimer() }

                Button("Full Rescan Now") {
                    store.fullRescan()
                    dismiss()
                }
                .disabled(store.isLoading || store.binaryMissing)
            }

            Section("CLI") {
                LabeledContent("Resolved path") {
                    Text(store.binaryPath ?? "not found")
                        .font(.body)
                        .textSelection(.enabled)
                        .foregroundStyle(store.binaryMissing ? .red : .secondary)
                }
                if let error = store.lastError {
                    Text(error)
                        .font(.body)
                        .foregroundStyle(.red)
                }
                Button("Recheck CLI") {
                    store.resolveBinary()
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 420, height: 360)
        .padding()
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Done") { dismiss() }
            }
        }
    }
}
