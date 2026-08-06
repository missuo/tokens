import AppKit
import SwiftUI

public struct MenuPanelView: View {
    @ObservedObject public var store: UsageStore
    @ObservedObject public var settings: AppSettings
    @ObservedObject public var layout: PanelLayoutState
    /// Called whenever the panel’s ideal size changes so `NSPopover.contentSize` can shrink-wrap.
    public var onIdealSizeChange: ((CGSize) -> Void)?

    /// Intrinsic height of the scrollable body (sections only).
    @State private var bodyContentHeight: CGFloat = 0
    /// Intrinsic height of header + tabs + footer (+ error shells).
    @State private var chromeHeight: CGFloat = 0
    /// Body viewport height — tracks measured content (CLIENT/PROJECT/MODEL lists push this open).
    @State private var bodyViewportHeight: CGFloat = MenuBarLayout.fallbackContentHeight
    /// CLIENT / PROJECT / MODEL lists: how many rows are visible (chevron loads more).
    @State private var clientVisibleCount: Int = MenuBarLayout.listPageSize
    @State private var projectVisibleCount: Int = MenuBarLayout.listPageSize
    @State private var modelVisibleCount: Int = MenuBarLayout.listPageSize
    /// Per-project nested model page counts keyed by `ProjectUsage.id`.
    @State private var projectModelVisibleCounts: [String: Int] = [:]
    @State private var showCustomEditor = false
    /// Root page: main dashboard vs Advanced (slides horizontally).
    @State private var page: PanelPage = .main
    /// While the pages slide, measurement mixes both pages — hold popover size.
    @State private var isPagingBetweenPages = false
    @State private var requestDatePickerFocus = false
    @State private var draftCustomRange = DateRangePickerConversion.today(timeZone: .current)
    @FocusState private var customTriggerFocused: Bool

    public init(
        store: UsageStore,
        settings: AppSettings,
        layout: PanelLayoutState,
        onIdealSizeChange: ((CGSize) -> Void)? = nil
    ) {
        self.store = store
        self.settings = settings
        self.layout = layout
        self.onIdealSizeChange = onIdealSizeChange
    }

    /// Cap from the presentation display (refreshed by AppDelegate from the clicked display).
    private var panelMaxHeight: CGFloat { layout.maxHeight }

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

    private var reportingTimeZone: TimeZone {
        if let identifier = store.report?.dateRange.timezone,
           let zone = TimeZone(identifier: identifier) {
            return zone
        }
        return .current
    }

