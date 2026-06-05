import SwiftUI

@main
struct TokensMenuBarApp: App {
    var body: some Scene {
        MenuBarExtra {
            Text("Spike OK")
                .padding()
                .frame(width: 220, height: 120)
        } label: {
            Image(systemName: "chart.bar.xaxis")
        }
        .menuBarExtraStyle(.window)
    }
}
