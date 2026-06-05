import AppKit
import SwiftUI
import TokscaleMenuBarCore

struct MenuBarLabelView: View {
    let summary: TokscaleSummary?

    var body: some View {
        if let image = renderedBadge() {
            Image(nsImage: image)
        } else {
            Image(systemName: "chart.bar.xaxis")
        }
    }

    @MainActor
    private func renderedBadge() -> NSImage? {
        guard let constrained = summary.flatMap({ QuotaGlance.mostConstrained(in: $0.quota) }) else {
            return nil
        }
        let badge = MenuBarBadge(
            remainingFraction: constrained.remainingPercent / 100,
            percent: Int(constrained.remainingPercent.rounded()),
            color: color(for: constrained.remainingPercent)
        )
        let renderer = ImageRenderer(content: badge)
        renderer.scale = NSScreen.main?.backingScaleFactor ?? 2
        guard let image = renderer.nsImage else {
            return nil
        }
        image.isTemplate = false
        return image
    }

    private func color(for remaining: Double) -> Color {
        switch QuotaGlance.urgency(remainingPercent: remaining) {
        case .healthy: return .green
        case .warning: return .yellow
        case .critical: return .red
        case .depleted: return .gray
        }
    }
}

private struct MenuBarBadge: View {
    let remainingFraction: Double
    let percent: Int
    let color: Color

    var body: some View {
        HStack(spacing: 4) {
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(color.opacity(0.30))
                    .frame(width: 22, height: 6)
                Capsule()
                    .fill(color)
                    .frame(width: 22 * min(max(remainingFraction, 0), 1), height: 6)
            }
            Text("\(percent)%")
                .font(.system(size: 11, weight: .semibold))
                .monospacedDigit()
                .foregroundStyle(color)
        }
        .padding(.horizontal, 1)
        .frame(height: 16)
    }
}
