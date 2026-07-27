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
                .padding(.horizontal, 16)
                .padding(.top, 14)
                .padding(.bottom, 10)

            Divider()

            if store.binaryMissing {
                missingCLI
                    .padding(16)
            } else if let error = store.lastError, store.report == nil {
                errorBanner(error)
                    .padding(16)
            } else if let report = store.report {
                periodPicker
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)

                ScrollView(.vertical, showsIndicators: true) {
                    VStack(alignment: .leading, spacing: 16) {
                        summaryCard(report)
                        breakdownCard(report)
                        clientSection(report)
                        modelSection(report)
                        daySection(report)
                        if let error = store.lastError {
                            errorBanner(error)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 12)
                }
                .frame(maxHeight: 420)

                Divider()
                footer(report)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
            } else {
                VStack(spacing: 12) {
                    ProgressView()
                        .controlSize(.regular)
                    Text(store.isLoading ? "Scanning local usage…" : "No data yet")
                        .font(.body)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, minHeight: 160)
                .padding(16)
            }
        }
        .frame(width: 400)
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("Tokens")
                .font(.title3.weight(.semibold))
            Spacer()
            if store.isLoading {
                ProgressView()
                    .controlSize(.small)
            }
        }
    }

    private var periodPicker: some View {
        Picker("Period", selection: Binding(
            get: { store.period },
            set: { store.setPeriod($0) }
        )) {
            ForEach(UsagePeriod.allCases) { period in
                Text(period.title).tag(period)
            }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .controlSize(.large)
    }

    private func summaryCard(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top) {
                metric("Tokens", Formatting.compactTokens(report.summary.totalTokens))
                Spacer(minLength: 8)
                metric("Cost", Formatting.cost(report.summary.totalCost))
                Spacer(minLength: 8)
                metric("Msgs", "\(report.summary.messages)")
            }
            Text("\(report.dateRange.start) → \(report.dateRange.end)")
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color(nsColor: .controlBackgroundColor)))
    }

    private func breakdownCard(_ report: UsageReport) -> some View {
        let b = report.tokenBreakdown
        return VStack(alignment: .leading, spacing: 8) {
            sectionTitle("Token breakdown")
            HStack(spacing: 12) {
                chip("in", b.input)
                chip("out", b.output)
                chip("cache", b.cacheRead)
                chip("reason", b.reasoning)
            }
        }
    }

    private func clientSection(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            sectionTitle("By client")
            if report.byClient.isEmpty {
                Text("No client data")
                    .font(.body)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(report.byClient.prefix(12)) { client in
                    DisclosureGroup {
                        VStack(alignment: .leading, spacing: 10) {
                            ForEach(client.models.prefix(10)) { model in
                                row(
                                    title: model.modelId,
                                    subtitle: model.providerId,
                                    tokens: model.tokens,
                                    cost: model.cost,
                                    share: model.share
                                )
                            }
                        }
                        .padding(.leading, 18)
                        .padding(.top, 6)
                        .padding(.bottom, 4)
                    } label: {
                        row(
                            title: client.client,
                            subtitle: Formatting.percent(client.share),
                            tokens: client.tokens,
                            cost: client.cost,
                            share: client.share
                        )
                        .padding(.vertical, 2)
                    }
                    .padding(.vertical, 2)
                }
            }
        }
    }

    private func modelSection(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            sectionTitle("By model")
            ForEach(report.byModel.prefix(12)) { model in
                row(
                    title: model.modelId,
                    subtitle: model.providerId,
                    tokens: model.tokens,
                    cost: model.cost,
                    share: model.share
                )
            }
        }
    }

    private func daySection(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            sectionTitle("By day")
            ForEach(report.byDay.suffix(14).reversed()) { day in
                HStack(spacing: 10) {
                    Text(day.date)
                        .font(.body.monospacedDigit())
                        .frame(width: 100, alignment: .leading)
                    GeometryReader { geo in
                        let width = max(
                            6,
                            geo.size.width * CGFloat(min(max(day.shareProxy(in: report), 0.02), 1))
                        )
                        RoundedRectangle(cornerRadius: 3)
                            .fill(Color.accentColor.opacity(0.35 + 0.1 * Double(day.intensity)))
                            .frame(width: width, height: 10)
                            .frame(maxHeight: .infinity, alignment: .center)
                    }
                    .frame(height: 14)
                    Text(Formatting.compactTokens(day.tokens))
                        .font(.body.monospacedDigit())
                        .frame(width: 64, alignment: .trailing)
                }
            }
        }
    }

    private func footer(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Updated \(Formatting.relativeTime(fromISO8601: report.generatedAt)) · \(report.scan.mode)")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            HStack(spacing: 14) {
                Button("Refresh") { store.manualRefresh() }
                    .disabled(store.isLoading)
                Button("Settings…") { store.showSettings = true }
                Spacer()
                Button("tokens.ci") { store.openTokensSite() }
                Button("Quit") { store.quit() }
            }
            .buttonStyle(.borderless)
            .controlSize(.regular)
            .font(.body)
        }
    }

    private var missingCLI: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("tokens CLI not found")
                .font(.headline)
            Text("Install or build the Menu Bar-capable CLI, then Recheck.\n\nbrew install owo-network/brew/tokens\n# or build this repo and link ~/.local/bin/tokens")
                .font(.body)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            HStack(spacing: 14) {
                Button("Recheck") { store.resolveBinary(); store.manualRefresh() }
                Button("Settings…") { store.showSettings = true }
                Spacer()
                Button("Quit") { store.quit() }
            }
            .buttonStyle(.borderless)
            .font(.body)
        }
        .padding(.vertical, 8)
    }

    private func errorBanner(_ message: String) -> some View {
        Text(message)
            .font(.body)
            .foregroundStyle(.red)
            .fixedSize(horizontal: false, vertical: true)
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(Color.red.opacity(0.08))
            )
    }

    private func sectionTitle(_ text: String) -> some View {
        Text(text)
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(.secondary)
    }

    private func metric(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.title2.monospacedDigit().weight(.semibold))
                .lineLimit(1)
                .minimumScaleFactor(0.7)
        }
    }

    private func chip(_ label: String, _ value: Int64) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Text(Formatting.compactTokens(value))
                .font(.body.monospacedDigit().weight(.medium))
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func row(
        title: String,
        subtitle: String,
        tokens: Int64,
        cost: Double,
        share: Double
    ) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(title)
                    .font(.body.weight(.medium))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                Text(Formatting.compactTokens(tokens))
                    .font(.body.monospacedDigit())
                Text(Formatting.cost(cost))
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(width: 64, alignment: .trailing)
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.secondary.opacity(0.15))
                    Capsule()
                        .fill(Color.accentColor.opacity(0.75))
                        .frame(width: max(6, geo.size.width * CGFloat(min(max(share, 0), 1))))
                }
            }
            .frame(height: 5)
            Text(subtitle)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .padding(.vertical, 4)
    }
}

private extension DayUsage {
    func shareProxy(in report: UsageReport) -> Double {
        let maxTokens = report.byDay.map(\.tokens).max() ?? 1
        guard maxTokens > 0 else { return 0 }
        return Double(tokens) / Double(maxTokens)
    }
}

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
