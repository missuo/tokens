import SwiftUI
import TokscaleMenuBarCore

struct MenuBarLabelView: View {
    let summary: TokscaleSummary?

    var body: some View {
        if let constrained = summary.flatMap({ QuotaGlance.mostConstrained(in: $0.quota) }) {
            let remaining = Int(constrained.remainingPercent.rounded())
            HStack(spacing: 3) {
                Image(systemName: "bolt.fill")
                Text("\(remaining)%")
            }
            .foregroundStyle(color(for: constrained.remainingPercent))
        } else {
            Image(systemName: "chart.bar.xaxis")
        }
    }

    private func color(for remaining: Double) -> Color {
        switch QuotaGlance.urgency(remainingPercent: remaining) {
        case .normal: return .primary
        case .warning: return .orange
        case .critical: return .red
        }
    }
}
