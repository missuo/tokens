import SwiftUI
import TokscaleMenuBarCore

struct MenuBarLabelView: View {
    let summary: TokscaleSummary?

    var body: some View {
        if let constrained = summary.flatMap({ QuotaGlance.mostConstrained(in: $0.quota) }) {
            let color = color(for: constrained.remainingPercent)
            HStack(spacing: 4) {
                MenuBarMiniBar(remainingFraction: constrained.remainingPercent / 100, color: color)
                Text("\(Int(constrained.remainingPercent.rounded()))%")
                    .font(.system(size: 11, weight: .semibold))
                    .monospacedDigit()
                    .foregroundStyle(color)
            }
        } else {
            Image(systemName: "chart.bar.xaxis")
        }
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

private struct MenuBarMiniBar: View {
    let remainingFraction: Double
    let color: Color

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(color.opacity(0.25))
                Capsule()
                    .fill(color)
                    .frame(width: geo.size.width * min(max(remainingFraction, 0), 1))
            }
        }
        .frame(width: 22, height: 6)
    }
}
