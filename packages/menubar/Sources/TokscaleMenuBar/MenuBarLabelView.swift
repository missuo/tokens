import AppKit
import SwiftUI
import TokscaleMenuBarCore

struct MenuBarLabelView: View {
    let image: NSImage?

    var body: some View {
        if let image {
            Image(nsImage: image)
        } else {
            Image(systemName: "chart.bar.xaxis")
        }
    }
}

enum MenuBarBadgeRenderer {
    @MainActor
    static func image(for summary: TokscaleSummary?) -> NSImage? {
        guard let constrained = summary.flatMap({ QuotaGlance.mostConstrained(in: $0.quota) }) else {
            return nil
        }
        let badge = MenuBarBadge(
            remainingFraction: constrained.remainingPercent / 100,
            percent: Int(constrained.remainingPercent.rounded()),
            stale: summary?.stale ?? false
        )
        let renderer = ImageRenderer(content: badge)
        renderer.scale = NSScreen.main?.backingScaleFactor ?? 2
        guard let image = renderer.nsImage else {
            return nil
        }
        // Template image: macOS tints it with the adaptive menu-bar color, so the
        // badge stays legible on any wallpaper and in light or dark menu bars.
        image.isTemplate = true
        return image
    }
}

private struct MenuBarBadge: View {
    let remainingFraction: Double
    let percent: Int
    let stale: Bool

    var body: some View {
        HStack(spacing: 4) {
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.black.opacity(0.32))
                    .frame(width: 22, height: 6)
                Capsule()
                    .fill(Color.black)
                    .frame(width: 22 * min(max(remainingFraction, 0), 1), height: 6)
            }
            Text("\(percent)%")
                .font(.system(size: 11, weight: .semibold))
                .monospacedDigit()
                .foregroundStyle(Color.black)
        }
        .padding(.horizontal, 1)
        .frame(height: 16)
        .opacity(stale ? 0.45 : 1.0)
    }
}
