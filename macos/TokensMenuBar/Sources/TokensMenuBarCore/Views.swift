import SwiftUI

public struct MenuPanelView: View {
    @ObservedObject public var store: UsageStore
    @ObservedObject public var settings: AppSettings

    public init(store: UsageStore, settings: AppSettings) {
        self.store = store
        self.settings = settings
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            if store.binaryMissing {
                missingCLI
            } else if let error = store.lastError, store.report == nil {
                errorBanner(error)
            } else if let report = store.report {
                periodPicker
                summaryCard(report)
                breakdownCard(report)
                clientSection(report)
                modelSection(report)
                daySection(report)
                if let error = store.lastError {
                    errorBanner(error)
                }
                footer(report)
            } else {
                ProgressView("Scanning local usage…")
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, 24)
            }
        }
        .padding(14)
        .frame(width: 360)
    }

    private var header: some View {
        HStack {
            Text("Tokens")
                .font(.headline)
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
    }

    private func summaryCard(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                metric("Tokens", Formatting.compactTokens(report.summary.totalTokens))
                Spacer()
                metric("Cost", Formatting.cost(report.summary.totalCost))
                Spacer()
                metric("Msgs", "\(report.summary.messages)")
            }
            Text("\(report.dateRange.start) → \(report.dateRange.end)")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: .controlBackgroundColor)))
    }

    private func breakdownCard(_ report: UsageReport) -> some View {
        let b = report.tokenBreakdown
        return VStack(alignment: .leading, spacing: 4) {
            Text("Token breakdown")
                .font(.caption)
                .foregroundStyle(.secondary)
            HStack(spacing: 8) {
                chip("in", b.input)
                chip("out", b.output)
                chip("cache", b.cacheRead)
                chip("reason", b.reasoning)
            }
        }
    }

    private func clientSection(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("By client")
                .font(.caption)
                .foregroundStyle(.secondary)
            if report.byClient.isEmpty {
                Text("No client data")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(report.byClient.prefix(8)) { client in
                    DisclosureGroup {
                        ForEach(client.models.prefix(6)) { model in
                            row(
                                title: model.modelId,
                                subtitle: model.providerId,
                                tokens: model.tokens,
                                cost: model.cost,
                                share: model.share
                            )
                        }
                    } label: {
                        row(
                            title: client.client,
                            subtitle: Formatting.percent(client.share),
                            tokens: client.tokens,
                            cost: client.cost,
                            share: client.share
                        )
                    }
                }
            }
        }
    }

    private func modelSection(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("By model")
                .font(.caption)
                .foregroundStyle(.secondary)
            ForEach(report.byModel.prefix(8)) { model in
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
        VStack(alignment: .leading, spacing: 6) {
            Text("By day")
                .font(.caption)
                .foregroundStyle(.secondary)
            ForEach(report.byDay.suffix(10).reversed()) { day in
                HStack {
                    Text(day.date)
                        .font(.caption)
                        .frame(width: 84, alignment: .leading)
                    GeometryReader { geo in
                        let width = max(4, geo.size.width * CGFloat(min(max(day.shareProxy(in: report), 0.02), 1)))
                        RoundedRectangle(cornerRadius: 2)
                            .fill(Color.accentColor.opacity(0.35 + 0.1 * Double(day.intensity)))
                            .frame(width: width, height: 8)
                    }
                    .frame(height: 8)
                    Text(Formatting.compactTokens(day.tokens))
                        .font(.caption2.monospacedDigit())
                        .frame(width: 52, alignment: .trailing)
                }
            }
        }
    }

    private func footer(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Updated \(Formatting.relativeTime(fromISO8601: report.generatedAt)) · \(report.scan.mode)")
                .font(.caption2)
                .foregroundStyle(.secondary)
            HStack {
                Button("Refresh") { store.manualRefresh() }
                    .disabled(store.isLoading)
                Button("Settings…") { store.showSettings = true }
                Spacer()
                Button("tokens.ci") { store.openTokensSite() }
                Button("Quit") { store.quit() }
            }
            .buttonStyle(.borderless)
            .controlSize(.small)
        }
    }

    private var missingCLI: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("tokens CLI not found")
                .font(.subheadline.weight(.semibold))
            Text("Install with Homebrew:\nbrew install owo-network/brew/tokens")
                .font(.caption)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            HStack {
                Button("Recheck") { store.resolveBinary(); store.manualRefresh() }
                Button("Settings…") { store.showSettings = true }
                Spacer()
                Button("Quit") { store.quit() }
            }
            .buttonStyle(.borderless)
        }
        .padding(.vertical, 8)
    }

    private func errorBanner(_ message: String) -> some View {
        Text(message)
            .font(.caption)
            .foregroundStyle(.red)
            .fixedSize(horizontal: false, vertical: true)
    }

    private func metric(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.title3.monospacedDigit().weight(.semibold))
        }
    }

    private func chip(_ label: String, _ value: Int64) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
            Text(Formatting.compactTokens(value))
                .font(.caption.monospacedDigit())
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func row(title: String, subtitle: String, tokens: Int64, cost: Double, share: Double) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack {
                Text(title)
                    .font(.caption.weight(.medium))
                    .lineLimit(1)
                Spacer()
                Text(Formatting.compactTokens(tokens))
                    .font(.caption.monospacedDigit())
                Text(Formatting.cost(cost))
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(width: 56, alignment: .trailing)
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.secondary.opacity(0.15))
                    Capsule()
                        .fill(Color.accentColor.opacity(0.7))
                        .frame(width: max(4, geo.size.width * CGFloat(min(max(share, 0), 1))))
                }
            }
            .frame(height: 4)
            Text(subtitle)
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 2)
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
                        .font(.caption)
                        .textSelection(.enabled)
                        .foregroundStyle(store.binaryMissing ? .red : .secondary)
                }
                if let error = store.lastError {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                }
                Button("Recheck CLI") {
                    store.resolveBinary()
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 380, height: 320)
        .padding()
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Done") { dismiss() }
            }
        }
    }
}