    public var body: some View {
        ZStack(alignment: .top) {
            if page == .main {
                mainPage
                    .transition(.asymmetric(
                        insertion: .move(edge: .leading),
                        removal: .move(edge: .leading)
                    ))
            } else {
                advancedPage
                    .transition(.asymmetric(
                        insertion: .move(edge: .trailing),
                        removal: .move(edge: .trailing)
                    ))
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
        .onDisappear { closeCustomEditor(restoreFocus: false) }
        .onChange(of: layout.presentationGeneration) { _ in
            closeCustomEditor(restoreFocus: false)
            // Each popover open starts on the dashboard page.
            page = .main
            isPagingBetweenPages = false
        }
        .onChange(of: store.report?.generatedAt) { _ in syncBodyHeightAndPublish() }
        .onChange(of: store.selection) { _ in
            // Reset expand pages; keep prior panel height until new report lands.
            clientVisibleCount = MenuBarLayout.listPageSize
            projectVisibleCount = MenuBarLayout.listPageSize
            modelVisibleCount = MenuBarLayout.listPageSize
            projectModelVisibleCounts = [:]
        }
        .onChange(of: store.isLoading) { _ in syncBodyHeightAndPublish() }
        .onChange(of: store.binaryMissing) { _ in syncBodyHeightAndPublish() }
        .onChange(of: store.lastError) { _ in syncBodyHeightAndPublish() }
        .onChange(of: clientVisibleCount) { _ in
            DispatchQueue.main.async { syncBodyHeightAndPublish() }
        }
        .onChange(of: projectVisibleCount) { _ in
            DispatchQueue.main.async { syncBodyHeightAndPublish() }
        }
        .onChange(of: modelVisibleCount) { _ in
            DispatchQueue.main.async { syncBodyHeightAndPublish() }
        }
        .onChange(of: projectModelVisibleCounts) { _ in
            DispatchQueue.main.async { syncBodyHeightAndPublish() }
        }
        .onChange(of: layout.maxHeight) { _ in
            // Multi-monitor: opening on a smaller display must reclamp body + popover.
            syncBodyHeightAndPublish()
        }
    }

    private var mainPage: some View {
        VStack(alignment: .leading, spacing: 0) {
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
                    rangeControls
                        .padding(.horizontal, MenuBarLayout.horizontalPadding)
                        .padding(.top, 14)
                        .padding(.bottom, 6)
                        .zIndex(showCustomEditor ? 20 : 0)
                }

                // Always ScrollView (even when short) so TODAY vs 30D does not
                // flip layout structure and remeasure thrash the popover height.
                reportBody(report)
                    .opacity(store.isShowingStaleReport ? 0.55 : 1)
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
    }

    @ViewBuilder
    private func reportBody(_ report: UsageReport) -> some View {
        // Hide scrollbar chrome; wheel/trackpad scroll still works.
        ScrollView(.vertical, showsIndicators: false) {
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
            costChartSection(report)
            clientSection(report)
            modelSection(report)
            projectSection(report)
            advancedEntrySection
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

    /// Size body viewport to measured content and notify AppKit popover (coalesced there).
    /// Height is content-driven: CLIENT/MODEL list length sets the body, no forced tween.
    /// Skip resize while the tab is ahead of the loaded report (TODAY short vs 30D tall).
    private func syncBodyHeightAndPublish() {
        // Mid-slide both pages are mounted; publishing now would mix heights.
        guard !isPagingBetweenPages else { return }
        // Keep the previous body viewport while a different range is loading.
        if !store.isShowingStaleReport,
           let target = targetBodyHeight,
           abs(target - bodyViewportHeight) > 0.5 {
            bodyViewportHeight = target
        }

        let chrome = chromeHeight > 0 ? chromeHeight : 140
        let body: CGFloat
        if store.report == nil {
            body = 0
        } else if store.isShowingStaleReport {
            body = bodyViewportHeight
        } else {
            body = targetBodyHeight ?? bodyViewportHeight
        }
        let ideal = min(chrome + body, panelMaxHeight)
        let height = max(ideal, 120)
        onIdealSizeChange?(CGSize(width: MenuBarLayout.panelWidth, height: height))
    }

    // MARK: - Range controls

    private var rangeControls: some View {
        HStack(spacing: 0) {
            ForEach(UsagePeriod.allCases) { period in
                let selected = store.selection == .preset(period)
                Button {
                    closeCustomEditor(restoreFocus: false)
                    store.setPeriod(period)
                } label: {
                    Text(period.monoTitle)
                        .font(.system(size: 11, design: .monospaced))
                        .tracking(0.4)
                        .foregroundStyle(selected ? Color.primary : Color.secondary)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 8)
                        .contentShape(Rectangle())
                        .overlay(alignment: .bottom) {
                            Rectangle()
                                .fill(selected ? Color.primary : Color.clear)
                                .frame(height: 2)
                        }
                }
                .buttonStyle(.plain)
                .frame(maxWidth: .infinity)
                .accessibilityAddTraits(selected ? .isSelected : [])
                .accessibilityLabel(period.monoTitle)
            }

            customRangeTrigger

            // Fixed slot: opacity toggles instead of insert/remove so the tab
            // buttons never shift sideways while a range refresh runs.
            ProgressView()
                .controlSize(.mini)
                .frame(width: 14)
                .padding(.leading, 4)
                .opacity(store.isLoading ? 1 : 0)
                .accessibilityHidden(!store.isLoading)
        }
        .overlay(alignment: .topTrailing) {
            if showCustomEditor {
                customRangeEditor
                    .offset(y: 42)
                    .zIndex(30)
            }
        }
    }

    private var customEditorAvailable: Bool {
        store.selection != .preset(.all) || !store.isShowingStaleReport
    }

    private var customRangeTrigger: some View {
        let activeRange = store.selection.customRange
        let label = activeRange.map {
            Formatting.compactDateRange($0, timeZone: reportingTimeZone)
        }
        let selected = activeRange != nil

        return Button { openCustomEditor() } label: {
            Group {
                if let label {
                    Text(label)
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                } else {
                    Image(systemName: "calendar")
                }
            }
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(selected ? Color.primary : Color.secondary)
            .frame(minWidth: selected ? 82 : 34, maxWidth: selected ? 108 : 34)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(selected ? Color.primary : Color.clear)
                    .frame(height: 2)
            }
        }
        .buttonStyle(.plain)
        .disabled(!customEditorAvailable)
        .opacity(customEditorAvailable ? 1 : 0.45)
        .focused($customTriggerFocused)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityLabel(
            label.map { "Custom date range, \($0)" } ?? "Custom date range"
        )
        .accessibilityHint(
            customEditorAvailable
                ? "Opens the inclusive date range editor"
                : "Available after the All range finishes loading"
        )
    }

    private var customRangeEditor: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("CUSTOM RANGE")
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
                    .tracking(0.8)
                Spacer()
                Button("Cancel") { closeCustomEditor(restoreFocus: true) }
                    .buttonStyle(.plain)
                    .font(.system(size: 10, design: .monospaced))
            }

            AppKitDateRangePicker(
                selection: $draftCustomRange,
                requestFocus: $requestDatePickerFocus,
                timeZone: reportingTimeZone,
                locale: .current,
                maximumDate: DateRangePickerConversion.maximumDate(
                    timeZone: reportingTimeZone
                ) ?? Date()
            )
            .frame(width: 300, height: 210)

            HStack {
                Text(
                    Formatting.compactDateRange(
                        draftCustomRange,
                        timeZone: reportingTimeZone
                    )
                )
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                Spacer()
                Button("APPLY RANGE") {
                    store.setCustomRange(draftCustomRange)
                    closeCustomEditor(restoreFocus: true)
                }
                .buttonStyle(.plain)
                .font(.system(size: 10, weight: .semibold, design: .monospaced))
                .disabled(!draftCustomRange.isOrdered)
            }
        }
        .padding(12)
        .frame(width: 324)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(Color(nsColor: .windowBackgroundColor))
                .shadow(color: .black.opacity(0.24), radius: 10, y: 4)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .strokeBorder(Color.primary.opacity(0.18), lineWidth: 1)
        )
        .onExitCommand { closeCustomEditor(restoreFocus: true) }
        .accessibilityElement(children: .contain)
    }

    private func openCustomEditor() {
        if showCustomEditor {
            closeCustomEditor(restoreFocus: true)
            return
        }
        switch store.selection {
        case .custom(let range):
            draftCustomRange = range
        case .preset(let period):
            if let report = store.report, report.selection == store.selection {
                draftCustomRange = DateSelectionRange(
                    startDate: report.dateRange.startDate,
                    endDate: report.dateRange.endDate
                )
            } else {
                guard let range = DateRangePickerConversion.range(
                    for: period,
                    timeZone: reportingTimeZone
                ) else { return }
                draftCustomRange = range
            }
        }
        showCustomEditor = true
        requestDatePickerFocus = true
    }

    private func closeCustomEditor(restoreFocus: Bool) {
        showCustomEditor = false
        requestDatePickerFocus = false
        if restoreFocus {
            DispatchQueue.main.async { customTriggerFocused = true }
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
        }
    }

    // MARK: - BREAKDOWN

    private func breakdownSection(_ report: UsageReport) -> some View {
        let b = report.tokenBreakdown
        // Input cache % = cache-read hit rate (cacheRead / (input + cacheRead)).
        // No “output cache” in the schema — only show rate on the input card.
        let inputCache = Formatting.inputCacheRate(input: b.input, cacheRead: b.cacheRead)
        let items: [(String, Int64, Double?)] = [
            ("input", b.input, inputCache),
            ("output", b.output, nil),
            ("cache", b.cacheRead, nil),
            ("reason", b.reasoning, nil),
        ]
        let accents: [Double] = [1, 0.72, 0.48, 0.28]

        return HStack(spacing: 6) {
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                breakdownCard(
                    label: item.0,
                    value: item.1,
                    cachePercent: item.2,
                    topAccent: Color.primary.opacity(accents[index])
                )
            }
        }
    }

    private func breakdownCard(
        label: String,
        value: Int64,
        cachePercent: Double?,
        topAccent: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            HStack(alignment: .firstTextBaseline, spacing: 0) {
                Text(Formatting.compactTokens(value))
                    .font(.system(size: 12, weight: .bold, design: .monospaced))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                if let cachePercent {
                    Text(" · ")
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundStyle(.secondary)
                    Text(Formatting.percent(cachePercent))
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                        .lineLimit(1)
                        .layoutPriority(1)
                }
            }
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
        .accessibilityElement(children: .combine)
        .accessibilityLabel(breakdownAccessibilityLabel(
            label: label,
            value: value,
            cachePercent: cachePercent
        ))
    }

    private func breakdownAccessibilityLabel(
        label: String,
        value: Int64,
        cachePercent: Double?
    ) -> String {
        var parts = ["\(label) \(Formatting.compactTokens(value))"]
        if let cachePercent {
            parts.append("input cache \(Formatting.percent(cachePercent))")
        }
        return parts.joined(separator: ", ")
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
                    expandChevron(remaining: all.count - visible.count, accessibilityNoun: "clients") {
                        // Content grows first; panel height follows measurement.
                        clientVisibleCount = min(
                            clientVisibleCount + MenuBarLayout.listPageSize,
                            all.count
                        )
                    }
                }
            }
        }
    }

    private func clientRows(_ clients: [ClientUsage]) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(clients) { client in
                clientRow(client)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        // Chevron page-in only. Period switches remeasure content and snap height —
        // the lists push the panel open; no forced height tween.
        .animation(.easeOut(duration: 0.18), value: clientVisibleCount)
    }

    /// Centered "More" control: load another page (no nested scrollbar).
    private func expandChevron(
        remaining: Int,
        accessibilityNoun: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Text("More")
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .tracking(0.4)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 6)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Show more \(accessibilityNoun)")
        .accessibilityHint("\(remaining) more")
    }

    /// Icon-only expand control for nested Project model pagination.
    private func projectModelExpandIcon(
        remaining: Int,
        accessibilityNoun: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: "chevron.down")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 6)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Show more \(accessibilityNoun)")
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

    // MARK: - PROJECT

    private func projectSection(_ report: UsageReport) -> some View {
        let all = report.byProject
        let visible = Array(all.prefix(max(projectVisibleCount, 0)))
        let hasMore = all.count > visible.count

        return VStack(alignment: .leading, spacing: 12) {
            sectionLabel("PROJECT")
            if all.isEmpty {
                emptyHint("No project data")
            } else {
                projectRows(visible)
                if hasMore {
                    expandChevron(remaining: all.count - visible.count, accessibilityNoun: "projects") {
                        projectVisibleCount = min(
                            projectVisibleCount + MenuBarLayout.listPageSize,
                            all.count
                        )
                    }
                }
            }
        }
    }

    private func projectRows(_ projects: [ProjectUsage]) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            ForEach(projects) { project in
                projectRow(project)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .animation(.easeOut(duration: 0.18), value: projectVisibleCount)
    }

    private func projectRow(_ project: ProjectUsage) -> some View {
        let visibleModelCount = projectModelVisibleCounts[project.id]
            ?? MenuBarLayout.projectModelPageSize
        let modelPage = ProjectModelPresentation.page(
            from: project.models,
            visibleCount: visibleModelCount
        )
        let folderName = project.folderName

        return VStack(alignment: .leading, spacing: 7) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(folderName)
                    .font(.system(size: 12, weight: .medium, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
                    .help(folderName)
                    .accessibilityLabel(folderName)
                Text("\(Formatting.cost(project.cost)) · \(Formatting.compactTokens(project.tokens))")
                    .font(.system(size: 12, design: .monospaced))
                    .monospacedDigit()
                    .layoutPriority(2)
            }

            if modelPage.totalCount > 0 {
                HStack(alignment: .top, spacing: 9) {
                    Rectangle()
                        .fill(Color.primary.opacity(0.16))
                        .frame(width: 1)
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(modelPage.models) { model in
                            projectModelRow(model)
                        }
                        if modelPage.hasMore {
                            projectModelExpandIcon(
                                remaining: modelPage.remainingCount,
                                accessibilityNoun: "models for \(folderName)"
                            ) {
                                projectModelVisibleCounts[project.id] = min(
                                    visibleModelCount + MenuBarLayout.projectModelPageSize,
                                    modelPage.totalCount
                                )
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .padding(.leading, 3)
            }
        }
        .accessibilityElement(children: .contain)
    }

    private func projectModelRow(_ model: ProjectModelUsage) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            HStack(spacing: 4) {
                Text(model.modelId)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text("/")
                    .foregroundStyle(.tertiary)
                Text(model.providerId)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .font(.system(size: 11, design: .monospaced))
            .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)

            Text("\(Formatting.cost(model.cost)) · \(Formatting.compactTokens(model.tokens))")
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .monospacedDigit()
                .layoutPriority(2)
        }
        .accessibilityElement(children: .combine)
    }

    // MARK: - MODEL

    private func modelSection(_ report: UsageReport) -> some View {
        let all = report.byModel
        let visible = Array(all.prefix(max(modelVisibleCount, 0)))
        let hasMore = all.count > visible.count

        return VStack(alignment: .leading, spacing: 10) {
            sectionLabel("MODEL")
            if all.isEmpty {
                emptyHint("No model data")
            } else {
                modelRows(visible)
                if hasMore {
                    expandChevron(remaining: all.count - visible.count, accessibilityNoun: "models") {
                        modelVisibleCount = min(
                            modelVisibleCount + MenuBarLayout.listPageSize,
                            all.count
                        )
                    }
                }
            }
        }
    }

    private func modelRows(_ models: [ModelUsage]) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(models) { model in
                modelRow(model)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .animation(.easeOut(duration: 0.18), value: modelVisibleCount)
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
        let unplaced = report.timeSeries.unplaced
        return VStack(alignment: .leading, spacing: 12) {
            sectionLabel("\(report.timeSeries.granularity.title.uppercased()) COST")
            CostChartView(
                timeSeries: report.timeSeries,
                timeZone: reportingTimeZone
            )
            .id(store.selection)
            if unplaced.tokens != 0 || unplaced.cost != 0 || unplaced.messages != 0 {
                Text(
                    "UNPLACED · \(Formatting.cost(unplaced.cost)) · "
                        + "\(Formatting.compactTokens(unplaced.tokens)) TOKENS "
                        + "WITHOUT RELIABLE BUCKET TIME"
                )
                .font(.system(size: 9, design: .monospaced))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityLabel(
                    "Unplaced usage, \(Formatting.cost(unplaced.cost)), "
                        + "\(Formatting.compactTokens(unplaced.tokens)) tokens, "
                        + "without reliable bucket time"
                )
            }
        }
    }

    // MARK: - Advanced page

    /// Last section of the dashboard: entry row that slides to the Advanced page.
    private var advancedEntrySection: some View {
        VStack(alignment: .leading, spacing: 12) {
            sectionLabel("ADVANCED")
            Button {
                switchToPage(.advanced)
            } label: {
                HStack(alignment: .center, spacing: 12) {
                    Text("WEEKDAY × HOUR HEATMAP")
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .tracking(0.4)
                        .foregroundStyle(.primary)
                    Spacer(minLength: 8)
                    Image(systemName: "arrow.right")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.secondary)
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
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Open Advanced page")
            .accessibilityHint("Slides to the weekday by hour cost heatmap")
        }
    }

    @ViewBuilder
    private var advancedPage: some View {
        VStack(alignment: .leading, spacing: 0) {
            measuredChrome {
                advancedHeader
                    .padding(.horizontal, MenuBarLayout.horizontalPadding)
                    .padding(.top, 14)
                    .padding(.bottom, 6)
            }

            if let report = store.report {
                ScrollView(.vertical, showsIndicators: false) {
                    VStack(alignment: .leading, spacing: MenuBarLayout.sectionSpacing) {
                        AdvancedHeatmapSection(report: report)
                    }
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
                .opacity(store.isShowingStaleReport ? 0.55 : 1)
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
                    Text("NO DATA YET")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .tracking(0.8)
                        .frame(maxWidth: .infinity, minHeight: 160)
                        .padding(MenuBarLayout.horizontalPadding)
                }
            }
        }
    }

    /// Back arrow (top-left) + page label.
    private var advancedHeader: some View {
        HStack(spacing: 8) {
            Button {
                switchToPage(.main)
            } label: {
                Image(systemName: "chevron.left")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(Color.primary)
                    .frame(width: 22, height: 22)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Back to dashboard")
            sectionLabel("ADVANCED")
            Spacer()
        }
    }

    /// Horizontal slide between dashboard and Advanced. The popover never
    /// tweens height; hold size publishing until the slide settles, then snap.
    private func switchToPage(_ next: PanelPage) {
        guard next != page else { return }
        isPagingBetweenPages = true
        withAnimation(.easeInOut(duration: 0.24)) {
            page = next
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.28) {
            isPagingBetweenPages = false
            syncBodyHeightAndPublish()
        }
    }

    // MARK: - Footer

    private func footer(_ report: UsageReport) -> some View {
        HStack(spacing: 10) {
            Text(
                "UPDATED \(Formatting.relativeTime(fromISO8601: report.generatedAt).uppercased())"
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
            .accessibilityAddTraits(.isHeader)
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

/// Root pages of the menu panel.
private enum PanelPage {
    case main
    case advanced
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
    /// Custom row unit toggle (MIN / HR); derived when entering custom.
    @State private var customUnit: ScanIntervalCustomUnit = .minutes

    public init(store: UsageStore, settings: AppSettings) {
        self.store = store
        self.settings = settings
        let minutes = settings.scanInterval.customMinutesOrDefault
        _customUnit = State(initialValue: ScanIntervalCustomUnit.preferred(forMinutes: minutes))
    }

    public var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 32) {
                menuBarSection
                scanningSection
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 18)
        .frame(width: 420)
        .frame(minHeight: 320, maxHeight: 420)
        .onAppear {
            if settings.scanInterval.isCustom {
                customUnit = ScanIntervalCustomUnit.preferred(
                    forMinutes: settings.scanInterval.customMinutesOrDefault
                )
            }
        }
    }

    // MARK: Status title (menu bar display mode)

    private var menuBarSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            settingsSectionLabel("STATUS TITLE")
            displaySegmentedControl
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

            Text("INTERVAL")
                .font(.system(size: 11, design: .monospaced))
                .tracking(0.4)

            intervalChipRow

            if settings.scanInterval.isCustom {
                customIntervalRow
                Text("5 MIN – 24 H · STEPS ON LADDER")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.secondary)
            } else {
                Text(
                    settings.scanInterval.isManual
                        ? "BACKGROUND LOCAL RESCAN OFF · MANUAL ONLY"
                        : "BACKGROUND LOCAL RESCAN CADENCE"
                )
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
            }

            Button {
                store.fullRescan()
            } label: {
                HStack(alignment: .center, spacing: 12) {
                    Text("FULL RESCAN NOW")
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .tracking(0.4)
                        .foregroundStyle(.primary)
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

            Text(
                "We cache local session scans so historical data is not re-read on every refresh. Use Full Rescan if numbers look wrong — it clears caches and rebuilds from all session files."
            )
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
            .lineSpacing(2)
        }
    }

    private var intervalChipRow: some View {
        HStack(spacing: 6) {
            ForEach(ScanIntervalOption.chips) { chip in
                let selected = chip.matches(settings.scanInterval)
                Button {
                    selectIntervalChip(chip)
                } label: {
                    Text(chip.monoTitle)
                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                        .tracking(0.5)
                        .foregroundStyle(
                            selected
                                ? Color(nsColor: .windowBackgroundColor)
                                : Color.secondary
                        )
                        .padding(.horizontal, 9)
                        .padding(.vertical, 7)
                        .background(
                            RoundedRectangle(cornerRadius: 3)
                                .fill(selected ? Color.primary : Color.clear)
                        )
                        .overlay(
                            RoundedRectangle(cornerRadius: 3)
                                .strokeBorder(
                                    selected ? Color.primary : Color.primary.opacity(0.18),
                                    lineWidth: 1
                                )
                        )
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selected ? .isSelected : [])
                .accessibilityLabel(chip.monoTitle)
            }
        }
    }

    private var customIntervalRow: some View {
        let minutes = settings.scanInterval.customMinutesOrDefault
        let displayValue: Int = {
            switch customUnit {
            case .minutes: return minutes
            case .hours: return max(1, minutes / 60)
            }
        }()

        return HStack(spacing: 10) {
            Text("EVERY")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
                .tracking(0.8)

            HStack(spacing: 0) {
                stepButton("−") { stepCustom(direction: -1) }
                Text("\(displayValue)")
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .monospacedDigit()
                    .frame(minWidth: 36)
                    .padding(.vertical, 6)
                    .overlay(alignment: .leading) {
                        Rectangle().fill(Color.primary.opacity(0.12)).frame(width: 1)
                    }
                    .overlay(alignment: .trailing) {
                        Rectangle().fill(Color.primary.opacity(0.12)).frame(width: 1)
                    }
                stepButton("+") { stepCustom(direction: 1) }
            }
            .overlay(
                RoundedRectangle(cornerRadius: 3)
                    .strokeBorder(Color.primary.opacity(0.18), lineWidth: 1)
            )

            HStack(spacing: 0) {
                unitButton("MIN", unit: .minutes)
                unitButton("HR", unit: .hours)
            }
            .overlay(
                RoundedRectangle(cornerRadius: 3)
                    .strokeBorder(Color.primary.opacity(0.18), lineWidth: 1)
            )

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 4)
                .fill(Color.primary.opacity(0.04))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 4)
                .strokeBorder(Color.primary.opacity(0.18), lineWidth: 1)
        )
    }

    private func stepButton(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12, weight: .medium, design: .monospaced))
                .frame(width: 28, height: 26)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func unitButton(_ title: String, unit: ScanIntervalCustomUnit) -> some View {
        let on = customUnit == unit
        return Button {
            setCustomUnit(unit)
        } label: {
            Text(title)
                .font(.system(size: 10, weight: .medium, design: .monospaced))
                .tracking(0.5)
                .foregroundStyle(on ? Color(nsColor: .windowBackgroundColor) : Color.secondary)
                .padding(.horizontal, 8)
                .padding(.vertical, 7)
                .background(on ? Color.primary : Color.clear)
        }
        .buttonStyle(.plain)
    }

    private func selectIntervalChip(_ chip: ScanIntervalChip) {
        switch chip {
        case .fifteenMinutes:
            settings.scanInterval = .fifteenMinutes
        case .oneHour:
            settings.scanInterval = .oneHour
        case .sixHours:
            settings.scanInterval = .sixHours
        case .twelveHours:
            settings.scanInterval = .twelveHours
        case .off:
            settings.scanInterval = .manual
        case .custom:
            let minutes = settings.lastCustomMinutes
            customUnit = ScanIntervalCustomUnit.preferred(forMinutes: minutes)
            settings.scanInterval = .custom(minutes: minutes)
        }
        store.restartTimer()
    }

    private func stepCustom(direction: Int) {
        let current = settings.scanInterval.customMinutesOrDefault
        let next = ScanIntervalOption.steppedCustomMinutes(
            from: current,
            unit: customUnit,
            direction: direction
        )
        settings.scanInterval = .custom(minutes: next)
        // Keep unit coherent when stepping across the hour boundary.
        if customUnit == .minutes, next >= 60, next % 60 == 0 {
            customUnit = .hours
        } else if customUnit == .hours, next < 60 {
            customUnit = .minutes
        }
        store.restartTimer()
    }

    private func setCustomUnit(_ unit: ScanIntervalCustomUnit) {
        guard customUnit != unit else { return }
        let current = settings.scanInterval.customMinutesOrDefault
        let converted: Int
        switch unit {
        case .minutes:
            // Show minute ladder; if currently multi-hour, land on 60.
            converted = current >= 60 ? 60 : ScanIntervalOption.clampMinutes(current)
        case .hours:
            let hours = max(1, Int((Double(current) / 60.0).rounded()))
            converted = ScanIntervalOption.clampMinutes(hours * 60)
        }
        customUnit = unit
        settings.scanInterval = .custom(minutes: converted)
        store.restartTimer()
    }

    private func settingsSectionLabel(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(.secondary)
            .tracking(1.0)
            .textCase(.uppercase)
    }
}
