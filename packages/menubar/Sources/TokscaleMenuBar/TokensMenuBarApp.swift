import SwiftUI

@main
struct TokensMenuBarApp: App {
    @StateObject private var model = MenuBarModel()

    var body: some Scene {
        MenuBarExtra {
            TokensPopoverView(model: model)
                .onAppear { model.refreshOnOpenIfNeeded() }
        } label: {
            MenuBarLabelView(image: model.menuBarImage)
        }
        .menuBarExtraStyle(.window)
    }
}
